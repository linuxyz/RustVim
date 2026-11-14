import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getwinvar" => {"""
arms = """            "gettabwinvar" => {
                let tabnr = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let winnr = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let name = args
                    .get(2)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let tabs = self.editor.view.tab_ids();
                let window = tabs
                    .get(tabnr.saturating_sub(1))
                    .and_then(|tab| {
                        self.editor
                            .view
                            .windows_in_tab(*tab)
                            .get(winnr.saturating_sub(1))
                            .copied()
                    });
                let value = window
                    .and_then(|window| {
                        self.vars
                            .get(&(
                                VarScope::Window,
                                format!("{}:{}", rvim_types::Id::raw(window), name),
                            ))
                            .cloned()
                    })
                    .or_else(|| args.get(3).cloned())
                    .unwrap_or(Value::Number(0));
                Ok(Some(value))
            }
            "settabwinvar" => {
                let tabnr = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let winnr = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let name = args
                    .get(2)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let value = args.get(3).cloned().unwrap_or(Value::Number(0));
                let tabs = self.editor.view.tab_ids();
                if let Some(window) = tabs.get(tabnr.saturating_sub(1)).and_then(|tab| {
                    self.editor
                        .view
                        .windows_in_tab(*tab)
                        .get(winnr.saturating_sub(1))
                        .copied()
                }) {
                    self.vars.insert(
                        (
                            VarScope::Window,
                            format!("{}:{}", rvim_types::Id::raw(window), name),
                        ),
                        value,
                    );
                }
                Ok(Some(Value::Number(0)))
            }
            "getwinvar" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
