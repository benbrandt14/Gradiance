import re

with open('src/geometry/extrusion.rs', 'r') as f:
    content = f.read()

# Fix initial pattern where I missed one instance
content = content.replace(
    'let (mesh, material) = create_extruded_mesh(&path, groups);',
    'let mesh = create_extruded_mesh(&path, groups);'
)

# Fix early return
content = content.replace(
    '''        return (
            Mesh::new(
                PrimitiveTopology::TriangleList,
                bevy::render::render_asset::RenderAssetUsages::default(),
            ),
            StandardMaterial::default(),
        );''',
    '''        return Mesh::new(
            PrimitiveTopology::TriangleList,
            bevy::render::render_asset::RenderAssetUsages::default(),
        );'''
)

# Fix bottom return
content = content.replace('    (mesh, material)\n}', '    mesh\n}')

# Remove unused material handle
content = content.replace('mut materials: ResMut<Assets<StandardMaterial>>,\n', '')
content = content.replace('mat_handle', '_mat_handle')

with open('src/geometry/extrusion.rs', 'w') as f:
    f.write(content)
