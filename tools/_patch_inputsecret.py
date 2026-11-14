import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "inputlist" => {"""
arm = """            "inputsecret" => {
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
            "inputlist" => {"""
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("patched")
