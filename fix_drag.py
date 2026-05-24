import re

with open('src/input/tools/drag_tool.rs', 'r') as f:
    content = f.read()

# Fix the drag tool offset by correctly assigning anchors
pattern = r'''
        // Create Joint
        // TODO: Fix offset drift / "latch to center" issue.
        // The `local_anchor2` calculation (via `calculate_local_anchor`) assumes the body's
        // transform at the moment of the click. If the body moves between the click (input system)
        // and this physics update, or if the hand spawn position (target_pos) is slightly different
        // from the click position, the joint will snap the body.
        // Fix: Capture exact world click pos and body transform in `DragToolData` and use them here.
        let joint = RevoluteJointBuilder::new\(\)
            \.local_anchor1\(Vec2::ZERO\)
            \.local_anchor2\(data\.local_anchor\);'''

replacement = r'''
        // Create Joint
        // The dragged entity is the parent in the joint (since it's added to the hand),
        // so local_anchor2 is for the hand and local_anchor1 is for the dragged entity.
        let joint = RevoluteJointBuilder::new()
            .local_anchor1(data.local_anchor)
            .local_anchor2(Vec2::ZERO);'''

content = re.sub(pattern, replacement, content, flags=re.DOTALL)

with open('src/input/tools/drag_tool.rs', 'w') as f:
    f.write(content)
