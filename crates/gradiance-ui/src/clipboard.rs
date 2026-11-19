//! The system clipboard for copy and paste of bodies (brief R-3).
//!
//! The clipboard belongs to egui (`bevy_egui`'s [`EguiClipboard`], present
//! with its default `manage_clipboard` feature), so this layer is the one
//! that touches it. Both directions are a bridge, not a second path:
//!
//! - **Copy.** Ctrl+C and Edit ▸ Copy invoke `edit.copy`, which lowers to a
//!   [`ClipboardWrite`] with the scene text. [`write_clipboard`] puts it on
//!   the system clipboard and keeps it in [`LastCopy`].
//! - **Paste.** Ctrl+V and Edit ▸ Paste write a [`PasteRequest`] (the key
//!   handler cannot read the clipboard). [`paste_clipboard`] reads the
//!   system clipboard, or [`LastCopy`] where there is none (no display, a
//!   headless test), and invokes `edit.paste` with that text and the place.
//!
//! Both systems run only on a frame that has their message, so a frame
//! without a copy or a paste costs nothing. A number field under the pointer
//! keeps Ctrl+C and Ctrl+V (its own copy and paste; the key handler leaves
//! them to it), and a focused text field takes every key.

use bevy::prelude::*;
use bevy_egui::EguiClipboard;
use gradiance_interaction::input::PasteRequest;
use gradiance_op::{InvokeOp, json};
use gradiance_persist::ops::ClipboardWrite;

/// The last scene text this app copied: what a paste reads when the system
/// clipboard cannot be reached.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct LastCopy(pub Option<String>);

/// Installs the clipboard bridge (headless too: the fallback works there).
pub(crate) fn install(app: &mut App) {
    app.init_resource::<LastCopy>();
    app.add_systems(
        Update,
        (
            write_clipboard
                .after(gradiance_op::OpInvokeSet)
                .run_if(on_message::<ClipboardWrite>),
            paste_clipboard
                .after(gradiance_interaction::InteractionSet)
                .before(gradiance_op::OpInvokeSet)
                .run_if(on_message::<PasteRequest>),
        ),
    );
}

/// Puts each copied text on the system clipboard and in [`LastCopy`].
pub fn write_clipboard(
    mut copies: MessageReader<ClipboardWrite>,
    clipboard: Option<ResMut<EguiClipboard>>,
    mut last: ResMut<LastCopy>,
) {
    let Some(ClipboardWrite(text)) = copies.read().last() else {
        return;
    };
    if let Some(mut clipboard) = clipboard {
        clipboard.set_text(text);
    }
    last.0 = Some(text.clone());
}

/// Invokes `edit.paste` with the clipboard's text, once per request.
pub fn paste_clipboard(
    mut requests: MessageReader<PasteRequest>,
    clipboard: Option<ResMut<EguiClipboard>>,
    last: Res<LastCopy>,
    mut invoke: MessageWriter<InvokeOp>,
) {
    let requests: Vec<PasteRequest> = requests.read().copied().collect();
    if requests.is_empty() {
        return;
    }
    let text = clipboard
        .and_then(|mut clipboard| clipboard.get_text())
        .filter(|text| !text.trim().is_empty())
        .or_else(|| last.0.clone());
    let Some(text) = text else {
        warn!("paste: the clipboard holds no text");
        return;
    };
    for request in requests {
        let at: Vec<f32> = request.at.map_or_else(Vec::new, |p| vec![p.x, p.y]);
        invoke.write(InvokeOp::with(
            "edit.paste",
            json!({"text": text.as_str(), "at": at}),
        ));
    }
}
