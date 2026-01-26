use gradiance::input::editable_shape::{generate_shape_components, ShapeType};
use bevy::prelude::*;

#[test]
fn test_box_valid() {
    let shape = ShapeType::Box { width: 1.0, height: 1.0 };
    let result = generate_shape_components(&shape);
    assert!(result.is_some(), "Valid box should return components");
}

#[test]
fn test_box_invalid_zero() {
    let shape = ShapeType::Box { width: 0.0, height: 1.0 };
    let result = generate_shape_components(&shape);
    assert!(result.is_none(), "Zero width box should not return components");

    let shape = ShapeType::Box { width: 1.0, height: 0.0 };
    let result = generate_shape_components(&shape);
    assert!(result.is_none(), "Zero height box should not return components");
}

#[test]
fn test_box_invalid_negative() {
    let shape = ShapeType::Box { width: -1.0, height: 1.0 };
    let result = generate_shape_components(&shape);
    assert!(result.is_none(), "Negative width box should not return components");

    let shape = ShapeType::Box { width: 1.0, height: -1.0 };
    let result = generate_shape_components(&shape);
    assert!(result.is_none(), "Negative height box should not return components");
}

#[test]
fn test_circle_valid() {
    let shape = ShapeType::Circle { radius: 1.0 };
    let result = generate_shape_components(&shape);
    assert!(result.is_some(), "Valid circle should return components");
}

#[test]
fn test_circle_invalid_zero() {
    let shape = ShapeType::Circle { radius: 0.0 };
    let result = generate_shape_components(&shape);
    assert!(result.is_none(), "Zero radius circle should not return components");
}

#[test]
fn test_circle_invalid_negative() {
    let shape = ShapeType::Circle { radius: -1.0 };
    let result = generate_shape_components(&shape);
    assert!(result.is_none(), "Negative radius circle should not return components");
}

#[test]
fn test_polygon_valid() {
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(0.0, 1.0),
    ];
    let shape = ShapeType::Polygon { points };
    let result = generate_shape_components(&shape);
    assert!(result.is_some(), "Valid polygon (3 points) should return components");
}

#[test]
fn test_polygon_invalid_too_few_points() {
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
    ];
    let shape = ShapeType::Polygon { points };
    let result = generate_shape_components(&shape);
    assert!(result.is_none(), "Polygon with 2 points should not return components");
}
