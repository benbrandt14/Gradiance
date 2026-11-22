//! The tools as operations (brief U-9): `tool.select`, `tool.box`, … one
//! per built-in tool, so the F3 search, the tools pie and the status bar
//! reach a tool through the registry like any other operation, and a script
//! or a remote client can pick one (`(box-tool)`).
//!
//! Choosing a tool is editor state, not an edit: each operation is
//! [`Seam::Editor`], records no undo step, and lowers to a [`ToolRequest`]
//! that [`apply_tool_requests`] turns into the `NextState<ToolState>` a
//! palette click sets. The declared chord is the tool's existing letter
//! (`interaction::input`'s map is unchanged; the chord is metadata the
//! status bar and the menus show). The thruster, the chain and the sketch
//! have no letter: the editor keymap is not settled, and a new tool adds no key.
//!
//! Also here: each tool's **hint table** ([`hints`]), the mouse and key
//! gestures the status bar shows for the active tool. A tool's gestures are
//! documented in its module; the table is the short form.

use bevy::prelude::*;
use gradiance_core::states::ToolState;
use gradiance_op::{
    OpContext, OpError, OpInvokeSet, OpMeta, OpOutput, OpRegistry, Operation, Seam,
};
use serde::Deserialize;

/// A tool change an operation asked for, applied by
/// [`apply_tool_requests`].
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolRequest(pub ToolState);

/// Declares one zero-param operation per tool, the [`TOOL_OPS`] table and
/// the registration function.
macro_rules! tool_ops {
    ($( $ty:ident => $state:expr, $id:literal, $lisp:literal, $menu:literal, $chord:literal, $doc:literal; )*) => {
        $(
            #[doc = $doc]
            #[derive(Debug, Deserialize, Reflect)]
            #[serde(deny_unknown_fields)]
            pub struct $ty {}

            impl Operation for $ty {
                const META: OpMeta = OpMeta {
                    id: $id,
                    lisp: $lisp,
                    doc: $doc,
                    seam: Seam::Editor,
                    menu: Some($menu),
                    // "" = a tool with no key of its own.
                    chord: if $chord.is_empty() { None } else { Some($chord) },
                    undo: None,
                    example: "{}",
                };

                fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
                    Ok(OpOutput::none().message(ToolRequest($state)))
                }
            }
        )*

        /// Every built-in tool and the operation that picks it, in palette
        /// order.
        pub const TOOL_OPS: &[(ToolState, &str)] = &[ $( ($state, $id), )* ];

        fn register_tool_ops(registry: &mut OpRegistry) {
            $( registry.register::<$ty>(); )*
        }
    };
}

tool_ops! {
    UseSelect => ToolState::Select, "tool.select", "select-tool", "Tools/Select", "S",
        "Use the select tool: click to select, drag to move, right-drag to rotate.";
    UseDrag => ToolState::Drag, "tool.drag", "drag-tool", "Tools/Drag", "D",
        "Use the drag tool: pull bodies with a mouse spring while playing, move them while paused.";
    UseBox => ToolState::Box, "tool.box", "box-tool", "Tools/Box", "B",
        "Use the box tool: drag out a rectangle.";
    UseCircle => ToolState::Circle, "tool.circle", "circle-tool", "Tools/Circle", "C",
        "Use the circle tool: press at the centre, drag out the radius.";
    UsePolygon => ToolState::Polygon, "tool.polygon", "polygon-tool", "Tools/Polygon", "P",
        "Use the polygon tool: click the vertices, close at the start.";
    UseGear => ToolState::Gear, "tool.gear", "gear-tool", "Tools/Gear", "",
        "Use the gear tool: press at the centre, drag out the pitch radius; drawn gears mesh.";
    UseWater => ToolState::Water, "tool.water", "water-tool", "Tools/Water", "",
        "Use the water tool: drag out a rectangle; it fills with particle water when the sim plays.";
    UseTexture => ToolState::Texture, "tool.texture", "texture-tool", "Tools/Texture", "",
        "Use the texture tool: drag a body's image to move it, Shift-drag to scale it, right-drag to turn it.";
    UseSketch => ToolState::Sketch, "tool.sketch", "sketch-tool", "Tools/Sketch", "",
        "Use the sketch tool: draw freehand; release makes one body of the stroke.";
    UseHinge => ToolState::Hinge, "tool.hinge", "hinge-tool", "Tools/Hinge", "H",
        "Use the hinge tool: click where two bodies overlap to hinge them, or on one body to pin it.";
    UsePrismatic => ToolState::Slider, "tool.prismatic", "prismatic-tool", "Tools/Prismatic", "R",
        "Use the prismatic tool: press at the anchor, drag out the slide axis.";
    UseSpring => ToolState::Spring, "tool.spring", "spring-tool", "Tools/Spring", "T",
        "Use the spring tool: drag from one anchor to the other.";
    UseRod => ToolState::Rod, "tool.rod", "rod-tool", "Tools/Rod", "L",
        "Use the rod tool: drag from one end to the other.";
    UseChain => ToolState::Chain, "tool.chain", "chain-tool", "Tools/Chain", "",
        "Use the chain tool: drag a path; release lays hinged links along it.";
    UseWeld => ToolState::Weld, "tool.weld", "weld-tool", "Tools/Weld", "W",
        "Use the weld tool: click two overlapping bodies to merge them.";
    UseGround => ToolState::Ground, "tool.ground", "ground-tool", "Tools/Ground", "G",
        "Use the ground tool: click to place a ground plane, drag to tilt it.";
    UseCut => ToolState::Cut, "tool.cut", "cut-tool", "Tools/Cut", "K",
        "Use the cut tool: drag a stroke across the bodies to cut.";
    UseTracer => ToolState::Tracer, "tool.tracer", "tracer-tool", "Tools/Tracer", "N",
        "Use the tracer tool: click a body (or empty space) to place a tracer.";
    UseThruster => ToolState::Thruster, "tool.thruster", "thruster-tool", "Tools/Thruster", "",
        "Use the thruster tool: press on a body where it pushes; a click pushes up, a drag aims the push.";
    UseLaser => ToolState::Laser, "tool.laser", "laser-tool", "Tools/Laser", "",
        "Use the laser tool: press where the laser sits, drag to aim; release over a body to make it the target.";
}

