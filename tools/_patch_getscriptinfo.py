import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "gettagstack" => {"""
arm = """            "getscriptinfo" => {
                let mut entries = Vec::new();
                for id in self.editor.viml.script_ids() {
                    let name = self.editor.viml.script_name(id).unwrap_or_default();
                    let mut dict = VimDict::new();
                    dict.set(
                        VimString::from("sid"),
                        Value::Number(rvim_types::Id::raw(id) as i64),
                    );
                    dict.set(VimString::from("name"), Value::string(name.as_str()));
                    dict.set(VimString::from("version"), Value::Number(1));
                    dict.set(
                        VimString::from("autoload"),
                        Value::Number(if name.contains("autoload/") { 1 } else { 0 }),
                    );
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "gettagstack" => {"""
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("patched")
