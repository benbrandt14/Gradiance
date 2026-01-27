#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::forward_io::VertexOutput

struct ToonMaterial {
    color: vec4<f32>,
    steps: u32,
    border_width: f32,
};

@group(2) @binding(0) var<uniform> material: ToonMaterial;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Simple directional light (fixed direction for now, simulating a sun)
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.5));
    let N = normalize(in.world_normal);

    // NdotL calculation
    let NdotL = max(dot(N, light_dir), 0.0);

    // Toon shading: Quantize intensity into steps
    let steps = f32(material.steps);
    // Add a small bias to avoid stepping at exactly 0.0 or 1.0 artifacts
    let quantized = floor(NdotL * steps + 0.01) / steps;

    // Ambient light component
    let ambient = 0.3;
    let intensity = ambient + quantized * (1.0 - ambient);

    return vec4<f32>(material.color.rgb * intensity, material.color.a);
}
