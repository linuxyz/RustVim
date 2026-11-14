import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "inputlist" => {"""
arm = """            "confirm" => {
                if let Some(message) = args.first() {
                    self.editor.messages.info(message.echo_string());
                }
                let choices = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "&Yes\\n&No".to_string());
                let default = args
                    .get(2)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let labeled: Vec<String> = choices
                    .lines()
                    .map(|line| line.trim_start_matches('&').to_string())
                    .collect();
                self.editor.messages.info(
                    labeled
                        .iter()
                        .enumerate()
                        .map(|(index, label)| format!("{}. {label}", index + 1))
                        .collect::<Vec<_>>()
                        .join("\\n"),
                );
                let mut text = String::new();
                while let Some(value) = self.editor.fed_keys.pop_front() {
                    match value {
                        Value::String(chunk) => text.push_str(&chunk.to_string_lossy()),
                        Value::Number(13) | Value::Number(10) => break,
                        _ => {}
                    }
                }
                let choice = text.trim().parse::<usize>().ok();
                let choice = match choice {
                    Some(choice) if choice >= 1 && choice <= labeled.len() => choice,
                    _ => default.min(labeled.len().max(1)),
                };
                Ok(Some(Value::Number(choice as i64)))
            }
            "inputlist" => {"""
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("patched")
