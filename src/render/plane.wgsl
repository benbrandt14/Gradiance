// Infinite-plane surface: standard PBR (matte, shadow-catching), with a
// horizon fade so the quad reads as an unbounded surface (visible under
// perspective) and an inside fade so orbiting the camera through the
// solid half-space reveals the scene instead of a wall.

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
    // x = alpha floor when the camera is inside the solid,
    // y/z = horizon fade start/end distance from the eye.
    data: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> plane: PlaneData;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<uniform> fade: FadeData;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    let eye = view.world_position;

    // Inside fade: signed distance of the eye from the plane; negative =
    // inside the solid half-space.
    let side = dot(eye, plane.data.xyz) - plane.data.w;
    let inside = mix(fade.data.x, 1.0, smoothstep(-40.0, 10.0, side));

    // Horizon fade: distance from the eye to this surface point.
    let dist = length(in.world_position.xyz - eye);
    let horizon = 1.0 - smoothstep(fade.data.y, fade.data.z, dist);

    out.color.a = out.color.a * inside * horizon;
    return out;
}
