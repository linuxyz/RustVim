import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""        let mut comment_opener = false;""", """        let mut comment_opener = false;
        let mut comment_column = None;""", 1)
old = """            if !add_indent && text.trim_start() == "/*" {
                add_indent = true;
                comment_opener = true;
            }"""
new = """            if !add_indent && text.trim_start() == "/*" {
                add_indent = true;
                comment_opener = true;
            }
            if !add_indent {
                let start = text.trim_start();
                if let Some(index) = start.find("/*") {
                    let after = &start[index + 2..];
                    if !after.trim().is_empty() {
                        add_indent = true;
                        comment_opener = true;
                        comment_column =
                            Some(index + 2 + after.len().saturating_sub(after.trim_start().len()));
                    }
                }
            }"""
assert old in s
s = s.replace(old, new, 1)
old2 = """            } else if comment_opener {
                add_amount = cino_amount(&cino, 'c', shiftwidth).unwrap_or(3);
            } else {"""
new2 = """            } else if comment_opener {
                let comment_indent = cino_amount(&cino, 'c', shiftwidth).unwrap_or(3);
                let forced = cino_amount(&cino, 'C', shiftwidth).unwrap_or(0) != 0;
                add_amount = match comment_column {
                    Some(column) if !forced => column as i32,
                    _ => comment_indent,
                };
            } else {"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("patched")
