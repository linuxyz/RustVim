import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace('''                } else if ch == ':' {\n                    self.reindent_case_label();\n                }''','''                } else if ch == ':' {\n                    self.reindent_case_label();\n                    self.reindent_jump_label();\n                }''',1)
anchor = """    fn reindent_case_label(&mut self) {"""
method = """    fn reindent_jump_label(&mut self) {
        let buffer_id = self.current_buffer_id();
        if !self.options.buffer_bool(buffer_id, "cindent") {
            return;
        }
        let cinkeys = self.options.buffer_string(buffer_id, "cinkeys");
        if !cinkeys.contains(':') {
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
        let text = String::from_utf8_lossy(&line[indent_len..]);
        let Some(label) = text.strip_suffix(':') else {
            return;
        };
        if label.is_empty()
            || !label
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            || !label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return;
        }
        let width = self
            .options
            .buffer_number(buffer_id, "shiftwidth")
            .max(0) as usize;
        let width = if width == 0 {
            self.options.buffer_number(buffer_id, "tabstop").max(1) as usize
        } else {
            width
        };
        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        let desired = match cino_amount(&cino, 'L', width) {
            Some(amount) if amount >= 0 => indent_len.saturating_sub(amount as usize),
            _ => 0,
        };
        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        let indent = if expandtab {
            " ".repeat(desired)
        } else {
            format!(
                "{}{}",
                "\\t".repeat(desired / width.max(1)),
                " ".repeat(desired % width.max(1))
            )
        };
        let mut new_line = indent.as_bytes().to_vec();
        new_line.extend_from_slice(&line[indent_len..]);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.replace_line(pos.lnum, VimString::from(new_line));
        }
        let col = indent.len() as u32 + (pos.col.saturating_sub(indent_len as u32));
        self.set_insert_cursor(BufPos::new(pos.lnum, col));
    }

    fn reindent_case_label(&mut self) {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
