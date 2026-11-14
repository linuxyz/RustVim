import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
old = """                    dict.set(
                        VimString::from("leftcol"),
                        Value::Number(window.viewport.leftcol as i64),
                    );
                    entries.push(Value::dict(dict));"""
new = """                    dict.set(
                        VimString::from("leftcol"),
                        Value::Number(window.viewport.leftcol as i64),
                    );
                    let mut variables = VimDict::new();
                    let prefix = format!("{}:", rvim_types::Id::raw(*id));
                    for ((scope, key), value) in &self.vars {
                        if *scope == VarScope::Window {
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
