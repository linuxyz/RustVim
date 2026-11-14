use rvim_types::WindowId;
use rvim_ui_api::Rect;
use rvim_view::{compute_layout, LayoutNode, SplitDir};

#[test]
fn vertical_split_reserves_separator_and_status() {
    let a = WindowId::from_raw(1);
    let b = WindowId::from_raw(2);
    let tree = LayoutNode::Split {
        dir: SplitDir::Vertical,
        children: vec![LayoutNode::Leaf(a), LayoutNode::Leaf(b)],
        weights: vec![1, 1],
    };
    let rects = compute_layout(&tree, Rect::new(1, 0, 20, 81), 1);
    assert_eq!(rects.len(), 2);
    assert_eq!(rects[0].text.cols + rects[1].text.cols, 80);
    assert_eq!(rects[0].status.rows, 1);
    assert_eq!(rects[1].status.rows, 1);
}
