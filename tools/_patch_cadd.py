import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "compiler" | "comp" | "compi" => {'''
arms = '''        "caddexpr" => {
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
            let mut combined = editor.quickfix.entries.clone();
            combined.extend(entries);
            editor.set_quickfix(":caddexpr".to_string(), combined);
        }
        "caddbuffer" | "cad" => {
            let mut text = String::new();
            for lnum in 1..=editor.line_count() {
                if let Some(line) = editor.line(lnum) {
                    text.push_str(&String::from_utf8_lossy(line));
                    text.push('\\n');
                }
            }
            let entries = editor.parse_quickfix_text(&text);
            let mut combined = editor.quickfix.entries.clone();
            combined.extend(entries);
            editor.set_quickfix(":caddbuffer".to_string(), combined);
        }
        "compiler" | "comp" | "compi" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
