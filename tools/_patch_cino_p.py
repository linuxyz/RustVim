import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""        let mut case_inline = false;""", """        let mut case_inline = false;
        let mut knr_params = false;""", 1)
old = """            if !add_indent && text.trim_start() == "/*" {"""
new = """            if !add_indent {
                let start = text.trim_start();
                if start.ends_with(')')
                    && start.contains('(')
                    && !start.contains(';')
                    && start
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
                {
                    add_indent = true;
                    knr_params = true;
                }
            }
            if !add_indent && text.trim_start() == "/*" {"""
assert old in s
s = s.replace(old, new, 1)
old2 = """            } else if case_inline {
                add_amount = 0;
            } else if comment_opener {"""
new2 = """            } else if case_inline {
                add_amount = 0;
            } else if knr_params {
                add_amount = cino_amount(&cino, 'p', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if comment_opener {"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("patched")
