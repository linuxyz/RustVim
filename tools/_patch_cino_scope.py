import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
# call scope reindent before jump label
s = s.replace('''                } else if ch == ':' {\n                    self.reindent_case_label();\n                    self.reindent_jump_label();\n                }''','''                } else if ch == ':' {\n                    self.reindent_case_label();\n                    self.reindent_scope_label();\n                    self.reindent_jump_label();\n                }''',1)
# jump label exclude scope keywords
s = s.replace('''        let Some(label) = text.strip_suffix(':') else {\n            return;\n        };\n        if label.is_empty()''','''        let Some(label) = text.strip_suffix(':') else {\n            return;\n        };\n        if matches!(label, "public" | "private" | "protected" | "signals" | "slots") {\n            return;\n        }\n        if label.is_empty()''',1)
# newline indent scope detection
s = s.replace('''        let mut comment_column = None;''','''        let mut comment_column = None;
        let mut scope_decl = false;''',1)
old = """            if !add_indent {
                let start = text.trim_start();
                if start.starts_with("case ") || start.starts_with("default:") {
                    add_indent = true;
                }
            }"""
new = """            if !add_indent {
                let start = text.trim_start();
                if start.starts_with("case ") || start.starts_with("default:") {
                    add_indent = true;
                } else if start.starts_with("public:")
                    || start.starts_with("private:")
                    || start.starts_with("protected:")
                    || start.starts_with("signals:")
                    || start.starts_with("slots:")
                {
                    add_indent = true;
                    scope_decl = true;
                }
            }"""
assert old in s
s = s.replace(old, new, 1)
old2 = """            } else if continuation {
                add_amount = cino_amount(&cino, '+', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if comment_opener {"""
new2 = """            } else if continuation {
                add_amount = cino_amount(&cino, '+', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if scope_decl {
                add_amount = cino_amount(&cino, 'h', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if comment_opener {"""
assert old2 in s
s = s.replace(old2, new2, 1)
# scope reindent method before case label
anchor = """    fn reindent_case_label(&mut self) {"""
method = """    fn reindent_scope_label(&mut self) {
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
        if !matches!(
            text.as_ref(),
            "public:" | "private:" | "protected:" | "signals:" | "slots:"
        ) {
            return;
        }
        let mut depth = 0i32;
        let mut base_indent = None;
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
                            let trimmed = opening.trim_start();
                            if trimmed.starts_with("class ")
                                || trimmed.starts_with("struct ")
                                || trimmed.starts_with("union ")
                                || trimmed.starts_with("namespace ")
                            {
                                base_indent = Some(
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
            if base_indent.is_some() {
                break;
            }
            lnum = lnum.saturating_sub(1);
        }
        let Some(base_indent) = base_indent else {
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
        let desired = base_indent
            + cino_amount(&cino, 'g', width)
                .unwrap_or(width as i32)
                .max(0) as usize;
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