/// The operation that picks `tool`, if it is a built-in.
pub fn tool_op_id(tool: ToolState) -> Option<&'static str> {
    TOOL_OPS
        .iter()
        .find(|(state, _)| *state == tool)
        .map(|(_, id)| *id)
}

/// Registers the tool operations, the request message and its applier.
pub fn install(app: &mut App) {
    register_tool_ops(&mut app.world_mut().resource_mut::<OpRegistry>());
    app.add_message::<ToolRequest>();
    app.add_systems(Update, apply_tool_requests.after(OpInvokeSet));
}

/// Switches to the last requested tool, as a palette click does.
pub fn apply_tool_requests(
    mut requests: MessageReader<ToolRequest>,
    mut next: ResMut<NextState<ToolState>>,
) {
    if let Some(ToolRequest(tool)) = requests.read().last() {
        next.set(*tool);
    }
}

/// One status-bar hint: an input and what it does with the active tool.
pub type Hint = (&'static str, &'static str);

/// The active tool's gestures, shortest form first, for the status bar.
/// A script tool collects clicks (its count is the tool's own).
pub fn hints(tool: ToolState) -> &'static [Hint] {
    match tool {
        ToolState::Select => &[
            ("Click", "select"),
            ("Shift+click", "add or remove"),
            ("Drag", "move"),
            ("Right-drag", "rotate"),
            ("Ctrl+drag", "duplicate"),
            ("Drag empty", "box select"),
        ],
        ToolState::Drag => &[("Drag", "pull (playing) or move (paused)")],
        ToolState::Box => &[("Drag", "draw a box"), ("Esc", "cancel")],
        ToolState::Circle => &[("Press", "centre"), ("Drag", "radius")],
        ToolState::Gear => &[("Press", "centre"), ("Drag", "pitch radius")],
        ToolState::Water => &[("Drag", "water region")],
        ToolState::Texture => &[
            ("Drag", "move image"),
            ("Shift+drag", "scale image"),
            ("Right-drag", "turn image"),
        ],
        ToolState::Polygon => &[
            ("Click", "add a vertex"),
            ("Click the start or Enter", "close"),
            ("Esc", "cancel"),
        ],
        ToolState::Hinge => &[
            ("Click an overlap", "hinge the two bodies"),
            ("Click one body", "pin it to the world"),
        ],
        ToolState::Slider => &[("Press", "anchor"), ("Drag", "slide axis")],
        ToolState::Spring | ToolState::Rod => &[("Drag", "from one end to the other")],
        ToolState::Weld => &[
            ("Click an overlap", "merge the two bodies"),
            ("Click one body", "make it static"),
        ],
        ToolState::Ground => &[
            ("Click", "place"),
            ("Drag", "tilt"),
            ("Ctrl", "snap the tilt"),
        ],
        ToolState::Cut => &[("Drag", "cut along the stroke")],
        ToolState::Tracer => &[("Click a body", "attach"), ("Click empty", "free tracer")],
        ToolState::Thruster => &[("Click a body", "push up"), ("Drag", "aim the push")],
        ToolState::Laser => &[("Drag", "aim the beam"), ("Release on a body", "target it")],
        ToolState::Chain => &[("Drag", "draw the path; release lays the links")],
        ToolState::Sketch => &[("Drag", "draw; end at the start to fill")],
        ToolState::Script(_) => &[("Click", "add a point"), ("Esc or right-click", "cancel")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_tool_has_one_operation_and_hints() {
        let built_in = [
            ToolState::Select,
            ToolState::Drag,
            ToolState::Box,
            ToolState::Circle,
            ToolState::Polygon,
            ToolState::Hinge,
            ToolState::Slider,
            ToolState::Spring,
            ToolState::Rod,
            ToolState::Weld,
            ToolState::Ground,
            ToolState::Cut,
            ToolState::Tracer,
            ToolState::Thruster,
            ToolState::Chain,
            ToolState::Sketch,
            ToolState::Laser,
            ToolState::Gear,
            ToolState::Water,
            ToolState::Texture,
        ];
        assert_eq!(TOOL_OPS.len(), built_in.len());
        for tool in built_in {
            assert!(tool_op_id(tool).is_some(), "{tool:?} has no operation");
            assert!(!hints(tool).is_empty(), "{tool:?} has no hints");
        }
        assert_eq!(tool_op_id(ToolState::Script(0)), None);
    }

    #[test]
    fn a_tool_operation_requests_its_tool() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.add_plugins(gradiance_op::OpPlugin);
        app.init_state::<ToolState>();
        install(&mut app);
        let result = gradiance_op::invoke(app.world_mut(), "tool.box", gradiance_op::json!({}));
        assert!(result.is_ok(), "{result:?}");
        app.update();
        app.update();
        assert_eq!(
            *app.world().resource::<State<ToolState>>().get(),
            ToolState::Box
        );
    }
}
