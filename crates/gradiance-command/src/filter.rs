//! Body **filters** and group **decompose keys** (brief 4b-1): pure
//! functions over the committed scene ([`SceneSnapshot`]), so the operations
//! that use them (`bodies-where`, `decompose`) and their tests need no app.
//!
//! A filter is a list of clauses that must all hold (AND); `not:` negates
//! one. Union is the caller's: two `bodies-where` lists appended. Clauses:
//!
//! | clause | holds when the body … |
//! |---|---|
//! | `kind:circle` | has that shape kind (`box`, `circle`, `polygon`, `rod`, `plane`, `csg`, `shape`) |
//! | `group:2` | is in the group at index 2 (any nesting depth) |
//! | `grouped` / `ungrouped` | is in some group / in none |
//! | `palette:3` | has palette slot 3 as its fill |
//! | `friction:0.2..0.5` | has a [`BODY_PROPS`](crate::body_prop::BODY_PROPS) value in the range (inclusive) |
//! | `static:1` | has exactly that value |
//! | `not:<clause>` | fails the clause |

use crate::body_prop::BodyProp;
use crate::snapshot::{BodySnapshot, SceneSnapshot};
use gradiance_core::ids::StableId;
use gradiance_domain::group::GroupIndex;

/// One parsed filter clause.
#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    /// Shape kind label (`ShapeDef::kind_label`).
    Kind(String),
    /// Member of the group with this id.
    Group(u32),
    /// In some group.
    Grouped,
    /// In no group.
    Ungrouped,
    /// Fill is this palette slot.
    Palette(usize),
    /// A table property within `[lo, hi]`.
    Range(BodyProp, f64, f64),
    /// The inner clause fails.
    Not(Box<Clause>),
}

/// Parses one clause against `groups` (a `group:i` clause names the group
/// at index `i`). The error says what was wrong, for the caller to show.
pub fn parse_clause(text: &str, groups: &GroupIndex) -> Result<Clause, String> {
    let text = text.trim();
    if let Some(inner) = text.strip_prefix("not:") {
        return Ok(Clause::Not(Box::new(parse_clause(inner, groups)?)));
    }
    match text {
        "grouped" => return Ok(Clause::Grouped),
        "ungrouped" => return Ok(Clause::Ungrouped),
        _ => {}
    }
    let Some((key, value)) = text.split_once(':') else {
        return Err(format!("`{text}` is not a filter clause (key:value)"));
    };
    let number = |v: &str| {
        v.trim()
            .parse::<f64>()
            .ok()
            .filter(|n| !n.is_nan())
            .ok_or_else(|| format!("`{v}` in `{text}` is not a number"))
    };
    match key {
        "kind" => Ok(Clause::Kind(value.to_owned())),
        "group" => {
            let i = number(value)?;
            usize::try_from(i as i64)
                .ok()
                .and_then(|i| groups.groups.get(i))
                .map(|g| Clause::Group(g.id))
                .ok_or_else(|| format!("no group {i} (the scene has {})", groups.len()))
        }
        "palette" => Ok(Clause::Palette(number(value)? as usize)),
        prop => {
            let Some(prop) = BodyProp::named(prop) else {
                return Err(format!(
                    "`{prop}` is not a filter key (kind, group, palette, grouped, ungrouped, not, or a property: {})",
                    BodyProp::names()
                ));
            };
            if let Some((lo, hi)) = value.split_once("..") {
                Ok(Clause::Range(prop, number(lo)?, number(hi)?))
            } else {
                let v = number(value)?;
                Ok(Clause::Range(prop, v, v))
            }
        }
    }
}

impl Clause {
    /// Whether `body` satisfies the clause.
    pub fn holds(&self, body: &BodySnapshot) -> bool {
        match self {
            Self::Kind(kind) => body.shape.kind_label() == kind,
            Self::Group(id) => body.groups.contains(id),
            Self::Grouped => !body.groups.is_empty(),
            Self::Ungrouped => body.groups.is_empty(),
            Self::Palette(slot) => {
                gradiance_domain::appearance::palette_slot(body.appearance.fill) == Some(*slot)
            }
            Self::Range(prop, lo, hi) => {
                let v = prop.read(body);
                let tolerance = 1e-6 * v.abs().max(1.0);
                v >= lo.min(*hi) - tolerance && v <= lo.max(*hi) + tolerance
            }
            Self::Not(inner) => !inner.holds(body),
        }
    }
}

