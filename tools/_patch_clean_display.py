import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
old = """    fn display_col_for(&self, line: &[u8], col: u32) -> usize {
        let tabstop = self.display_tabstop();
        rvim_types::unicode::display_width(line, 0, tabstop).min(usize::MAX)
            * 0
            + rvim_types::unicode::display_width(
                &line[..(col as usize).min(line.len())],
                0,
                tabstop,
            )
    }"""
new = """    fn display_col_for(&self, line: &[u8], col: u32) -> usize {
        let tabstop = self.display_tabstop();
        rvim_types::unicode::display_width(
            &line[..(col as usize).min(line.len())],
            0,
            tabstop,
        )
    }"""
assert old in s
s = s.replace(old, new, 1)
s = s.replace("""        let start_col = if index > 0 { start + 0 } else { start };
        let prefix_cells = prefix;
        let content_start = start_col;""", """        let prefix_cells = if index > 0 { prefix } else { 0 };
        let content_start = start;""", 1)
p.write_text(s)
print("patched")
