// Infinite-ground surface: standard PBR (matte, shadowed), fading out when
// the camera is inside the solid half-space so orbiting "through" the
// ground reveals the scene instead of a wall.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}

struct GroundPlane {
    // xy = outward surface normal (world), z = plane offset, w = alpha floor
    // when the camera is inside the solid.
    data: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> ground: GroundPlane;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    // Signed height of the eye above the surface (world XY plane math —
    // the sandbox plane). Negative = inside the solid: fade the ground so
    // the view is never swallowed by it.
    let eye = view.world_position;
    let side = dot(eye.xy, ground.data.xy) - ground.data.z;
    let fade = mix(ground.data.w, 1.0, smoothstep(-40.0, 10.0, side));
    out.color.a = out.color.a * fade;
    return out;
}