/// The indices (id order) of the bodies every clause holds for.
pub fn matching(scene: &SceneSnapshot, clauses: &[Clause]) -> Vec<usize> {
    scene
        .bodies
        .iter()
        .enumerate()
        .filter(|(_, b)| clauses.iter().all(|c| c.holds(b)))
        .map(|(i, _)| i)
        .collect()
}

/// How `decompose` splits a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecomposeKey {
    /// One part per distinct fill colour.
    Color,
    /// One part per shape kind.
    Kind,
    /// One part per joint-connected island (joints between members only).
    Islands,
    /// No parts: the group is dissolved.
    Dissolve,
}

impl DecomposeKey {
    /// The key called `name`.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "color" | "colour" => Some(Self::Color),
            "kind" => Some(Self::Kind),
            "islands" => Some(Self::Islands),
            "dissolve" => Some(Self::Dissolve),
            _ => None,
        }
    }
}

/// Splits `members` into parts by `key`, in order of first appearance.
pub fn decompose(
    scene: &SceneSnapshot,
    members: &[StableId],
    key: DecomposeKey,
) -> Vec<Vec<StableId>> {
    let bodies: Vec<&BodySnapshot> = members.iter().filter_map(|id| scene.get(*id)).collect();
    match key {
        DecomposeKey::Dissolve => Vec::new(),
        DecomposeKey::Color => partition(&bodies, |a, b| a.appearance.fill == b.appearance.fill),
        DecomposeKey::Kind => {
            partition(&bodies, |a, b| a.shape.kind_label() == b.shape.kind_label())
        }
        DecomposeKey::Islands => {
            // Union-find over the members, joined by every joint whose two
            // ends are both members.
            let ids: Vec<StableId> = bodies.iter().map(|b| b.id).collect();
            let mut root: Vec<usize> = (0..ids.len()).collect();
            for (a, b) in &scene.joint_links {
                let (Some(a), Some(b)) = (
                    ids.iter().position(|id| id == a),
                    b.and_then(|b| ids.iter().position(|id| *id == b)),
                ) else {
                    continue;
                };
                let (ra, rb) = (find(&mut root, a), find(&mut root, b));
                root[ra.max(rb)] = ra.min(rb);
            }
            let mut parts: Vec<(usize, Vec<StableId>)> = Vec::new();
            for (i, id) in ids.iter().enumerate() {
                let r = find(&mut root, i);
                match parts.iter_mut().find(|(k, _)| *k == r) {
                    Some((_, part)) => part.push(*id),
                    None => parts.push((r, vec![*id])),
                }
            }
            parts.into_iter().map(|(_, part)| part).collect()
        }
    }
}

/// The root of `i` in a union-find forest (with path halving).
fn find(root: &mut [usize], mut i: usize) -> usize {
    while root[i] != i {
        root[i] = root[root[i]];
        i = root[i];
    }
    i
}

