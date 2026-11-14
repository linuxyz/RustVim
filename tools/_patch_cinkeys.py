import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
# call after insert char
s = s.replace("""                let mut buffer = [0u8; 4];
                let text = ch.encode_utf8(&mut buffer);
                self.insert_text(text.as_bytes());
            }
            Key::Enter => {""", """                let mut buffer = [0u8; 4];
                let text = ch.encode_utf8(&mut buffer);
                self.insert_text(text.as_bytes());
                if ch == '}' {
                    self.reindent_closing_brace();
                }
            }
            Key::Enter => {""", 1)
# add method before insert_ctrl
anchor = """    fn insert_ctrl(&mut self, ch: char) {"""
method = """    fn reindent_closing_brace(&mut self) {
        let buffer_id = self.current_buffer_id();
        let cindent = self.options.buffer_bool(buffer_id, "cindent");
        let smartindent = self.options.buffer_bool(buffer_id, "smartindent");
        if !cindent && !smartindent {
            return;
        }
        if cindent {
            let cinkeys = self.options.buffer_string(buffer_id, "cinkeys");
            if !cinkeys.contains("0}") && !cinkeys.split(',').any(|key| key == "}") {
                return;
            }
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
        if line.get(indent_len) != Some(&b'}') {
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
            self.options
                .buffer_number(buffer_id, "tabstop")
                .max(1) as usize
        } else {
            width
        };
        let desired = (depth.saturating_sub(1) as usize) * width;
        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        let indent = if expandtab {
            " ".repeat(desired)
        } else {
            let tabs = desired / width.max(1);
            let spaces = desired % width.max(1);
            format!("{}{}", "\\t".repeat(tabs), " ".repeat(spaces))
        };
        let mut new_line = indent.as_bytes().to_vec();
        new_line.extend_from_slice(&line[indent_len..]);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.replace_line(pos.lnum, VimString::from(new_line));
        }
        self.set_insert_cursor(BufPos::new(pos.lnum, indent.len() as u32 + 1));
    }

    fn insert_ctrl(&mut self, ch: char) {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
