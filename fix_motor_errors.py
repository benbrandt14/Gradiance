import re

with open('src/physics/controllers.rs', 'r') as f:
    content = f.read()

# Fix the method calls and variable names
content = content.replace(
    '(get_angle(entity, joint.parent, rev.local_anchor1, rev.local_anchor2, &transforms), min, max)',
    '(get_angle(entity, joint.parent, rev.local_anchor1(), rev.local_anchor2(), &transforms), min, max)'
)

content = content.replace(
    '(get_dist(entity, joint.parent, prism.local_anchor1, prism.local_anchor2, &transforms), min, max)',
    '(get_dist(entity, joint.parent, prism.local_anchor1(), prism.local_anchor2(), &transforms), min, max)'
)

# For GenericJoint, the previous script erroneously replaced `g.` with `rev.` and `prism.` inside the generic block.
# I will just replace those specific lines directly.
content = content.replace(
    '(get_angle(entity, joint.parent, rev.local_anchor1(), rev.local_anchor2(), &transforms), min, max)',
    '(get_angle(entity, joint.parent, rev.local_anchor1(), rev.local_anchor2(), &transforms), min, max)',
    1 # Keep first one
)
# We need a more targeted replace for the GenericJoint ones
pattern1 = r'''
                    // Revolute
                    let \(min, max\) = if let Some\(l\) = g\.limits\(JointAxis::AngX\) \{
                        \(l\.min, l\.max\)
                    \} else \{
                        \(-f32::MAX, f32::MAX\)
                    \};
                    \(get_angle\(entity, joint\.parent, rev\.local_anchor1\(\), rev\.local_anchor2\(\), &transforms\), min, max\)'''

repl1 = r'''
                    // Revolute
                    let (min, max) = if let Some(l) = g.limits(JointAxis::AngX) {
                        (l.min, l.max)
                    } else {
                        (-f32::MAX, f32::MAX)
                    };
                    (get_angle(entity, joint.parent, g.local_anchor1(), g.local_anchor2(), &transforms), min, max)'''
content = re.sub(pattern1, repl1, content)


pattern2 = r'''
                    // Prismatic
                    let \(min, max\) = if let Some\(l\) = g\.limits\(JointAxis::LinX\) \{
                        \(l\.min, l\.max\)
                    \} else \{
                        \(-f32::MAX, f32::MAX\)
                    \};
                    \(get_dist\(entity, joint\.parent, prism\.local_anchor1\(\), prism\.local_anchor2\(\), &transforms\), min, max\)'''

repl2 = r'''
                    // Prismatic
                    let (min, max) = if let Some(l) = g.limits(JointAxis::LinX) {
                        (l.min, l.max)
                    } else {
                        (-f32::MAX, f32::MAX)
                    };
                    (get_dist(entity, joint.parent, g.local_anchor1(), g.local_anchor2(), &transforms), min, max)'''
content = re.sub(pattern2, repl2, content)

with open('src/physics/controllers.rs', 'w') as f:
    f.write(content)
