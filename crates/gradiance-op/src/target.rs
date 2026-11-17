//! [`Target`]: how an operation's params name a scene object.

use bevy::math::Vec2;
use bevy::reflect::Reflect;
use gradiance_core::ids::StableId;
use serde::Deserialize;
use uuid::Uuid;

/// A reference to one scene object, as a doorway writes it.
///
/// JSON forms: a number is an [`Index`](Self::Index), a UUID string is an
/// [`Id`](Self::Id), the string `"selection"` is [`Selection`](Self::Selection)
/// and `{"at": [x, y]}` is [`At`](Self::At).
///
/// A target is resolved to a [`StableId`] inside `lower`, against a mirror
/// from the [`OpContext`](crate::OpContext) ([`ResolveTarget`]). The intent
/// then holds only `StableId`s (invariant 3).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Reflect)]
#[serde(try_from = "TargetRepr")]
pub enum Target {
    /// The i-th object in id order, in the context's snapshot. A negative
    /// index never resolves: the query verbs return `-1` for "none", so
    /// `(place (nearest-at x y) …)` on an empty scene stays a quiet no-op.
    Index(i64),
    /// The object with this id.
    Id(StableId),
    /// The current selection (resolves only where a doorway publishes a
    /// selection mirror).
    Selection,
    /// The object under this world point.
    At(Vec2),
}

/// The JSON shapes a [`Target`] decodes from.
#[derive(Deserialize)]
#[serde(untagged)]
enum TargetRepr {
    Number(f64),
    Text(String),
    At { at: [f32; 2] },
}

impl TryFrom<TargetRepr> for Target {
    type Error = String;

    fn try_from(repr: TargetRepr) -> Result<Self, String> {
        match repr {
            TargetRepr::Number(n) if n.is_finite() => Ok(Self::Index(n as i64)),
            TargetRepr::Number(n) => Err(format!("{n} is not an index")),
            TargetRepr::Text(text) if text == "selection" => Ok(Self::Selection),
            TargetRepr::Text(text) => Uuid::parse_str(&text)
                .map(|uuid| Self::Id(StableId(uuid)))
                .map_err(|_| format!("`{text}` is not an object id or \"selection\"")),
            TargetRepr::At { at: [x, y] } => Ok(Self::At(Vec2::new(x, y))),
        }
    }
}

/// The editor's selection, as a read-only mirror: the selected objects
/// (bodies and behaviour nodes) in selection order, the first being the
/// primary, and the selected joint. The layer that
/// owns the selection publishes it; a [`Target::Selection`] resolves against
/// it. Objects and a joint are never selected together.
///
/// The Lisp doorway builds its context **without** this mirror (scripting
/// rule 2: a script acting on whatever happens to be selected is not
/// reproducible), so `"selection"` there is a visible error
/// ([`OpContext::selection`](crate::OpContext::selection)).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionMirror {
    /// Selected bodies and nodes, in selection order.
    pub objects: Vec<StableId>,
    /// The selected joint, if any.
    pub joint: Option<StableId>,
}

/// A mirror that can resolve a [`Target`] of one object kind. The layer that
/// owns the scene snapshot implements it; `None` means "names nothing".
pub trait ResolveTarget {
    /// The id `target` names in this mirror, if any.
    fn resolve(&self, target: &Target) -> Option<StableId>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decode(value: serde_json::Value) -> Result<Target, String> {
        serde_json::from_value(value).map_err(|e| e.to_string())
    }

    #[test]
    fn every_json_form_decodes() {
        assert_eq!(decode(json!(2)), Ok(Target::Index(2)));
        // Query verbs return floats; an index truncates as the verbs always did.
        assert_eq!(decode(json!(2.9)), Ok(Target::Index(2)));
        assert_eq!(decode(json!(-1.0)), Ok(Target::Index(-1)));
        assert_eq!(decode(json!("selection")), Ok(Target::Selection));
        assert_eq!(
            decode(json!({"at": [1.5, -2.0]})),
            Ok(Target::At(Vec2::new(1.5, -2.0)))
        );
        let id = Uuid::from_u128(7);
        assert_eq!(decode(json!(id.to_string())), Ok(Target::Id(StableId(id))));
    }

    #[test]
    fn a_wrong_form_is_a_decode_error() {
        assert!(decode(json!("that one")).is_err());
        assert!(decode(json!(true)).is_err());
        assert!(decode(json!({"near": [0, 0]})).is_err());
    }
}
