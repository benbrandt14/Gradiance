//! A phase plot (brief S-2): height against x of a launched projectile,
//! paired sample by sample, is a parabola.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, headless_app, step};
use bevy::prelude::*;
use gradiance::domain::motion::InitialMotion;
use gradiance::op::invoke;
use gradiance::prelude::*;
use gradiance::signal::SignalBus;
use gradiance::ui::plot::{Series, phase_points};
use gradiance::units::{AngularVelocity, Dimension, UnitPrefs, Velocity2};
use serde_json::json;

/// Least-squares fit of `y = a x² + b x + c`; returns the residual RMS.
fn parabola_residual(points: &[[f64; 2]]) -> f64 {
    // Normal equations for the three coefficients.
    let mut m = [[0.0_f64; 3]; 3];
    let mut v = [0.0_f64; 3];
    for &[x, y] in points {
        let row = [x * x, x, 1.0];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += row[i] * row[j];
            }
            v[i] += row[i] * y;
        }
    }
    // Gaussian elimination.
    for col in 0..3 {
        let pivot = m[col][col];
        for r in (col + 1)..3 {
            let f = m[r][col] / pivot;
            for c in col..3 {
                m[r][c] -= f * m[col][c];
            }
            v[r] -= f * v[col];
        }
    }
    let mut k = [0.0_f64; 3];
    for r in (0..3).rev() {
        let mut sum = v[r];
        for c in (r + 1)..3 {
            sum -= m[r][c] * k[c];
        }
        k[r] = sum / m[r][r];
    }
    let n = points.len() as f64;
    (points
        .iter()
        .map(|&[x, y]| (y - (k[0] * x * x + k[1] * x + k[2])).powi(2))
        .sum::<f64>()
        / n)
        .sqrt()
}

#[test]
fn a_projectile_plots_as_a_parabola() {
    let mut app = headless_app();
    let mut ball = box_record(Vec2::ZERO, 0.2, 0.2);
    ball.shape = ShapeDef::Circle { radius: 0.1 };
    ball.motion = Some(InitialMotion {
        velocity: Velocity2(Vec2::new(4.0, 6.0)),
        spin: AngularVelocity(0.0),
    });
    app.world_mut()
        .write_message(SpawnBodyIntent { record: ball });
    app.update();
    for (name, quantity) in [("px", "pos-x"), ("py", "height")] {
        invoke(
            app.world_mut(),
            "signal.bind",
            json!({"name": name, "source": quantity, "sink": "plot", "target": -1, "of": [0], "min": null, "max": null}),
        )
        .unwrap();
    }
    step(&mut app, 70);
    let bus = app.world().resource::<SignalBus>();
    let unit = UnitPrefs::default().display(Dimension::Dimensionless);
    let series = |name: &str| {
        let (_, entry) = bus.entries().find(|(n, _)| *n == name).expect(name);
        Series {
            name: "s",
            unit,
            values: entry.history(),
            times: entry.times(),
        }
    };
    let (x, y) = (series("px"), series("py"));
    let points = phase_points(&x, &y);
    assert!(points.len() > 30, "{} paired samples", points.len());
    let span = points.iter().map(|p| p[1]).fold(f64::MIN, f64::max)
        - points.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
    let residual = parabola_residual(&points);
    assert!(
        residual < 0.01 * span,
        "residual {residual} m against a {span} m rise and fall"
    );
}
