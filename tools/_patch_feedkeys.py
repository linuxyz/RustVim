import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub suspend_requested: bool,""", """    pub suspend_requested: bool,
    pub fed_keys: std::collections::VecDeque<rvim_viml::Value>,""", 1)
s = s.replace("""            suspend_requested: false,""", """            suspend_requested: false,
            fed_keys: std::collections::VecDeque::new(),""", 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "win_getid" => {"""
arms = """            "feedkeys" => {
                let text = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                for value in parse_fed_keys(&text) {
                    self.editor.fed_keys.push_back(value);
                }
                Ok(Some(Value::Number(0)))
            }
            "getchar" => Ok(Some(
                self.editor
                    .fed_keys
                    .pop_front()
                    .unwrap_or(Value::Number(0)),
            )),
            "getcharstr" => {
                let value = self
                    .editor
                    .fed_keys
                    .pop_front()
                    .unwrap_or(Value::String(VimString::from("")));
                let text = match value {
                    Value::String(text) => text.as_str().to_string(),
                    Value::Number(code) => char::from_u32(code as u32)
                        .map(|ch| ch.to_string())
                        .unwrap_or_default(),
                    other => other.echo_string(),
                };
                Ok(Some(Value::String(VimString::from(text))))
            }
            "input" => {
                if let Some(prompt) = args.first() {
                    self.editor.messages.info(prompt.echo_string());
                }
                let mut text = String::new();
                while let Some(value) = self.editor.fed_keys.pop_front() {
                    match value {
                        Value::String(chunk) => text.push_str(&chunk.echo_string()),
                        Value::Number(13) | Value::Number(10) => break,
                        _ => {}
                    }
                }
                Ok(Some(Value::String(VimString::from(text))))
            }
            "win_getid" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
# append helper before impl? after struct impl maybe; easiest append at end
s += '''

fn parse_fed_keys(text: &str) -> Vec<Value> {
    let mut keys = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '<' {
            keys.push(Value::String(VimString::from(ch.to_string())));
            continue;
        }
        let mut notation = String::new();
        while let Some(&next) = chars.peek() {
            chars.next();
            if next == '>' {
                break;
            }
            notation.push(next);
        }
        let value = match notation.as_str() {
            "CR" | "Enter" => Value::Number(13),
            "NL" => Value::Number(10),
            "Esc" => Value::Number(27),
            "Tab" => Value::Number(9),
            "BS" => Value::Number(8),
            "Space" => Value::Number(32),
            name if name.starts_with("C-") => {
                let key = name[2..].chars().next().unwrap_or('@');
                Value::Number((key.to_ascii_lowercase() as i64 - 'a' as i64 + 1).max(0))
            }
            other => Value::String(VimString::from(format!("<{other}>"))),
        };
        keys.push(value);
    }
    keys
}
'''
p.write_text(s)
print("host patched")
