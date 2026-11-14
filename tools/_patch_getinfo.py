import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getjumplist" => {"""
arms = """            "getbufinfo" => {
                let ids = self.editor.buffer_ids();
                let mut entries = Vec::new();
                for (index, id) in ids.iter().enumerate() {
                    let Some(buffer) = self.editor.buffers.get(*id) else {
                        continue;
                    };
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("bufnr"), Value::Number(index as i64 + 1));
                    dict.set(
                        VimString::from("name"),
                        Value::string(buffer.name_string()),
                    );
                    dict.set(VimString::from("loaded"), Value::Bool(buffer.loaded));
                    dict.set(VimString::from("listed"), Value::Bool(buffer.listed));
                    dict.set(
                        VimString::from("changed"),
                        Value::Bool(buffer.modified()),
                    );
                    dict.set(
                        VimString::from("linecount"),
                        Value::Number(buffer.text.line_count() as i64),
                    );
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "getwininfo" => {
                let ids = self.editor.view.window_ids_in_current_tab();
                let buffers = self.editor.buffer_ids();
                let tabnr = self
                    .editor
                    .view
                    .tab_ids()
                    .iter()
                    .position(|id| *id == self.editor.view.current_tab)
                    .map_or(1, |index| index + 1);
                let mut entries = Vec::new();
                for (index, id) in ids.iter().enumerate() {
                    let Some(window) = self.editor.view.windows.get(id) else {
                        continue;
                    };
                    let bufnr = buffers
                        .iter()
                        .position(|buffer| *buffer == window.buffer)
                        .map_or(0, |index| index + 1);
                    let mut dict = VimDict::new();
                    dict.set(
                        VimString::from("winid"),
                        Value::Number(rvim_types::Id::raw(*id) as i64),
                    );
                    dict.set(VimString::from("winnr"), Value::Number(index as i64 + 1));
                    dict.set(VimString::from("bufnr"), Value::Number(bufnr as i64));
                    dict.set(VimString::from("tabnr"), Value::Number(tabnr as i64));
                    dict.set(
                        VimString::from("height"),
                        Value::Number(window.height as i64),
                    );
                    dict.set(
                        VimString::from("width"),
                        Value::Number(window.width as i64),
                    );
                    dict.set(
                        VimString::from("topline"),
                        Value::Number(window.viewport.topline as i64),
                    );
                    dict.set(
                        VimString::from("leftcol"),
                        Value::Number(window.viewport.leftcol as i64),
                    );
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "getjumplist" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
