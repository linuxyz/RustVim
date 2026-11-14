use rvim_types::WindowId;
use rvim_ui_api::{Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDir {
    /// Children are stacked top-to-bottom.
    Horizontal,
    /// Children are placed left-to-right.
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutNode {
    Leaf(WindowId),
    Split {
        dir: SplitDir,
        children: Vec<LayoutNode>,
        weights: Vec<u16>,
    },
}

impl LayoutNode {
    pub fn leaf(id: WindowId) -> Self {
        Self::Leaf(id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutRect {
    pub window: WindowId,
    pub text: Rect,
    pub status: Rect,
    pub separator: Option<Rect>,
}

/// Compute window rectangles from a tree. Separators occupy one cell between
/// siblings; statusline rows are reserved at the bottom of each leaf.
pub fn compute_layout(node: &LayoutNode, area: Rect, status_height: u16) -> Vec<LayoutRect> {
    let mut out = Vec::new();
    walk(node, area, status_height, &mut out, None);
    out
}

fn walk(
    node: &LayoutNode,
    area: Rect,
    status_height: u16,
    out: &mut Vec<LayoutRect>,
    separator: Option<Rect>,
) {
    match node {
        LayoutNode::Leaf(window) => {
            let status_rows = status_height.min(area.rows.saturating_sub(1));
            let text_rows = area.rows.saturating_sub(status_rows);
            out.push(LayoutRect {
                window: *window,
                text: Rect::new(area.row, area.col, text_rows, area.cols),
                status: Rect::new(area.row + text_rows, area.col, status_rows, area.cols),
                separator,
            });
        }
        LayoutNode::Split {
            dir,
            children,
            weights,
        } => {
            if children.is_empty() {
                return;
            }
            let count = children.len();
            let separators = (count.saturating_sub(1)) as u16;
            match dir {
                SplitDir::Horizontal => {
                    let usable = area.rows.saturating_sub(separators);
                    let sizes = distribute(usable, weights, count);
                    let mut row = area.row;
                    for (index, child) in children.iter().enumerate() {
                        let rows = sizes[index];
                        let child_area = Rect::new(row, area.col, rows, area.cols);
                        let separator = (index + 1 < count).then_some(Rect::new(
                            row + rows,
                            area.col,
                            1,
                            area.cols,
                        ));
                        walk(child, child_area, status_height, out, separator);
                        row += rows;
                        if index + 1 < count {
                            row += 1;
                        }
                    }
                }
                SplitDir::Vertical => {
                    let usable = area.cols.saturating_sub(separators);
                    let sizes = distribute(usable, weights, count);
                    let mut col = area.col;
                    for (index, child) in children.iter().enumerate() {
                        let cols = sizes[index];
                        let child_area = Rect::new(area.row, col, area.rows, cols);
                        let separator = (index + 1 < count).then_some(Rect::new(
                            area.row,
                            col + cols,
                            area.rows,
                            1,
                        ));
                        walk(child, child_area, status_height, out, separator);
                        col += cols;
                        if index + 1 < count {
                            col += 1;
                        }
                    }
                }
            }
        }
    }
}

/// Resize the split so `target`'s text area has exactly `rows` rows when the
/// layout permits it.  Returns whether the tree was modified.
pub fn set_text_height(
    node: &mut LayoutNode,
    area: Rect,
    status_height: u16,
    target: WindowId,
    rows: u16,
) -> bool {
    let LayoutNode::Split {
        dir,
        children,
        weights,
    } = node
    else {
        return false;
    };
    match dir {
        SplitDir::Horizontal => {
            let count = children.len();
            let separators = count.saturating_sub(1) as u16;
            let usable = area.rows.saturating_sub(separators);
            let sizes = distribute(usable, weights, count);
            let mut row = area.row;
            for (index, child) in children.iter_mut().enumerate() {
                let child_rows = sizes[index];
                let child_area = Rect::new(row, area.col, child_rows, area.cols);
                row += child_rows + 1;
                if !contains_window(child, target) {
                    continue;
                }
                if matches!(child, LayoutNode::Leaf(id) if *id == target) {
                    let status_rows = status_height.min(child_rows.saturating_sub(1));
                    let max_target = usable.saturating_sub(1).max(1);
                    let desired = rows.saturating_add(status_rows).clamp(1, max_target);
                    let other = if count == 2 {
                        index ^ 1
                    } else if index + 1 < count {
                        index + 1
                    } else {
                        index - 1
                    };
                    weights[index] = desired;
                    weights[other] = usable.saturating_sub(desired).max(1);
                    return true;
                }
                return set_text_height(child, child_area, status_height, target, rows);
            }
            false
        }
        SplitDir::Vertical => {
            let count = children.len();
            let separators = count.saturating_sub(1) as u16;
            let usable = area.cols.saturating_sub(separators);
            let sizes = distribute(usable, weights, count);
            let mut col = area.col;
            for (index, child) in children.iter_mut().enumerate() {
                let child_cols = sizes[index];
                let child_area = Rect::new(area.row, col, area.rows, child_cols);
                col += child_cols + 1;
                if contains_window(child, target) {
                    return set_text_height(child, child_area, status_height, target, rows);
                }
            }
            false
        }
    }
}

fn contains_window(node: &LayoutNode, target: WindowId) -> bool {
    match node {
        LayoutNode::Leaf(id) => *id == target,
        LayoutNode::Split { children, .. } => {
            children.iter().any(|child| contains_window(child, target))
        }
    }
}

fn distribute(total: u16, weights: &[u16], count: usize) -> Vec<u16> {
    if count == 0 {
        return Vec::new();
    }
    let mut sizes = vec![0u16; count];
    let weight_sum: u32 = (0..count)
        .map(|i| weights.get(i).copied().unwrap_or(1).max(1) as u32)
        .sum();
    let mut used = 0u16;
    for (index, slot) in sizes.iter_mut().enumerate() {
        let weight = weights.get(index).copied().unwrap_or(1).max(1) as u32;
        let size = ((total as u32 * weight) / weight_sum.max(1)) as u16;
        *slot = size.max(1).min(total.saturating_sub(used));
        used = used.saturating_add(*slot);
    }
    // Give remaining cells to the last non-empty child.
    if used < total {
        if let Some(last) = sizes.iter_mut().rev().find(|size| **size > 0) {
            *last += total - used;
        }
    }
    sizes
}

pub fn all_windows(node: &LayoutNode) -> Vec<WindowId> {
    let mut out = Vec::new();
    match node {
        LayoutNode::Leaf(id) => out.push(*id),
        LayoutNode::Split { children, .. } => {
            for child in children {
                out.extend(all_windows(child));
            }
        }
    }
    out
}

pub fn tree_size(node: &LayoutNode) -> Size {
    match node {
        LayoutNode::Leaf(_) => Size::new(1, 1),
        LayoutNode::Split { dir, children, .. } => {
            let mut size = Size::new(0, 0);
            for child in children {
                let child_size = tree_size(child);
                match dir {
                    SplitDir::Horizontal => {
                        size.rows = size.rows.saturating_add(child_size.rows);
                        size.cols = size.cols.max(child_size.cols);
                    }
                    SplitDir::Vertical => {
                        size.cols = size.cols.saturating_add(child_size.cols);
                        size.rows = size.rows.max(child_size.rows);
                    }
                }
            }
            size
        }
    }
}
