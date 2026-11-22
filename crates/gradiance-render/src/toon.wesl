// Toon lighting as an extension over the standard PBR pipeline.
//
// The full clustered-light + shadow-map path runs first
// (apply_pbr_lighting); the lit result, relative to the material's albedo,
// then indexes a colour ramp (brief 4b-6) whose tone tints the fill, so
// cast shadows and shading terms all step through the same tones. Band
// edges dither in screen space. Toon off = smooth matte PBR.

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::lights,
};

struct ToonParams {
    // x = toon on (1) or smooth PBR (0), y = rim strength,
    // z = white point (lit/albedo ratio mapped to the top of the ramp),
    // w = quantized-specular strength.
    data: vec4<f32>,
}

struct FaceParams {
    // x = relief: how far a front face's normal tilts at its outline
    // (0 = flat), y = the glint dome's slope, z = bevel: how far in from the
    // outline the relief reaches (a fraction of the deepest depth),
    // w = 1 when the face field texture is bound. Briefs 4b-5, 4b-6.
    data: vec4<f32>,
}

struct ToonRamp {
    // The ramp sampled once on the CPU (`ColorRamp::table`): sRGB tones.
    tones: array<vec4<f32>, 128>,
    // x = 1 linear / 0 constant, y = dither width (lighting levels),
    // z = screen pixels per dither cell, w = toon on.
    params: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> toon: ToonParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> face: FaceParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var<uniform> ramp: ToonRamp;
// The body's face field (`geometry::face_field`): rg = direction toward the
// nearest outline (body-local, [-1, 1] as [0, 1]), b = depth inside as a
// fraction of the deepest. Laid over the face coordinates.
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var field_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var field_sampler: sampler;
// The body's image (brief S-13): rows of the affine map from the face
// coordinates to the image's [0, 1]² (v down). image_u.w = 1 when bound,
// image_v.w = 1 for a raw (unlit) image.
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var<uniform> image_u: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(106) var<uniform> image_v: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(107) var image_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(108) var image_sampler: sampler;

// The world directions of the face's local +x and +y (unit), from screen
// derivatives of the face coordinate `uv` (`geometry::extrusion::face_coords`:
// the outline's bounding box spans [-1, 1]), so they follow the body's
// rotation without the model matrix. Zero off a front or back cap (side
// walls keep their own normals).
struct FaceAxes {
    t: vec3<f32>,
    b: vec3<f32>,
}

fn face_axes(p: vec3<f32>, uv: vec2<f32>, n: vec3<f32>) -> FaceAxes {
    let dpx = dpdx(p);
    let dpy = dpdy(p);
    let dux = dpdx(uv);
    let duy = dpdy(uv);
    let det = dux.x * duy.y - duy.x * dux.y;
    if abs(n.z) < 0.99 || abs(det) < 1e-12 {
        return FaceAxes(vec3(0.0), vec3(0.0));
    }
    // [dP/du dP/dv] = [dP/dx dP/dy] * inverse(d(u,v)/d(x,y)).
    let t = normalize((dpx * duy.y - dpy * dux.y) * sign(det));
    let b = normalize((dpy * dux.x - dpx * duy.x) * sign(det));
    return FaceAxes(t, b);
}

// 4x4 Bayer threshold in [0, 1): the ordered-dither pattern band edges break
// into (the same matrix as plane.wesl's).
fn bayer4(cell: vec2<f32>) -> f32 {
    var thresholds = array<f32, 16>(
        0.03125, 0.53125, 0.15625, 0.65625,
        0.78125, 0.28125, 0.90625, 0.40625,
        0.21875, 0.71875, 0.09375, 0.59375,
        0.96875, 0.46875, 0.84375, 0.34375,
    );
    let x = u32(abs(cell.x)) % 4u;
    let y = u32(abs(cell.y)) % 4u;
    return thresholds[y * 4u + x];
}

// The ramp tone at lighting level `level` in [0, 1]: an index (constant) or
// an index and a lerp (linear) into the sampled table; never a search.
fn ramp_tone(level: f32) -> vec3<f32> {
    let l = clamp(level, 0.0, 1.0);
    if ramp.params.x > 0.5 {
        let pos = l * 127.0;
        let i = min(u32(pos), 126u);
        return mix(ramp.tones[i].rgb, ramp.tones[i + 1u].rgb, pos - f32(i));
    }
    return ramp.tones[min(u32(l * 128.0), 127u)].rgb;
}

// Hard light per channel (`domain::ramp::hard_light`): a tone below 0.5
// multiplies, above it screens; 0.5 leaves `base` as it is.
fn hard_light(base: vec3<f32>, tone: vec3<f32>) -> vec3<f32> {
    let multiply = 2.0 * base * tone;
    let screen = 1.0 - 2.0 * (1.0 - base) * (1.0 - tone);
    return select(screen, multiply, tone < vec3(0.5));
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

    // Face relief (brief 4b-6): light the front cap as a rounded body. The
    // normal tilts toward the nearest outline by a quarter-round profile of
    // the distance to it (the face field), reaching `relief` at the outline
    // and nothing `bevel` in. Without a field, the 4b-5 dome over the face
    // coordinates. Lighting only: the rim stays on the true normal.
    let flat_n = pbr_input.N;
    var axes = FaceAxes(vec3(0.0), vec3(0.0));
    var uv = vec2(0.0);
    @if(VERTEX_UVS_A) {
        uv = in.uv;
        axes = face_axes(in.world_position.xyz, uv, flat_n);
    }
    let field = textureSampleLevel(field_texture, field_sampler, uv * 0.5 + 0.5, 0.0);

    // The image (brief S-13), on the front cap only, inside its rectangle.
    // A toon image is the face's albedo, so the ramp shades it; a raw one
    // goes over the lit result below.
    var raw_image = vec4(0.0);
    if image_u.w > 0.5 && flat_n.z > 0.99 {
        let iuv = vec2(dot(image_u.xy, uv) + image_u.z, dot(image_v.xy, uv) + image_v.z);
        if all(iuv >= vec2(0.0)) && all(iuv <= vec2(1.0)) {
            let c = textureSampleLevel(image_texture, image_sampler, iuv, 0.0);
            if image_v.w > 0.5 {
                raw_image = c;
            } else {
                let b = pbr_input.material.base_color;
                pbr_input.material.base_color = vec4(mix(b.rgb, c.rgb, c.a), b.a);
            }
        }
    }
    let toon_on = toon.data.x > 0.5;
    let relief = face.data.x;
    if toon_on && relief > 0.0 && dot(axes.t, axes.t) > 0.5 {
        var tilt = vec3(0.0);
        var up = 1.0;
        if face.data.w > 0.5 {
            let dir = field.rg * 2.0 - 1.0;
            let len = length(dir);
            let out_of_face = clamp(1.0 - field.b / max(face.data.z, 1e-3), 0.0, 1.0);
            let m = min(relief * out_of_face, 0.97);
            if len > 1e-3 {
                tilt = normalize(axes.t * dir.x + axes.b * dir.y) * m;
                up = sqrt(1.0 - m * m);
            }
        } else {
            tilt = (axes.t * uv.x + axes.b * uv.y) * relief;
        }
        pbr_input.N = normalize(flat_n * up + tilt);
    }

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

    if toon_on {
        let base = max(pbr_input.material.base_color.rgb, vec3(1e-3));
        // How lit the surface is relative to its albedo: shadows, shading,
        // and ambient all move this ratio, so the ramp tracks real lighting.
        let lit = out.color.rgb / base;
        let level = clamp(dot(lit, vec3(0.2126, 0.7152, 0.0722)) / toon.data.z, 0.0, 1.0);
        // Ordered dither at the stop edges: offset the lookup by the Bayer
        // threshold, so within `dither` of an edge the pixels pick one tone
        // or the other in a screen pattern (cells of `params.z` pixels).
        let cell = floor(in.position.xy / ramp.params.z);
        let jitter = (bayer4(cell) - 0.5) * 2.0 * ramp.params.y;
        let tone = ramp_tone(level + jitter);
        // The tone tints the fill by hard light in gamma space, as the
        // ramp editor previews it (`domain::ramp::shade`).
        let toned = pow(hard_light(pow(base, vec3(1.0 / 2.2)), tone), vec3(2.2));
        // Rim light: brighten grazing view angles (silhouette definition).
        let n_dot_v = saturate(dot(flat_n, pbr_input.V));
        let rim = smoothstep(0.55, 1.0, 1.0 - n_dot_v) * toon.data.y;
        // Quantized specular (off in every look since brief 4b-6; the lit
        // patch is a ramp tone): a hard two-step Blinn-Phong glint (a halo
        // inside ~14 degrees of the half vector, a core inside ~7) from the
        // strongest light, in its hue (GPU light colors are premultiplied
        // by lux, so normalize to the peak channel), over a dome on the
        // face (face.y).
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
                let ng = normalize(flat_n + (axes.t * uv.x + axes.b * uv.y) * face.data.y);
                let s = dot(ng, normalize(pbr_input.V + light.direction_to_light));
                let halo = smoothstep(0.9686, 0.9730, s);
                let core = smoothstep(0.9914, 0.9936, s);
                glint = (light.color.rgb / max(key_peak, 1e-4)) * (0.5 * halo + 0.5 * core);
            }
        }
        // Emission rides on top of the ramp: the ramp maps a ratio to the
        // albedo, so a glow folded into `lit` would clamp to the top tone
        // and vanish. Added after, a glowing body reads brighter than any
        // lit one, still flat-toned.
        let glow = pbr_input.material.emissive.rgb;
        out.color = vec4(toned + base * rim + glint * toon.data.w + glow, out.color.a);
    }

    out.color = vec4(mix(out.color.rgb, raw_image.rgb, raw_image.a), out.color.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
