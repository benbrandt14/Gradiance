struct ToonShaderMaterial {
    color: vec4<f32>,
    sun_dir: vec3<f32>,
    sun_color: vec4<f32>,
    camera_pos: vec3<f32>,
    ambient_color: vec4<f32>,
    steps: u32,
};

@group(2) @binding(0)
var<uniform> material: ToonShaderMaterial;
@group(2) @binding(1)
var base_color_texture: texture_2d<f32>;
@group(2) @binding(2)
var base_color_sampler: sampler;

#import bevy_pbr::forward_io::VertexOutput

@fragment
fn fragment (in: VertexOutput) -> @location(0) vec4<f32> {
    // if model doesn't have uvs, this lets it still render.
    #ifdef VERTEX_UVS_A
        let uv = in.uv;
    #else
        let uv = vec2(1.0, 1.0);
    #endif

    let base_color = material.color * textureSample(base_color_texture, base_color_sampler, uv);
    let normal = normalize(in.world_normal);
    let sun_dir = normalize(material.sun_dir);

    // Use absolute value to light backfaces (since culling is off)
    // Plus a small bias to prevent perfectly flat shading
    let n_dot_l = abs(dot(sun_dir, normal));
    var light_intensity = 0.0;

    let bands = f32(material.steps);
    var x = n_dot_l * bands;
    x = ceil(x); // Use ceil for sharper bands
    light_intensity = x / bands;

    // Add a rim light effect for better definition
    let view_dir_rim = normalize(material.camera_pos - in.world_position.xyz);
    let n_dot_v = 1.0 - abs(dot(normal, view_dir_rim));
    let rim = smoothstep(0.6, 1.0, n_dot_v) * 0.5;

    light_intensity += rim;

    let light = light_intensity * material.sun_color;

    let view_vec = material.camera_pos - in.world_position.xyz;
    var view_dir = vec3<f32>(0.0, 0.0, 1.0);
    if (length(view_vec) > 0.001) {
        view_dir = normalize(view_vec);
    }

    let half_vector = normalize(sun_dir + view_dir);
    let n_dot_h = dot(normal, half_vector);
    let glossiness = 32.0;
    let specular_intensity = pow(n_dot_h, glossiness * glossiness);

    let specular_intensity_smooth = smoothstep(0.005, 0.01, specular_intensity);
    let specular = specular_intensity_smooth * vec4<f32>(0.9, 0.9 ,0.9 ,1.0);

    return base_color * (light + material.ambient_color + specular);
}
