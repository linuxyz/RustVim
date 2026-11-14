import pathlib
p = pathlib.Path("crates/rvim-viml/src/builtins.rs")
s = p.read_text()
s = s.replace('''    "join",''', '''    "join",
    "tr",''', 1)
anchor = '''        "join" => {'''
arms = '''        "tr" => {
            arity(name, args, 3, Some(3))?;
            let text = string_arg(arg(args, 0))?;
            let from: Vec<char> = string_arg(arg(args, 1))?.to_string_lossy().chars().collect();
            let to: Vec<char> = string_arg(arg(args, 2))?.to_string_lossy().chars().collect();
            let mut out = VimString::new();
            for ch in text.to_string_lossy().chars() {
                if let Some(index) = from.iter().position(|candidate| *candidate == ch) {
                    if let Some(character) = to.get(index) {
                        out.push_str(&character.to_string());
                    }
                } else {
                    out.push_str(&ch.to_string());
                }
            }
            Ok(Value::String(out))
        }
        "join" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
