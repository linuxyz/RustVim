import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "tabpagenr" => {"""
arms = """            "getwinvar" => {
                let window = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let name = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let ids = self.editor.view.window_ids_in_current_tab();
                let id = ids
                    .get(window.saturating_sub(1))
                    .copied()
                    .or_else(|| self.editor.view.current_window().map(|window| window.id));
                let value = id
                    .and_then(|id| {
                        let key = (
                            VarScope::Window,
                            format!("{}:{}", rvim_types::Id::raw(id), name),
                        );
                        self.vars.get(&key).cloned()
                    })
                    .or_else(|| args.get(2).cloned())
                    .unwrap_or(Value::Number(0));
                Ok(Some(value))
            }
            "setwinvar" => {
                let window = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let name = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let value = args.get(2).cloned().unwrap_or(Value::Number(0));
                let ids = self.editor.view.window_ids_in_current_tab();
                if let Some(id) = ids
                    .get(window.saturating_sub(1))
                    .copied()
                    .or_else(|| self.editor.view.current_window().map(|window| window.id))
                {
                    self.vars.insert(
                        (
                            VarScope::Window,
                            format!("{}:{}", rvim_types::Id::raw(id), name),
                        ),
                        value,
                    );
                }
                Ok(Some(Value::Number(0)))
            }
            "getbufvar" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let name = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let value = self
                    .editor
                    .buffer_by_number(number)
                    .and_then(|id| {
                        let key = (
                            VarScope::Buffer,
                            format!("{}:{}", rvim_types::Id::raw(id), name),
                        );
                        self.vars.get(&key).cloned()
                    })
                    .or_else(|| args.get(2).cloned())
                    .unwrap_or(Value::Number(0));
                Ok(Some(value))
            }
            "setbufvar" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let name = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let value = args.get(2).cloned().unwrap_or(Value::Number(0));
                if let Some(id) = self.editor.buffer_by_number(number) {
                    self.vars.insert(
                        (
                            VarScope::Buffer,
                            format!("{}:{}", rvim_types::Id::raw(id), name),
                        ),
                        value,
                    );
                }
                Ok(Some(Value::Number(0)))
            }
            "tabpagenr" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
