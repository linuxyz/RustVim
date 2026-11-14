import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
old = """        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        let mut add_amount = 0i32;
        let mut add_indent = (smartindent || cindent) && trimmed.last() == Some(&b'{');"""
new = """        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        let mut add_amount = 0i32;
        let mut continuation = false;
        let mut add_indent = (smartindent || cindent) && trimmed.last() == Some(&b'{');"""
assert old in s
s = s.replace(old, new, 1)
old2 = """            if !add_indent {
                let start = text.trim_start();
                if start.starts_with("case ") || start.starts_with("default:") {
                    add_indent = true;
                }
            }
        }"""
new2 = """            if !add_indent {
                let start = text.trim_start();
                if start.starts_with("case ") || start.starts_with("default:") {
                    add_indent = true;
                }
            }
            if !add_indent {
                let mut parens = 0i32;
                for byte in &trimmed {
                    match byte {
                        b'(' => parens += 1,
                        b')' => parens = (parens - 1).max(0),
                        _ => {}
                    }
                }
                if parens > 0 {
                    add_indent = true;
                    continuation = true;
                }
            }
        }"""
assert old2 in s
s = s.replace(old2, new2, 1)
old3 = """            } else {
                add_amount = cino_amount(&cino, '>', shiftwidth).unwrap_or(shiftwidth as i32);
                if trimmed.last() == Some(&b'{') {"""
new3 = """            } else if continuation {
                add_amount = cino_amount(&cino, '+', shiftwidth).unwrap_or(shiftwidth as i32);
            } else {
                add_amount = cino_amount(&cino, '>', shiftwidth).unwrap_or(shiftwidth as i32);
                if trimmed.last() == Some(&b'{') {"""
assert old3 in s
s = s.replace(old3, new3, 1)
p.write_text(s)
print("patched")