/// Groups `bodies` by an equivalence, first appearance first.
fn partition(
    bodies: &[&BodySnapshot],
    same: impl Fn(&BodySnapshot, &BodySnapshot) -> bool,
) -> Vec<Vec<StableId>> {
    let mut parts: Vec<(&BodySnapshot, Vec<StableId>)> = Vec::new();
    for body in bodies {
        match parts.iter_mut().find(|(first, _)| same(first, body)) {
            Some((_, part)) => part.push(body.id),
            None => parts.push((body, vec![body.id])),
        }
    }
    parts.into_iter().map(|(_, part)| part).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian2d::prelude::Friction;
    use bevy::prelude::Vec2;
    use gradiance_domain::appearance::{Appearance, PALETTE, Rgba};
    use gradiance_domain::depth::DepthBand;
    use gradiance_domain::props::BodyPhysics;
    use gradiance_domain::shape::ShapeDef;
    use gradiance_op::{OpContext, OpIdSource};

    fn id(n: u64) -> StableId {
        OpContext::new(OpIdSource::Sequence(n)).new_id()
    }

    fn body(n: u64, shape: ShapeDef, fill: Rgba, groups: Vec<u32>, friction: f32) -> BodySnapshot {
        BodySnapshot {
            id: id(n),
            physics: BodyPhysics {
                friction: Friction::new(friction),
                ..BodyPhysics::default()
            },
            pos: Vec2::ZERO,
            rot: 0.0,
            shape,
            touching: 0,
            appearance: Appearance {
                fill,
                ..Appearance::default()
            },
            depth: DepthBand::default(),
            groups,
            fluid: None,
        }
    }

    fn scene() -> SceneSnapshot {
        let circle = ShapeDef::Circle { radius: 0.5 };
        let square = ShapeDef::Box {
            width: 1.0,
            height: 1.0,
        };
        let (red, blue) = (PALETTE[0].color, PALETTE[1].color);
        SceneSnapshot {
            bodies: vec![
                body(1, circle.clone(), red, vec![5], 0.1),
                body(2, square.clone(), red, vec![5], 0.3),
                body(3, circle, blue, vec![5, 6], 0.6),
                body(4, square, blue, Vec::new(), 0.3),
            ],
            joint_links: vec![(id(1), Some(id(3))), (id(2), None)],
            ..SceneSnapshot::default()
        }
    }

    fn clauses(scene: &SceneSnapshot, texts: &[&str]) -> Vec<Clause> {
        let groups = scene.group_index();
        texts
            .iter()
            .filter_map(|t| parse_clause(t, &groups).ok())
            .collect()
    }

    #[test]
    fn clauses_compose_by_and_and_negate() {
        let s = scene();
        assert_eq!(matching(&s, &clauses(&s, &["kind:circle"])), vec![0, 2]);
        assert_eq!(matching(&s, &clauses(&s, &["ungrouped"])), vec![3]);
        assert_eq!(matching(&s, &clauses(&s, &["group:1"])), vec![2]);
        assert_eq!(
            matching(&s, &clauses(&s, &["palette:1", "grouped"])),
            vec![2]
        );
        assert_eq!(
            matching(&s, &clauses(&s, &["friction:0.2..0.5"])),
            vec![1, 3]
        );
        assert_eq!(
            matching(&s, &clauses(&s, &["friction:0.3", "not:kind:box"])),
            Vec::<usize>::new()
        );
        assert_eq!(matching(&s, &clauses(&s, &["not:group:0"])), vec![3]);
        assert_eq!(matching(&s, &[]), vec![0, 1, 2, 3]);
    }

    #[test]
    fn a_bad_clause_says_why() {
        let s = scene();
        let g = s.group_index();
        assert!(parse_clause("group:9", &g).is_err_and(|e| e.contains("no group 9")));
        assert!(parse_clause("mass:1", &g).is_err_and(|e| e.contains("not a filter key")));
        assert!(parse_clause("friction:x", &g).is_err_and(|e| e.contains("not a number")));
        assert!(parse_clause("circle", &g).is_err());
    }

    #[test]
    fn decompose_splits_by_colour_kind_and_islands() {
        let s = scene();
        let members = vec![id(1), id(2), id(3)];
        assert_eq!(
            decompose(&s, &members, DecomposeKey::Color),
            vec![vec![id(1), id(2)], vec![id(3)]]
        );
        assert_eq!(
            decompose(&s, &members, DecomposeKey::Kind),
            vec![vec![id(1), id(3)], vec![id(2)]]
        );
        assert_eq!(
            decompose(&s, &members, DecomposeKey::Islands),
            vec![vec![id(1), id(3)], vec![id(2)]]
        );
        assert_eq!(
            decompose(&s, &members, DecomposeKey::Dissolve),
            Vec::<Vec<StableId>>::new()
        );
        assert_eq!(DecomposeKey::named("colour"), Some(DecomposeKey::Color));
    }
}
