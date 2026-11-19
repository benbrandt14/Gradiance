// Toon lighting as an extension over the standard PBR pipeline.
//
// The full clustered-light + shadow-map path runs first
// (apply_pbr_lighting); the lit result is then quantized into bands
// relative to the material's albedo, so cast shadows and shading terms
// all band together. bands < 1 bypasses banding (smooth matte PBR).
// Behavioral spec: the original Gradiance toon shader (ceil-banded n·l
// plus rim light), re-derived on top of real shadowed lighting.

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::lights,
};

struct ToonParams {
    // x = band count (0 disables banding), y = rim strength,
    // z = white point (lit/albedo ratio mapped to the top band),
    // w = quantized-specular strength.
    data: vec4<f32>,
}

struct FaceParams {
    // x = relief: the dome slope a front face is lit with at its outline
    // (0 = flat), y = the glint dome's slope. Brief 4b-5.
    data: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> toon: ToonParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> face: FaceParams;

// The world-space tilt that reads a front cap as a dome, per unit slope.
//
// `uv` is the mesh's face coordinate (`geometry::extrusion::face_coords`:
// the outline's bounding box spans [-1, 1]); dP/du and dP/dv come from
// screen derivatives, so the tilt follows the body's rotation without the
// model matrix. A flat cap has one normal, so N.L is one band over the
// whole face; tilting the normal by the face coordinate steps the bands
// across it instead. Side walls (and anything not facing +-Z) stay flat.
fn face_tilt(p: vec3<f32>, uv: vec2<f32>, n: vec3<f32>) -> vec3<f32> {
    let dpx = dpdx(p);
    let dpy = dpdy(p);
    let dux = dpdx(uv);
    let duy = dpdy(uv);
    let det = dux.x * duy.y - duy.x * dux.y;
    if abs(n.z) < 0.99 || abs(det) < 1e-12 {
        return vec3(0.0);
    }
    // [dP/du dP/dv] = [dP/dx dP/dy] * inverse(d(u,v)/d(x,y)).
    let t = normalize((dpx * duy.y - dpy * dux.y) * sign(det));
    let b = normalize((dpy * dux.x - dpx * duy.x) * sign(det));
    return t * uv.x + b * uv.y;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color =
        alpha_discard(
            pbr_input.material.flags,
            pbr_input.material.alpha_cutoff,
            pbr_input.material.base_color,
        );

    // Face relief: light the front cap as a shallow dome (brief 4b-5).
    // Lighting only; the rim stays on the true normal.
    let flat_n = pbr_input.N;
    var tilt = vec3(0.0);
    @if(VERTEX_UVS_A) {
        tilt = face_tilt(in.world_position.xyz, in.uv, flat_n);
    }
    let bands = toon.data.x;
    if bands >= 1.0 && face.data.x > 0.0 {
        pbr_input.N = normalize(flat_n + tilt * face.data.x);
    }

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

    if bands >= 1.0 {
        let base = max(pbr_input.material.base_color.rgb, vec3(1e-3));
        // How lit the surface is relative to its albedo: shadows, shading,
        // and ambient all move this ratio, so bands track real lighting.
        let lit = out.color.rgb / base;
        let level = clamp(dot(lit, vec3(0.2126, 0.7152, 0.0722)) / toon.data.z, 0.0, 1.0);
        let banded = ceil(level * bands) / bands;
        // Rim light: brighten grazing view angles (silhouette definition).
        let n_dot_v = saturate(dot(flat_n, pbr_input.V));
        let rim = smoothstep(0.55, 1.0, 1.0 - n_dot_v) * toon.data.y;
        // Quantized specular: a hard two-step Blinn-Phong glint (a halo
        // inside ~14 degrees of the half vector, a core inside ~7) from the
        // strongest light, in its hue (GPU light colors are premultiplied
        // by lux, so normalize to the peak channel). It reads the front cap
        // as a dome (face.y), so the glint lands on every face, toward the
        // light; with a flat normal and a head-on view it never would. The
        // diffuse path is fully matte (roughness 1, reflectance 0), so this
        // is the *only* view-dependent shading term: deliberate and
        // discrete instead of a smooth sheen that drags whole bands with
        // the camera.
        var glint = vec3(0.0);
        if toon.data.w > 0.001 {
            let n = min(lights.n_directional_lights, 4u);
            var key = 0u;
            var key_peak = -1.0;
            for (var i = 0u; i < n; i++) {
                let c = lights.directional_lights[i].color;
                let peak = max(c.r, max(c.g, c.b));
                if peak > key_peak {
                    key = i;
                    key_peak = peak;
                }
            }
            if n > 0u {
                let light = lights.directional_lights[key];
                let ng = normalize(flat_n + tilt * face.data.y);
                let s = dot(ng, normalize(pbr_input.V + light.direction_to_light));
                let halo = smoothstep(0.9686, 0.9730, s);
                let core = smoothstep(0.9914, 0.9936, s);
                glint = (light.color.rgb / max(key_peak, 1e-4)) * (0.5 * halo + 0.5 * core);
            }
        }
        // Emission rides on top of the bands: banding quantizes a ratio to
        // the albedo, so a glow folded into `lit` would clamp to the top
        // band and vanish. Added after, a glowing body reads brighter than
        // any lit one, still flat-toned.
        let glow = pbr_input.material.emissive.rgb;
        out.color = vec4(base * (banded + rim) + glint * toon.data.w + glow, out.color.a);
    }

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
