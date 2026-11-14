import pathlib
p = pathlib.Path("crates/rvim-headless/src/lib.rs")
s = p.read_text()
anchor = """    pub fn assert_clean(&self) -> Result<(), UiError> {"""
method = """    /// Deterministic text + per-cell attribute-id + palette dump for golden tests.
    pub fn snapshot_with_attrs(&self) -> String {
        let Some(frame) = &self.last else {
            return String::new();
        };
        let grid = frame.composite();
        let mut out = String::new();
        for row in 0..frame.size.rows {
            out.push_str(&format!("{row:02} |{}|\n", grid.row_text(row)));
            let ids = (0..frame.size.cols)
                .map(|col| grid.cell(row, col).map_or(0, |cell| cell.attrs))
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            out.push_str(&format!("   {ids}\n"));
        }
        out.push_str("\n# palette\n");
        for (index, attr) in grid.attrs.iter().enumerate() {
            out.push_str(&format!(
                "{index}: fg={:?} bg={:?} sp={:?} flags={}\n",
                attr.fg,
                attr.bg,
                attr.sp,
                attr.flags.bits()
            ));
        }
        out
    }

    pub fn assert_clean(&self) -> Result<(), UiError> {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("headless patched")
p = pathlib.Path("crates/rvim-headless/tests/golden.rs")
s = p.read_text()
# add test
s += '''

#[test]
fn golden_attribute_palette_80x24() {
    let path = std::env::temp_dir().join("rvim-golden-attrs.txt");
    std::fs::write(&path, b"TODO normal\\n").unwrap();
    let mut editor = Editor::new(Size::new(24, 80));
    editor.swap_enabled = false;
    editor.open_file(&path).unwrap();
    editor.execute_ex("syntax match Todo /TODO/");
    editor.execute_ex("highlight Todo ctermfg=red cterm=bold");
    editor.execute_ex("set laststatus=2");
    let frame = editor.render();
    let mut backend = HeadlessBackend::new(Size::new(24, 80));
    backend.init().unwrap();
    backend.present(&frame).unwrap();
    compare_golden("attrs_80x24.txt", &backend.snapshot_with_attrs());
    let _ = std::fs::remove_file(path);
}
'''
p.write_text(s)
print("golden patched")
