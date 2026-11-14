import pathlib
p = pathlib.Path("crates/rvim-viml/src/builtins.rs")
s = p.read_text()
s = s.replace('''    "strchars",''', '''    "strchars",
    "strwidth",
    "strdisplaywidth",''', 1)
anchor = '''        "strchars" => {'''
arms = '''        "strwidth" => {
            arity(name, args, 1, Some(1))?;
            let text = string_arg(arg(args, 0))?;
            Ok(Value::Number(
                rvim_types::unicode::display_width(text.as_bytes(), 0, 1) as i64,
            ))
        }
        "strdisplaywidth" => {
            arity(name, args, 1, Some(2))?;
            let text = string_arg(arg(args, 0))?;
            let start = if args.len() > 1 {
                number_arg(arg(args, 1))?.max(0) as usize
            } else {
                0
            };
            Ok(Value::Number(
                rvim_types::unicode::display_width(text.as_bytes(), start, 8) as i64,
            ))
        }
        "strchars" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
