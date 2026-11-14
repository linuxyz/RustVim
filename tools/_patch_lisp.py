import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""        let smartindent = self.options.buffer_bool(buffer_id, "smartindent");
        let cindent = self.options.buffer_bool(buffer_id, "cindent");
        if !autoindent && !smartindent && !cindent {
            return String::new();
        }""", """        let smartindent = self.options.buffer_bool(buffer_id, "smartindent");
        let cindent = self.options.buffer_bool(buffer_id, "cindent");
        let lisp = self.options.buffer_bool(buffer_id, "lisp");
        if !autoindent && !smartindent && !cindent && !lisp {
            return String::new();
        }""", 1)
# after shiftwidth/expandtab computed, insert lisp override before add_indent block
old = """        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        let mut add_indent = (smartindent || cindent) && trimmed.last() == Some(&b'{');"""
new = """        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        if lisp {
            let pos = self.cursor();
            let mut depth = 0i32;
            for lnum in 1..=pos.lnum {
                if let Some(previous) = self
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(lnum))
                {
                    for byte in previous {
                        match byte {
                            b'(' => depth += 1,
                            b')' => depth = (depth - 1).max(0),
                            _ => {}
                        }
                    }
                }
            }
            let width = (depth as usize) * shiftwidth;
            return if expandtab {
                " ".repeat(width)
            } else {
                let tabs = width / shiftwidth.max(1);
                let spaces = width % shiftwidth.max(1);
                format!("{}{}", "\\t".repeat(tabs), " ".repeat(spaces))
            };
        }
        let mut add_indent = (smartindent || cindent) && trimmed.last() == Some(&b'{');"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("patched")
