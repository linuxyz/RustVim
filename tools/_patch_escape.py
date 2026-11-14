import pathlib
p = pathlib.Path("crates/rvim-viml/src/builtins.rs")
s = p.read_text()
s = s.replace('''    "strwidth",''', '''    "strwidth",
    "escape",
    "shellescape",
    "fnameescape",''', 1)
anchor = '''        "strwidth" => {'''
arms = '''        "escape" => {
            arity(name, args, 2, Some(2))?;
            let text = string_arg(arg(args, 0))?;
            let chars = string_arg(arg(args, 1))?;
            let mut out = VimString::new();
            for ch in text.to_string_lossy().chars() {
                if chars.to_string_lossy().contains(ch) {
                    out.push(b'\\\\');
                }
                out.push_str(&ch.to_string());
            }
            Ok(Value::String(out))
        }
        "shellescape" => {
            arity(name, args, 1, Some(2))?;
            let text = string_arg(arg(args, 0))?;
            let mut out = VimString::from("'");
            for ch in text.to_string_lossy().chars() {
                if ch == '\\'' {
                    out.push_str("'\\\\''");
                } else {
                    out.push_str(&ch.to_string());
                }
            }
            out.push(b'\\'');
            Ok(Value::String(out))
        }
        "fnameescape" => {
            arity(name, args, 1, Some(1))?;
            let text = string_arg(arg(args, 0))?;
            let special = " \\t\\n*?[{\`$\\\\%#'\\\"|!<";
            let mut out = VimString::new();
            for ch in text.to_string_lossy().chars() {
                if special.contains(ch) {
                    out.push(b'\\\\');
                }
                out.push_str(&ch.to_string());
            }
            Ok(Value::String(out))
        }
        "strwidth" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
