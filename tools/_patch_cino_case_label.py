import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace('''                if ch == '}' {\n                    self.reindent_closing_brace();\n                }''','''                if ch == '}' {\n                    self.reindent_closing_brace();\n                } else if ch == ':' {\n                    self.reindent_case_label();\n                }''',1)
anchor = """    fn reindent_closing_brace(&mut self) {"""
method = """    fn reindent_case_label(&mut self) {
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
        if !text.starts_with("case ") && !text.starts_with("default:") {
            return;
        }
        let mut depth = 0i32;
        let mut switch_indent = None;
        let mut lnum = pos.lnum.saturating_sub(1);
        while lnum >= 1 {
            let Some(previous) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
                .map(|line| line.to_vec())
            else {
                break;
            };
            for byte in previous.iter().rev() {
                match byte {
                    b'}' => depth += 1,
                    b'{' => {
                        if depth == 0 {
                            let opening = String::from_utf8_lossy(&previous);
                            if opening.trim_start().starts_with("switch") {
                                switch_indent = Some(
                                    previous
                                        .iter()
                                        .take_while(|byte| **byte == b' ' || **byte == b'\\t')
                                        .count(),
                                );
                            }
                            break;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            if switch_indent.is_some() {
                break;
            }
            lnum = lnum.saturating_sub(1);
        }
        let Some(switch_indent) = switch_indent else {
            return;
        };
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
        let amount = cino_amount(&cino, ':', width).unwrap_or(width as i32).max(0) as usize;
        let desired = switch_indent + amount;
        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        let indent = if expandtab {
            " ".repeat(desired)
        } else {
            format!("{}{}", "\\t".repeat(desired / width.max(1)), " ".repeat(desired % width.max(1)))
        };
        let mut new_line = indent.as_bytes().to_vec();
        new_line.extend_from_slice(&line[indent_len..]);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.replace_line(pos.lnum, VimString::from(new_line));
        }
        let col = indent.len() as u32 + (pos.col.saturating_sub(indent_len as u32));
        self.set_insert_cursor(BufPos::new(pos.lnum, col));
    }

    fn reindent_closing_brace(&mut self) {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
