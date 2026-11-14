import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    pub fn clamp_cursor(&self, pos: BufPos) -> BufPos {"""
method = """    pub fn showcmd_text(&self) -> Option<String> {
        if !self.options.get_bool("showcmd") {
            return None;
        }
        let mut out = String::new();
        if let Some(count) = self.pending_count {
            out.push_str(&count.to_string());
        }
        if let Some(register) = self.pending_register {
            out.push('"');
            out.push(register);
        }
        if let Some(operator) = self.pending_operator {
            out.push(match operator {
                Operator::Delete => 'd',
                Operator::Change => 'c',
                Operator::Yank => 'y',
                Operator::UpperCase => 'U',
                Operator::LowerCase => 'u',
                Operator::Rot13 => 'g',
                Operator::Format => 'g',
                Operator::Indent => '>',
                Operator::Unindent => '<',
                Operator::Filter => '!',
            });
        }
        if self.pending_g {
            out.push('g');
        }
        if self.pending_ctrl_w {
            out.push_str("^W");
        }
        if self.pending_z {
            out.push('z');
        }
        if let Some(pending) = &self.pending_char {
            out.push(match pending {
                CharPending::Find { forward, till } => match (forward, till) {
                    (true, true) => 't',
                    (true, false) => 'f',
                    (false, true) => 'T',
                    (false, false) => 'F',
                },
                CharPending::Replace => 'r',
                CharPending::Mark => 'm',
                CharPending::Z => 'z',
                CharPending::TextObject { around } => {
                    if *around {
                        'a'
                    } else {
                        'i'
                    }
                }
                CharPending::Jump { exact } => {
                    if *exact {
                        char::from(96)
                    } else {
                        char::from(39)
                    }
                }
                CharPending::MacroRecord => 'q',
                CharPending::MacroPlay => '@',
                CharPending::Shift { right } => {
                    if *right {
                        '>'
                    } else {
                        '<'
                    }
                }
            });
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    pub fn clamp_cursor(&self, pos: BufPos) -> BufPos {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
