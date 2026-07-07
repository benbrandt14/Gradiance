//! Align & distribute operations over a selection (PowerPoint-style).
//!
//! Pure math over `(id, pose, world AABB)` tuples: callers gather the
//! tuples, this module returns the `TransformChange` batch, and the
//! ordinary command path applies it as one undo step.

use crate::command::intent::TransformChange;
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use bevy::math::Vec2;

/// One selection member: identity, pose, and world-space bounds.
pub type AlignItem = (StableId, PosRot, (Vec2, Vec2));

/// The align/distribute operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignOp {
    /// Left edges to the selection's leftmost edge.
    Left,
    /// Right edges to the selection's rightmost edge.
    Right,
    /// Top edges to the selection's topmost edge.
    Top,
    /// Bottom edges to the selection's bottommost edge.
    Bottom,
    /// Centers onto the selection's mean vertical axis.
    CenterX,
    /// Centers onto the selection's mean horizontal axis.
    CenterY,
    /// Equal horizontal gaps between AABB centers, order preserved.
    DistributeX,
    /// Equal vertical gaps between AABB centers, order preserved.
    DistributeY,
}

/// Computes the pose changes for `op` (empty when nothing moves).
pub fn align_changes(items: &[AlignItem], op: AlignOp) -> Vec<TransformChange> {
    if items.len() < 2 {
        return Vec::new();
    }
    let deltas: Vec<Vec2> = match op {
        AlignOp::Left => {
            let edge = fold_min(items, |(min, _)| min.x);
            items
                .iter()
                .map(|(_, _, (min, _))| Vec2::new(edge - min.x, 0.0))
                .collect()
        }
        AlignOp::Right => {
            let edge = fold_max(items, |(_, max)| max.x);
            items
                .iter()
                .map(|(_, _, (_, max))| Vec2::new(edge - max.x, 0.0))
                .collect()
        }
        AlignOp::Bottom => {
            let edge = fold_min(items, |(min, _)| min.y);
            items
                .iter()
                .map(|(_, _, (min, _))| Vec2::new(0.0, edge - min.y))
                .collect()
        }
        AlignOp::Top => {
            let edge = fold_max(items, |(_, max)| max.y);
            items
                .iter()
                .map(|(_, _, (_, max))| Vec2::new(0.0, edge - max.y))
                .collect()
        }
        AlignOp::CenterX => {
            let mean = mean_center(items).x;
            items
                .iter()
                .map(|(_, _, b)| Vec2::new(mean - center(*b).x, 0.0))
                .collect()
        }
        AlignOp::CenterY => {
            let mean = mean_center(items).y;
            items
                .iter()
                .map(|(_, _, b)| Vec2::new(0.0, mean - center(*b).y))
                .collect()
        }
        AlignOp::DistributeX => distribute(items, |b| center(b).x, |d| Vec2::new(d, 0.0)),
        AlignOp::DistributeY => distribute(items, |b| center(b).y, |d| Vec2::new(0.0, d)),
    };

    items
        .iter()
        .zip(deltas)
        .filter(|(_, delta)| delta.length() > 1e-4)
        .map(|((id, old, _), delta)| TransformChange {
            id: *id,
            old: *old,
            new: PosRot {
                pos: old.pos + delta,
                rot: old.rot,
            },
        })
        .collect()
}

fn center(bounds: (Vec2, Vec2)) -> Vec2 {
    (bounds.0 + bounds.1) / 2.0
}

fn mean_center(items: &[AlignItem]) -> Vec2 {
    items.iter().map(|(_, _, b)| center(*b)).sum::<Vec2>() / items.len() as f32
}

fn fold_min(items: &[AlignItem], f: impl Fn((Vec2, Vec2)) -> f32) -> f32 {
    items.iter().map(|(_, _, b)| f(*b)).fold(f32::MAX, f32::min)
}

fn fold_max(items: &[AlignItem], f: impl Fn((Vec2, Vec2)) -> f32) -> f32 {
    items.iter().map(|(_, _, b)| f(*b)).fold(f32::MIN, f32::max)
}

/// Even spacing of AABB centers between the two extremes along one axis.
fn distribute(
    items: &[AlignItem],
    key: impl Fn((Vec2, Vec2)) -> f32,
    delta: impl Fn(f32) -> Vec2,
) -> Vec<Vec2> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        key(items[a].2)
            .partial_cmp(&key(items[b].2))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let first = key(items[order[0]].2);
    let last = key(items[order[order.len() - 1]].2);
    let step = (last - first) / (items.len() - 1) as f32;
    let mut deltas = vec![Vec2::ZERO; items.len()];
    for (slot, &index) in order.iter().enumerate() {
        let target = first + step * slot as f32;
        deltas[index] = delta(target - key(items[index].2));
    }
    deltas
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // asserting untouched coordinates exactly
mod tests {
    use super::*;

    fn item(id: StableId, x: f32, y: f32, half: f32) -> AlignItem {
        (
            id,
            PosRot {
                pos: Vec2::new(x, y),
                rot: 0.0,
            },
            (Vec2::new(x - half, y - half), Vec2::new(x + half, y + half)),
        )
    }

    #[test]
    fn align_left_moves_everything_to_the_leftmost_edge() {
        let items = [
            item(StableId::new(), 0.0, 0.0, 10.0),
            item(StableId::new(), 50.0, 20.0, 5.0),
        ];
        let changes = align_changes(&items, AlignOp::Left);
        assert_eq!(changes.len(), 1, "the leftmost body stays put");
        assert!((changes[0].new.pos.x - (-10.0 + 5.0)).abs() < 1e-4);
        assert_eq!(changes[0].new.pos.y, 20.0, "only X moves");
    }

    #[test]
    fn distribute_spaces_centers_evenly_and_keeps_extremes() {
        let a = StableId::new();
        let b = StableId::new();
        let c = StableId::new();
        let items = [
            item(a, 0.0, 0.0, 5.0),
            item(b, 10.0, 0.0, 5.0),
            item(c, 100.0, 0.0, 5.0),
        ];
        let changes = align_changes(&items, AlignOp::DistributeX);
        assert_eq!(changes.len(), 1, "only the middle body moves");
        assert_eq!(changes[0].id, b);
        assert!((changes[0].new.pos.x - 50.0).abs() < 1e-4);
    }
}
