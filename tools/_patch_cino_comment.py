import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""        let mut add_amount = 0i32;
        let mut continuation = false;""", """        let mut add_amount = 0i32;
        let mut continuation = false;
        let mut comment_opener = false;""", 1)
old = """            if !add_indent {
                let mut parens = 0i32;"""
new = """            if !add_indent && text.trim_start() == "/*" {
                add_indent = true;
                comment_opener = true;
            }
            if !add_indent {
                let mut parens = 0i32;"""
assert old in s
s = s.replace(old, new, 1)
old2 = """            } else if continuation {
                add_amount = cino_amount(&cino, '+', shiftwidth).unwrap_or(shiftwidth as i32);
            } else {"""
new2 = """            } else if continuation {
                add_amount = cino_amount(&cino, '+', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if comment_opener {
                add_amount = cino_amount(&cino, 'c', shiftwidth).unwrap_or(3);
            } else {"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("patched")
