//! Stacking order within one depth band: the newest body is drawn on top.
//!
//! Bodies on the same band have coplanar front faces, so the GPU's depth
//! test drew whichever came first, while a click picked by another rule:
//! a body moved onto another could look on top and still not be
//! clickable there (brief J-2). The hit-test now picks the newest body of
//! a band first (`interaction::tools::bodies_at_sorted`; ids are minted in
//! creation order), and this system draws the same order: it sinks each
//! older body of a band a few millimetres along `z`, so the newest sits at
//! the band's front face and depth testing does the rest.
//!
//! Derived and render-only: the offset lives in `Transform::translation.z`,
//! which no pose, record, command or physics read uses (poses are 2D;
//! avian keeps `z` when it writes a pose back).

use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::Body;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::shape::ShapeDef;

/// The `z` step between two bodies of one band: above the depth buffer's
/// resolution over the camera's depth slab (about 1 mm), small against a
/// band (a layer is 10 m deep).
pub const STACK_STEP: f32 = 0.004;

/// How many of a band's newest bodies get their own step. Older bodies
/// share the deepest step (an overlap among them falls back to the GPU's
/// order), so a band never sinks more than `STACK_STEP * STACK_DEPTH`.
pub const STACK_DEPTH: usize = 32;

/// The stacking offset of each body, from `(band near, id)` rows: `0` for
/// the newest body of each band, one step deeper for each newer body of
/// the same band, capped at [`STACK_DEPTH`] steps.
pub fn stack_offsets(rows: &[(f32, StableId)]) -> Vec<f32> {
    let mut offsets = vec![0.0; rows.len()];
    let mut order: Vec<usize> = (0..rows.len()).collect();
    // Band, then newest first.
    order.sort_by(|&a, &b| {
        rows[a]
            .0
            .total_cmp(&rows[b].0)
            .then(rows[b].1.0.cmp(&rows[a].1.0))
    });
    let mut rank = 0usize;
    for (i, &row) in order.iter().enumerate() {
        if i > 0 && rows[order[i - 1]].0.to_bits() != rows[row].0.to_bits() {
            rank = 0;
        }
        #[expect(clippy::cast_precision_loss)] // rank <= STACK_DEPTH
        let step = rank.min(STACK_DEPTH) as f32;
        offsets[row] = -STACK_STEP * step;
        rank += 1;
    }
    offsets
}

/// Restacks every body when a body is added or removed or a band changes.
pub fn stack_bodies(
    added: Query<(), Added<Body>>,
    changed: Query<(), (With<Body>, Changed<DepthBand>)>,
    mut removed: RemovedComponents<Body>,
    mut bodies: Query<(&ShapeDef, &DepthBand, &StableId, &mut Transform), With<Body>>,
) {
    let removed_any = removed.read().count() > 0;
    if added.is_empty() && changed.is_empty() && !removed_any {
        return;
    }
    let rows: Vec<(f32, StableId)> = bodies
        .iter()
        .filter(|(shape, ..)| !shape.contains_half_plane())
        .map(|(_, band, id, _)| (band.sanitized().near.value(), *id))
        .collect();
    let offsets = stack_offsets(&rows);
    let mut by_id: Vec<(StableId, f32)> = rows.iter().map(|r| r.1).zip(offsets).collect();
    by_id.sort_by_key(|(id, _)| id.0);
    for (shape, _, id, mut transform) in &mut bodies {
        let z = if shape.contains_half_plane() {
            0.0
        } else {
            by_id
                .binary_search_by_key(&id.0, |(id, _)| id.0)
                .map_or(0.0, |i| by_id[i].1)
        };
        // Write only on a change: a write marks the pose changed.
        if transform.translation.z.to_bits() != z.to_bits() {
            transform.translation.z = z;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_body_of_each_band_sits_at_its_front() {
        let ids: Vec<StableId> = (0..4).map(|_| StableId::new()).collect();
        let rows = vec![
            (0.0, ids[0]),
            (0.0, ids[2]),
            (2.5, ids[1]),
            (0.0, ids[1]),
            (2.5, ids[3]),
        ];
        let offsets = stack_offsets(&rows);
        assert_eq!(offsets[1], 0.0, "newest of band 0");
        assert_eq!(offsets[3], -STACK_STEP);
        assert_eq!(offsets[0], -2.0 * STACK_STEP);
        assert_eq!(offsets[4], 0.0, "newest of band 2.5");
        assert_eq!(offsets[2], -STACK_STEP);
    }

    #[test]
    fn a_crowded_band_sinks_no_deeper_than_the_cap() {
        let rows: Vec<(f32, StableId)> = (0..100).map(|_| (0.0, StableId::new())).collect();
        let offsets = stack_offsets(&rows);
        #[expect(clippy::cast_precision_loss)]
        let floor = -STACK_STEP * STACK_DEPTH as f32;
        assert!(offsets.iter().all(|z| *z >= floor - 1e-6));
        assert_eq!(offsets[99], 0.0);
    }
}
