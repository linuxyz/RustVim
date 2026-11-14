import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""        let mut paren_depth = 0i32;""", """        let mut paren_depth = 0i32;
        let mut case_inline = false;""", 1)
old = """                if start.starts_with("case ") || start.starts_with("default:") {
                    add_indent = true;
                } else if start.starts_with("public:")"""
new = """                if start.starts_with("case ") || start.starts_with("default:") {
                    add_indent = true;
                    if let Some(colon) = start.find(':') {
                        if !start[colon + 1..].trim().is_empty()
                            && cino_amount(&cino, 'l', shiftwidth).unwrap_or(0) != 0
                        {
                            case_inline = true;
                        }
                    }
                } else if start.starts_with("public:")"""
assert old in s
s = s.replace(old, new, 1)
old2 = """            } else if initializer {
                add_amount = cino_amount(&cino, 'i', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if comment_opener {"""
new2 = """            } else if initializer {
                add_amount = cino_amount(&cino, 'i', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if case_inline {
                add_amount = 0;
            } else if comment_opener {"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("patched")
