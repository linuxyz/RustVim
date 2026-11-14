import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "caddexpr" => {'''
arms = '''        "cgetexpr" => {
            let value = editor.eval_viml(command.args.trim())?;
            let text = match value {
                rvim_viml::Value::List(list) => list
                    .borrow()
                    .items
                    .iter()
                    .map(|item| item.echo_string())
                    .collect::<Vec<_>>()
                    .join("\\n"),
                other => other.echo_string(),
            };
            let entries = editor.parse_quickfix_text(&text);
            editor.set_quickfix(":cgetexpr".to_string(), entries);
        }
        "cbuffer" => {
            let mut text = String::new();
            for lnum in 1..=editor.line_count() {
                if let Some(line) = editor.line(lnum) {
                    text.push_str(&String::from_utf8_lossy(line));
                    text.push('\\n');
                }
            }
            let entries = editor.parse_quickfix_text(&text);
            editor.set_quickfix(":cbuffer".to_string(), entries);
        }
        "cgetfile" => {
            let path = resolve_file_arg(editor, command.args.trim())?;
            let text = rvim_platform::fs::read_file(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            let entries = editor.parse_quickfix_text(&String::from_utf8_lossy(&text));
            editor.set_quickfix(path.display().to_string(), entries);
        }
        "caddexpr" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
