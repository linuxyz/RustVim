import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "feedkeys" => {"""
arms = """            "inputlist" => {
                let Some(Value::List(list)) = args.first() else {
                    return Ok(Some(Value::Number(0)));
                };
                let items = list.borrow().items.clone();
                let mut display = Vec::new();
                for (index, item) in items.iter().enumerate() {
                    display.push(format!("{}. {}", index + 1, item.echo_string()));
                }
                self.editor.messages.info(display.join("\\n"));
                let mut text = String::new();
                while let Some(value) = self.editor.fed_keys.pop_front() {
                    match value {
                        Value::String(chunk) => text.push_str(&chunk.to_string_lossy()),
                        Value::Number(13) | Value::Number(10) => break,
                        _ => {}
                    }
                }
                let selected = text.trim().parse::<usize>().unwrap_or(0);
                let selected = if selected >= 1 && selected <= items.len() {
                    selected
                } else {
                    0
                };
                Ok(Some(Value::Number(selected as i64)))
            }
            "inputdialog" => {
                if let Some(prompt) = args.first() {
                    self.editor.messages.info(prompt.echo_string());
                }
                let default = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let mut text = String::new();
                while let Some(value) = self.editor.fed_keys.pop_front() {
                    match value {
                        Value::String(chunk) => text.push_str(&chunk.to_string_lossy()),
                        Value::Number(13) | Value::Number(10) => break,
                        _ => {}
                    }
                }
                if text.is_empty() {
                    text = default;
                }
                Ok(Some(Value::String(VimString::from(text))))
            }
            "feedkeys" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
