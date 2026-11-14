import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
old = """        let cino = self.options.buffer_string(buffer_id, "cinoptions");\n        let desired = match cino_amount(&cino, 'L', width) {"""
new = """        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        if cino_amount(&cino, 'J', width).unwrap_or(0) != 0 {
            let mut depth = 0i32;
            let mut lnum = pos.lnum.saturating_sub(1);
            while lnum >= 1 {
                let Some(previous) = self
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(lnum))
                else {
                    break;
                };
                let mut opening = None;
                for byte in previous.iter().rev() {
                    match byte {
                        b'}' => depth += 1,
                        b'{' => {
                            if depth == 0 {
                                opening = Some(previous.to_vec());
                                break;
                            }
                            depth -= 1;
                        }
                        _ => {}
                    }
                }
                if let Some(opening) = opening {
                    let text = String::from_utf8_lossy(&opening);
                    let text = text.trim_start();
                    if text.starts_with("var ")
                        || text.starts_with("let ")
                        || text.starts_with("const ")
                        || text.contains(" = ")
                        || text.ends_with('=')
                    {
                        return;
                    }
                    break;
                }
                lnum = lnum.saturating_sub(1);
            }
        }
        let desired = match cino_amount(&cino, 'L', width) {"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("patched")
