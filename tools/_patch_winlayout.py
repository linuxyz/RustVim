import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "screenchar" => {"""
arm = """            "winlayout" => {
                let page = self
                    .editor
                    .view
                    .tabs
                    .get(&self.editor.view.current_tab);
                let value = page
                    .map(|page| layout_value(&page.root))
                    .unwrap_or_else(|| Value::list(Vec::new()));
                Ok(Some(value))
            }
            "screenchar" => {"""
assert anchor in s
s = s.replace(anchor, arm, 1)
s += '''

fn layout_value(node: &rvim_view::layout::LayoutNode) -> Value {
    use rvim_view::layout::LayoutNode;
    match node {
        LayoutNode::Leaf(id) => Value::list(vec![
            Value::string("leaf"),
            Value::Number(rvim_types::Id::raw(*id) as i64),
        ]),
        LayoutNode::Split { dir, children, .. } => {
            let label = match dir {
                rvim_view::layout::SplitDir::Horizontal => "row",
                rvim_view::layout::SplitDir::Vertical => "col",
            };
            let mut items = vec![Value::string(label)];
            items.extend(children.iter().map(layout_value));
            Value::list(items)
        }
    }
}
'''
p.write_text(s)
print("patched")
