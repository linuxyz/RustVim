import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
old = """                    dict.set(
                        VimString::from("linecount"),
                        Value::Number(buffer.text.line_count() as i64),
                    );
                    entries.push(Value::dict(dict));"""
new = """                    dict.set(
                        VimString::from("linecount"),
                        Value::Number(buffer.text.line_count() as i64),
                    );
                    let windows: Vec<Value> = self
                        .editor
                        .view
                        .windows
                        .iter()
                        .filter(|(_, window)| window.buffer == *id)
                        .map(|(_, window)| Value::Number(rvim_types::Id::raw(window.id) as i64))
                        .collect();
                    dict.set(VimString::from("windows"), Value::list(windows));
                    let mut variables = VimDict::new();
                    let prefix = format!("{}:", rvim_types::Id::raw(*id));
                    for ((scope, key), value) in &self.vars {
                        if *scope == VarScope::Buffer {
                            if let Some(name) = key.strip_prefix(&prefix) {
                                variables.set(VimString::from(name), value.clone());
                            }
                        }
                    }
                    dict.set(VimString::from("variables"), Value::dict(variables));
                    entries.push(Value::dict(dict));"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("patched")
