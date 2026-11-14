import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace('''                } else if ch == ':' {\n                    self.reindent_case_label();\n                    self.reindent_scope_label();\n                    self.reindent_jump_label();\n                }''','''                } else if ch == ':' {\n                    self.reindent_case_label();\n                    self.reindent_scope_label();\n                    self.reindent_jump_label();\n                } else if ch == ';' {\n                    self.reindent_break();\n                }''',1)
anchor = """    fn reindent_scope_label(&mut self) {"""
method = """    fn reindent_break(&mut self) {
        let buffer_id = self.current_buffer_id();
        if !self.options.buffer_bool(buffer_id, "cindent") {
            return;
        }
        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        let width = self
            .options
            .buffer_number(buffer_id, "shiftwidth")
            .max(0) as usize;
        let width = if width == 0 {
            self.options.buffer_number(buffer_id, "tabstop").max(1) as usize
        } else {
            width
        };
        if cino_amount(&cino, 'b', width).unwrap_or(0) == 0 {
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
        if &line[indent_len..] != b"break;" {
            return;
        }
        let mut lnum = pos.lnum.saturating_sub(1);
        let mut desired = None;
        while lnum >= 1 {
            let Some(previous) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
                .map(|line| line.to_vec())
            else {
                break;
            };
            let indent = previous
                .iter()
                .take_while(|byte| **byte == b' ' || **byte == b'\\t')
                .count();
            let text = String::from_utf8_lossy(&previous[indent..]);
            if text.starts_with("case ") || text.starts_with("default:") {
                desired = Some(indent);
                break;
            }
            lnum = lnum.saturating_sub(1);
        }
        let Some(desired) = desired else {
            return;
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
        new_line.extend_from_slice(b"break;");
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.replace_line(pos.lnum, VimString::from(new_line));
        }
        self.set_insert_cursor(BufPos::new(pos.lnum, indent.len() as u32 + 6));
    }

    fn reindent_scope_label(&mut self) {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
