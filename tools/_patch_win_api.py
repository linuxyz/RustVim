import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getqflist" => {"""
arms = """            "win_getid" => {
                let id = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| window.id)
                    .unwrap_or_default();
                Ok(Some(Value::Number(rvim_types::Id::raw(id) as i64)))
            }
            "win_id2win" => {
                let id = args
                    .first()
                    .map(|value| value.echo_string().parse::<u64>().unwrap_or(0))
                    .unwrap_or(0);
                let ids = self.editor.view.window_ids_in_current_tab();
                let index = ids
                    .iter()
                    .position(|window| rvim_types::Id::raw(*window) == id)
                    .map_or(0, |index| index + 1);
                Ok(Some(Value::Number(index as i64)))
            }
            "win_gotoid" => {
                let id = args
                    .first()
                    .map(|value| value.echo_string().parse::<u64>().unwrap_or(0))
                    .unwrap_or(0);
                let found = self
                    .editor
                    .view
                    .window_ids_in_current_tab()
                    .into_iter()
                    .find(|window| rvim_types::Id::raw(*window) == id);
                if let Some(window) = found {
                    self.editor.view.set_current_window(window);
                    self.editor.sync_current_buffer_to_window();
                    self.editor.sync_window_options();
                    Ok(Some(Value::Number(1)))
                } else {
                    Ok(Some(Value::Number(0)))
                }
            }
            "win_findbuf" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let Some(buffer) = self.editor.buffer_by_number(number) else {
                    return Ok(Some(Value::list(Vec::new())));
                };
                let ids = self
                    .editor
                    .view
                    .windows
                    .iter()
                    .filter(|(_, window)| window.buffer == buffer)
                    .map(|(_, window)| Value::Number(rvim_types::Id::raw(window.id) as i64))
                    .collect();
                Ok(Some(Value::list(ids)))
            }
            "getqflist" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
