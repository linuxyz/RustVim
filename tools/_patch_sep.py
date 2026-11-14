import pathlib
p = pathlib.Path("crates/rvim-view/src/layout.rs")
s = p.read_text()
s = s.replace("""pub fn compute_layout(node: &LayoutNode, area: Rect, status_height: u16) -> Vec<LayoutRect> {
    let mut out = Vec::new();
    walk(node, area, status_height, &mut out);
    out
}

fn walk(node: &LayoutNode, area: Rect, status_height: u16, out: &mut Vec<LayoutRect>) {""", """pub fn compute_layout(node: &LayoutNode, area: Rect, status_height: u16) -> Vec<LayoutRect> {
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
) {""", 1)
s = s.replace("""                separator: None,
            });""", """                separator,
            });""", 1)
old = """                    let mut row = area.row;
                    for (index, child) in children.iter().enumerate() {
                        let rows = sizes[index];
                        walk(
                            child,
                            Rect::new(row, area.col, rows, area.cols),
                            status_height,
                            out,
                        );
                        row += rows;
                        if index + 1 < count {
                            row += 1;
                        }
                    }"""
new = """                    let mut row = area.row;
                    for (index, child) in children.iter().enumerate() {
                        let rows = sizes[index];
                        let child_area = Rect::new(row, area.col, rows, area.cols);
                        let separator = (index + 1 < count)
                            .then_some(Rect::new(row + rows, area.col, 1, area.cols));
                        walk(child, child_area, status_height, out, separator);
                        row += rows;
                        if index + 1 < count {
                            row += 1;
                        }
                    }"""
assert old in s
s = s.replace(old, new, 1)
old2 = """                    let mut col = area.col;
                    for (index, child) in children.iter().enumerate() {
                        let cols = sizes[index];
                        walk(
                            child,
                            Rect::new(area.row, col, area.rows, cols),
                            status_height,
                            out,
                        );
                        col += cols;
                        if index + 1 < count {
                            col += 1;
                        }
                    }"""
new2 = """                    let mut col = area.col;
                    for (index, child) in children.iter().enumerate() {
                        let cols = sizes[index];
                        let child_area = Rect::new(area.row, col, area.rows, cols);
                        let separator = (index + 1 < count)
                            .then_some(Rect::new(area.row, col + cols, area.rows, 1));
                        walk(child, child_area, status_height, out, separator);
                        col += cols;
                        if index + 1 < count {
                            col += 1;
                        }
                    }"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("layout patched")
