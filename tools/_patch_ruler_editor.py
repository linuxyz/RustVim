import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    pub fn showmode_text(&self) -> Option<String> {"""
method = """    pub fn ruler_text(&self) -> Option<String> {
        if !self.options.get_bool("ruler") || self.mode.is_command() {
            return None;
        }
        if self
            .view
            .current_window()
            .is_some_and(|window| window.status_height() > 0)
        {
            return None;
        }
        let pos = self.cursor();
        Some(format!("{},{}", pos.lnum, pos.col + 1))
    }

    pub fn showmode_text(&self) -> Option<String> {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
