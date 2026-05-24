import re

with open('src/geometry/extrusion.rs', 'r') as f:
    content = f.read()

# Update create_extruded_mesh to only return Mesh
pattern = re.compile(r'fn create_extruded_mesh\(\n    path: &lyon::path::Path,\n    groups: CollisionGroups,\n\) -> \(Mesh, StandardMaterial\) \{.*?\n\}', re.DOTALL)

def replacement_func(match):
    text = match.group(0)
    text = text.replace(') -> (Mesh, StandardMaterial) {', ') -> Mesh {')
    text = text.replace('    let color = generate_color_from_layers(min_i, max_i);\n    let material = StandardMaterial {\n        base_color: color,\n        perceptual_roughness: 0.8,\n        metallic: 0.1,\n        ..default()\n    };\n\n    (mesh, material)', '    mesh')
    return text

content = pattern.sub(replacement_func, content)

# Update extrusion_system
extrusion_sys_pattern = r'''
    for \(entity, path, groups, mesh_handle, mat_handle\) in query\.iter\(\) \{
        let \(mesh, material\) = create_extruded_mesh\(&path\.0, \*groups\);

        // Remove old assets to prevent leaks
        meshes\.remove\(&mesh_handle\.0\);
        materials\.remove\(&mat_handle\.0\);

        let new_mesh_handle = meshes\.add\(mesh\);
        let new_mat_handle = materials\.add\(material\);

        commands
            \.entity\(entity\)
            \.insert\(\(Mesh3d\(new_mesh_handle\), MeshMaterial3d\(new_mat_handle\)\)\);
    \}'''

extrusion_sys_repl = r'''
    for (entity, path, groups, mesh_handle, mat_handle) in query.iter() {
        let mesh = create_extruded_mesh(&path.0, *groups);

        // Remove old mesh to prevent leaks
        meshes.remove(&mesh_handle.0);
        // We do NOT remove or replace the material, so user modifications are preserved

        let new_mesh_handle = meshes.add(mesh);

        commands
            .entity(entity)
            .insert(Mesh3d(new_mesh_handle));
    }'''

content = re.sub(extrusion_sys_pattern, extrusion_sys_repl, content)

# Update initial mesh generation to still create the material
initial_pattern = r'''
    let \(mesh, material\) = create_extruded_mesh\(&path\.0, groups\);
    let mesh_handle = meshes\.add\(mesh\);
    let mat_handle = materials\.add\(material\);'''

initial_repl = r'''
    let mesh = create_extruded_mesh(&path.0, groups);
    let mesh_handle = meshes.add(mesh);

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
    let mat_handle = materials.add(material);'''

content = re.sub(initial_pattern, initial_repl, content)


with open('src/geometry/extrusion.rs', 'w') as f:
    f.write(content)
