use katran::{interaction::ShelfTab, tab_layout};

#[test]
fn default_positions_and_stable_reorder() {
    use ShelfTab::*;
    let mut order = tab_layout::default_order();
    assert_eq!(
        order,
        vec![Images, Screenshots, Downloads, Text, Links, Tables]
    );
    assert_eq!(tab_layout::tab_at(&order, 0).unwrap(), All);
    for (index, tab) in order.iter().enumerate() {
        assert_eq!(tab_layout::tab_at(&order, index as u8 + 1).unwrap(), *tab);
    }
    tab_layout::reorder(&mut order, Downloads, 0).unwrap();
    assert_eq!(
        order,
        vec![Downloads, Images, Screenshots, Text, Links, Tables]
    );
    assert_eq!(tab_layout::tab_at(&order, 1).unwrap(), Downloads);
    tab_layout::reorder(&mut order, Downloads, 2).unwrap();
    assert_eq!(order, tab_layout::default_order());
    tab_layout::reorder(&mut order, Screenshots, 1).unwrap();
    assert_eq!(order, tab_layout::default_order());
}

#[test]
fn invalid_requests_preserve_order() {
    use ShelfTab::*;
    let original = tab_layout::default_order();
    for (tab, index) in [(All, 0), (Images, 6), (Downloads, usize::MAX)] {
        let mut order = original.clone();
        assert!(tab_layout::reorder(&mut order, tab, index).is_err());
        assert_eq!(order, original);
    }
    for position in 7..=u8::MAX {
        assert_eq!(
            tab_layout::tab_at(&original, position).unwrap_err().code,
            "invalid_tab_position"
        );
    }
    for mut order in [
        vec![],
        vec![Images],
        vec![Images, Images, Downloads],
        vec![All, Images, Downloads],
        vec![Images, Screenshots, Downloads, All],
    ] {
        let before = order.clone();
        assert!(tab_layout::validate_order(&order).is_err());
        assert!(tab_layout::reorder(&mut order, Images, 0).is_err());
        assert_eq!(order, before);
        assert_eq!(
            tab_layout::tab_at(&order, 0).unwrap_err().code,
            "invalid_tab_order"
        );
    }
}
