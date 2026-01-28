use gradiance::ui::context_menu::calculate_layer_distribution;

#[test]
fn test_distribute_even() {
    // 2 entities, 2 layers (0, 1) -> 1 each
    let masks = calculate_layer_distribution(2, 0, 1);
    assert_eq!(masks.len(), 2);
    assert_eq!(masks[0], 1 << 0);
    assert_eq!(masks[1], 1 << 1);
}

#[test]
fn test_distribute_remainder() {
    // 3 entities, 4 layers (0, 1, 2, 3) -> 2, 1, 1
    // Total 4. layers_per_object = 1. remainder = 1.
    // Entity 0 gets 2 layers (0, 1).
    // Entity 1 gets 1 layer (2).
    // Entity 2 gets 1 layer (3).
    let masks = calculate_layer_distribution(3, 0, 3);
    assert_eq!(masks.len(), 3);
    assert_eq!(masks[0], (1 << 0) | (1 << 1));
    assert_eq!(masks[1], 1 << 2);
    assert_eq!(masks[2], 1 << 3);
}

#[test]
fn test_distribute_single() {
    // 1 entity, 3 layers (0, 1, 2).
    let masks = calculate_layer_distribution(1, 0, 2);
    assert_eq!(masks.len(), 1);
    // Mask: 111 binary -> 7.
    assert_eq!(masks[0], (1 << 0) | (1 << 1) | (1 << 2));
}

#[test]
fn test_distribute_fewer_layers_than_entities() {
    // 3 entities, 1 layer (0).
    // Total 1. layers_per_object = 0. remainder = 1.
    // Entity 0 gets 1 layer (0).
    // Entity 1 gets 0 layers.
    // Entity 2 gets 0 layers.
    let masks = calculate_layer_distribution(3, 0, 0);
    assert_eq!(masks.len(), 3);
    assert_eq!(masks[0], 1 << 0);
    assert_eq!(masks[1], 0);
    assert_eq!(masks[2], 0);
}

#[test]
fn test_distribute_empty() {
    let masks = calculate_layer_distribution(0, 0, 10);
    assert_eq!(masks.len(), 0);
}
