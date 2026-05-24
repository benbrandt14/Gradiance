import re

with open('src/geometry/extrusion.rs', 'r') as f:
    content = f.read()

# Fix the generate_color_from_layers function not existing
content = content.replace(
    'let color = generate_color_from_layers(min_i, max_i);',
    'let hue = min_i as f32 * 30.0;\n    let color = Color::hsl(hue, 0.8, 0.5);'
)

# Remove the unused material in create_extruded_mesh
pattern = r'''
    // Material
    // Color based on layer index \(min_i\) to distinguish zones
    let hue = min_i as f32 \* 30\.0;
    let color = Color::hsl\(hue, 0\.8, 0\.5\);
    let material = StandardMaterial \{
        base_color: color,
        perceptual_roughness: 0\.5,
        metallic: 0\.0,
        // Disabled culling to ensure visibility
        cull_mode: None,
        double_sided: true,
        \.\.default\(\)
    \};'''

content = re.sub(pattern, '', content)

with open('src/geometry/extrusion.rs', 'w') as f:
    f.write(content)
