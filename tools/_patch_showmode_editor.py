import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    pub fn showcmd_text(&self) -> Option<String> {"""
method = """    pub fn showmode_text(&self) -> Option<String> {
        if !self.options.get_bool("showmode") {
            return None;
        }
        let text = match &self.mode {
            Mode::Insert(state) => {
                if state.replace {
                    \"-- REPLACE --\"
                } else {
                    \"-- INSERT --\"
                }
            }
            Mode::Visual {
                linewise, block, ..
            } => {
                if *block {
                    \"-- VISUAL BLOCK --\"
                } else if *linewise {
                    \"-- VISUAL LINE --\"
                } else {
                    \"-- VISUAL --\"
                }
            }
            _ => return None,
        };
        Some(text.to_string())
    }

    pub fn showcmd_text(&self) -> Option<String> {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
