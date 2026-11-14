import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace('''                } else if ch == ';' {\n                    self.reindent_break();\n                }''','''                } else if ch == ';' {\n                    self.reindent_break();\n                } else if ch == '#' {\n                    self.reindent_preprocessor();\n                }''',1)
anchor = """    fn reindent_break(&mut self) {"""
method = """    fn reindent_preprocessor(&mut self) {
        let buffer_id = self.current_buffer_id();
        if !self.options.buffer_bool(buffer_id, "cindent") {
            return;
        }
        let cinkeys = self.options.buffer_string(buffer_id, "cinkeys");
        if !cinkeys.split(',').any(|key| key == "0#" || key == "#") {
            return;
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let indent_len = line
            .iter()
            .take_while(|byte| **byte == b' ' || **byte == b'\\t')
            .count();
        if line.get(indent_len) != Some(&b'#') {
            return;
        }
        let trimmed = line[indent_len..].to_vec();
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.replace_line(pos.lnum, VimString::from(trimmed));
        }
        let col = pos.col.saturating_sub(indent_len as u32);
        self.set_insert_cursor(BufPos::new(pos.lnum, col));
    }

    fn reindent_break(&mut self) {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
