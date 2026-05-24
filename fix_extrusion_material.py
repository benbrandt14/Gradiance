import re

with open('src/geometry/extrusion.rs', 'r') as f:
    content = f.read()

# Fix the hook to properly generate the material and mesh
pattern = r'''
    let mesh = create_extruded_mesh\(&path, groups\);

    // Asset Registration
    let mesh_handle = \{
        let mut meshes = world\.resource_mut::<Assets<Mesh>>\(\);
        meshes\.add\(mesh\)
    \};

    let material_handle = \{
        let mut materials = world\.resource_mut::<Assets<StandardMaterial>>\(\);
        materials\.add\(material\)
    \};'''

replacement = r'''
    let mesh = create_extruded_mesh(&path, groups);

    // Parse memberships to generate initial color
    let memberships = groups.memberships.bits();
    let mut min_i = 32;
    let mut max_i = 0;
    for i in 0..32 {
        if (memberships & (1 << i)) != 0 {
            if i < min_i {
                min_i = i;
            }
            if i > max_i {
                max_i = i;
            }
        }
    }
    if min_i == 32 {
        min_i = 0;
    }

    let color = generate_color_from_layers(min_i, max_i);
    let material = StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.8,
        metallic: 0.1,
        ..default()
    };

    // Asset Registration
    let mesh_handle = {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        meshes.add(mesh)
    };

    let material_handle = {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        materials.add(material)
    };'''

content = re.sub(pattern, replacement, content, flags=re.DOTALL)

with open('src/geometry/extrusion.rs', 'w') as f:
    f.write(content)
