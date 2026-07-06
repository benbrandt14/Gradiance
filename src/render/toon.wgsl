// Toon lighting as an extension over the standard PBR pipeline.
//
// The full clustered-light + shadow-map path runs first
// (apply_pbr_lighting); the lit result is then quantized into bands
// relative to the material's albedo, so cast shadows and shading terms
// all band together. bands < 1 bypasses banding (smooth matte PBR).
// Behavioral spec: the original Gradiance toon shader (ceil-banded n·l
// plus rim light), re-derived on top of real shadowed lighting.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

struct ToonParams {
    // x = band count (0 disables banding), y = rim strength, z, w unused.
    data: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> toon: ToonParams;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

    let bands = toon.data.x;
    if bands >= 1.0 {
        let base = max(pbr_input.material.base_color.rgb, vec3(1e-3));
        // How lit the surface is relative to its albedo: shadows, shading,
        // and ambient all move this ratio, so bands track real lighting.
        let lit = out.color.rgb / base;
        let level = clamp(dot(lit, vec3(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
        let banded = ceil(level * bands) / bands;
        // Rim light: brighten grazing view angles (silhouette definition).
        let n_dot_v = saturate(dot(pbr_input.N, pbr_input.V));
        let rim = smoothstep(0.55, 1.0, 1.0 - n_dot_v) * toon.data.y;
        out.color = vec4(base * (banded + rim), out.color.a);
    }

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
