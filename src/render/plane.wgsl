// Infinite-plane surface: standard PBR (matte, shadow-catching), rendered
// opaque with depth so plane/plane and plane/body intersections resolve as
// crisp seams (alpha blending made the megaquads sort-order dependent —
// the whole background used to flip color as the view rotated). The
// "infinite" read comes from a horizon fog toward the backdrop color, and
// crossing the camera inside the solid half-space dissolves the surface
// (dithered discard) so orbiting through never blinds the view. A faint
// trace line marks where the plane crosses the authoring plane (z = 0).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}

struct PlaneData {
    // xyz = outward plane normal (world), w = plane offset.
    data: vec4<f32>,
}
struct FadeData {
    // x = unused (was the alpha floor), y/z = horizon fade start/end
    // distance from the eye.
    data: vec4<f32>,
}
struct HorizonData {
    // Linear-space backdrop color the horizon fog fades toward (matches
    // the camera clear color, so the surface dissolves into the sky).
    color: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> plane: PlaneData;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<uniform> fade: FadeData;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> horizon: HorizonData;

// 4x4 Bayer threshold — the opaque pass has no blending, so the inside
// reveal is a screen-door dissolve (brief: the band is ~50 world px of
// camera travel).
fn bayer4(p: vec2<f32>) -> f32 {
    var thresholds = array<f32, 16>(
        0.03125, 0.53125, 0.15625, 0.65625,
        0.78125, 0.28125, 0.90625, 0.40625,
        0.21875, 0.71875, 0.09375, 0.59375,
        0.96875, 0.46875, 0.84375, 0.34375,
    );
    let x = u32(p.x) % 4u;
    let y = u32(p.y) % 4u;
    return thresholds[y * 4u + x];
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let eye = view.world_position;

    // Inside reveal: signed distance of the eye from the plane; once the
    // camera crosses into the solid half-space the surface dissolves so
    // the scene stays visible instead of becoming a wall.
    let side = dot(eye, plane.data.xyz) - plane.data.w;
    let reveal = smoothstep(-40.0, 10.0, side);
    if reveal < bayer4(in.position.xy) {
        discard;
    }

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

    // Authoring-plane trace: a faint screen-width line where this plane
    // crosses the interaction plane (world z = 0) — the authored surface
    // line, the anchor that keeps tilted views navigable. Parallel planes
    // (the back plane) never satisfy |z| ≈ 0, so they draw no line.
    let dz = in.world_position.z;
    let aa = max(fwidth(dz), 1e-4);
    let line = 1.0 - smoothstep(aa, aa * 3.0, abs(dz));
    let lum = dot(out.color.rgb, vec3(0.2126, 0.7152, 0.0722));
    let tint = select(vec3(1.0), vec3(0.0), lum > 0.4);
    let traced = mix(out.color.rgb, tint, 0.22 * line);

    // Horizon fog: fade toward the backdrop color with eye distance, so
    // the surface reads as unbounded and never shows the quad edge.
    let dist = length(in.world_position.xyz - eye);
    let fog = smoothstep(fade.data.y, fade.data.z, dist);
    out.color = vec4(mix(traced, horizon.color.rgb, fog), 1.0);

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
