import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace('''                if ch == '}' {\n                    self.reindent_closing_brace();\n                } else if ch == ':' {\n                    self.reindent_case_label();\n                }''','''                if ch == '}' {\n                    self.reindent_closing_brace();\n                } else if ch == '{' {\n                    self.reindent_opening_brace();\n                } else if ch == ':' {\n                    self.reindent_case_label();\n                }''',1)
anchor = """    fn reindent_case_label(&mut self) {"""
method = """    fn reindent_opening_brace(&mut self) {
        let buffer_id = self.current_buffer_id();
        if !self.options.buffer_bool(buffer_id, "cindent") {
            return;
        }
        let cinkeys = self.options.buffer_string(buffer_id, "cinkeys");
        if !cinkeys.contains('{') {
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
        let trimmed = &line[indent_len..];
        if trimmed != b"{" {
            return;
        }
        let mut depth = 0i32;
        for lnum in 1..pos.lnum {
            if let Some(previous) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
            {
                for byte in previous {
                    match byte {
                        b'{' => depth += 1,
                        b'}' => depth = (depth - 1).max(0),
                        _ => {}
                    }
                }
            }
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
        let desired = if depth == 0 {
            cino_amount(&cino, 'f', width).unwrap_or(0).max(0) as usize
        } else {
            let mut previous_indent = 0usize;
            let mut lnum = pos.lnum.saturating_sub(1);
            while lnum >= 1 {
                if let Some(previous) = self
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(lnum))
                {
                    if !previous.is_empty() {
                        previous_indent = previous
                            .iter()
                            .take_while(|byte| **byte == b' ' || **byte == b'\\t')
                            .count();
                        break;
                    }
                }
                lnum = lnum.saturating_sub(1);
            }
            previous_indent
                + cino_amount(&cino, '{', width)
                    .unwrap_or(0)
                    .max(0) as usize
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
        new_line.extend_from_slice(trimmed);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.replace_line(pos.lnum, VimString::from(new_line));
        }
        self.set_insert_cursor(BufPos::new(pos.lnum, indent.len() as u32 + 1));
    }

    fn reindent_case_label(&mut self) {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
