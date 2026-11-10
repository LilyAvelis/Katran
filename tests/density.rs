use katran::density::{DEFAULT_GALLERY_COLUMNS, DEFAULT_STREAM_ROWS, adjust_density};

#[test]
fn density_steps_are_discrete_and_bounded() {
    assert_eq!(adjust_density(DEFAULT_GALLERY_COLUMNS, 1), 5);
    assert_eq!(adjust_density(DEFAULT_STREAM_ROWS, 1), 4);
    assert_eq!(adjust_density(5, -1), 4);
    assert_eq!(adjust_density(1, -1), 1);
    assert_eq!(adjust_density(12, 1), 12);
    assert_eq!(adjust_density(1, i8::MAX), 12);
    assert_eq!(adjust_density(12, i8::MIN), 1);
}
