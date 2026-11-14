import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
old = """                let ids = self.editor.view.tab_ids();
                let value = ids
                    .get(tab.saturating_sub(1))
                    .and_then(|id| self.editor.tab_vars.get(id))
                    .and_then(|vars| vars.get(&name))
                    .cloned()
                    .or_else(|| args.get(2).cloned())
                    .unwrap_or(Value::Number(0));
                Ok(Some(value))"""
new = """                let ids = self.editor.view.tab_ids();
                let value = ids
                    .get(tab.saturating_sub(1))
                    .and_then(|id| {
                        let key = (
                            VarScope::Tab,
                            format!("{}:{}", rvim_types::Id::raw(*id), name),
                        );
                        self.vars.get(&key).cloned()
                    })
                    .or_else(|| args.get(2).cloned())
                    .unwrap_or(Value::Number(0));
                Ok(Some(value))"""
assert old in s
s = s.replace(old, new, 1)
old2 = """                let ids = self.editor.view.tab_ids();
                if let Some(id) = ids.get(tab.saturating_sub(1)).copied() {
                    self.editor
                        .tab_vars
                        .entry(id)
                        .or_default()
                        .insert(name, value);
                }
                Ok(Some(Value::Number(0)))"""
new2 = """                let ids = self.editor.view.tab_ids();
                if let Some(id) = ids.get(tab.saturating_sub(1)).copied() {
                    self.vars.insert(
                        (VarScope::Tab, format!("{}:{}", rvim_types::Id::raw(id), name)),
                        value,
                    );
                }
                Ok(Some(Value::Number(0)))"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("patched")
