import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getbufinfo" => {"""
arm = """            "gettabinfo" => {
                let ids = self.editor.view.tab_ids();
                let mut entries = Vec::new();
                for (index, id) in ids.iter().enumerate() {
                    let Some(page) = self.editor.view.tabs.get(id) else {
                        continue;
                    };
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("tabnr"), Value::Number(index as i64 + 1));
                    dict.set(
                        VimString::from("windows"),
                        Value::Number(
                            rvim_view::layout::all_windows(&page.root).len() as i64,
                        ),
                    );
                    let mut variables = VimDict::new();
                    let prefix = format!("{}:", rvim_types::Id::raw(*id));
                    for ((scope, key), value) in &self.vars {
                        if *scope != VarScope::Tab {
                            continue;
                        }
                        if let Some(name) = key.strip_prefix(&prefix) {
                            variables.set(VimString::from(name), value.clone());
                        }
                    }
                    dict.set(VimString::from("variables"), Value::dict(variables));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "getbufinfo" => {"""
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("patched")
