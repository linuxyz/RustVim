import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""        let mut scope_decl = false;""", """        let mut scope_decl = false;
        let mut initializer = false;""", 1)
old = """            if !add_indent && text.trim_start() == "/*" {"""
new = """            if !add_indent {
                let start = text.trim_start();
                let base_class = start.starts_with("class ") || start.starts_with("struct ");
                if start.ends_with(':') && (base_class || start.contains("::")) {
                    add_indent = true;
                    initializer = true;
                }
            }
            if !add_indent && text.trim_start() == "/*" {"""
assert old in s
s = s.replace(old, new, 1)
old2 = """            } else if scope_decl {
                add_amount = cino_amount(&cino, 'h', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if comment_opener {"""
new2 = """            } else if scope_decl {
                add_amount = cino_amount(&cino, 'h', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if initializer {
                add_amount = cino_amount(&cino, 'i', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if comment_opener {"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("patched")
