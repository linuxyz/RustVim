import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getbufline" => {"""
arms = """            "bufnr" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "%".to_string());
                let number = resolve_buffer(self.editor, &spec)
                    .map(|id| self.editor.buffer_number_of(rvim_types::Id::raw(id)) as i64)
                    .unwrap_or(-1);
                Ok(Some(Value::Number(number)))
            }
            "bufname" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "%".to_string());
                let name = resolve_buffer(self.editor, &spec)
                    .and_then(|id| self.editor.buffers.get(id))
                    .map(|buffer| buffer.name_string())
                    .unwrap_or_default();
                Ok(Some(Value::string(name)))
            }
            "bufexists" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                Ok(Some(Value::Number(i64::from(
                    resolve_buffer(self.editor, &spec).is_some(),
                ))))
            }
            "bufloaded" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let loaded = resolve_buffer(self.editor, &spec)
                    .and_then(|id| self.editor.buffers.get(id))
                    .is_some_and(|buffer| buffer.loaded);
                Ok(Some(Value::Number(i64::from(loaded))))
            }
            "bufwinnr" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let number = resolve_buffer(self.editor, &spec)
                    .and_then(|buffer| {
                        self.editor
                            .view
                            .window_ids_in_current_tab()
                            .into_iter()
                            .position(|window| {
                                self.editor
                                    .view
                                    .windows
                                    .get(&window)
                                    .is_some_and(|window| window.buffer == buffer)
                            })
                            .map(|index| index as i64 + 1)
                    })
                    .unwrap_or(-1);
                Ok(Some(Value::Number(number)))
            }
            "bufadd" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let id = self
                    .editor
                    .create_buffer(Some(std::path::Path::new(&name)))
                    .map_err(|error| VimError::plain(error.to_string()))?;
                let id = self.editor.buffer_number_of(rvim_types::Id::raw(id));
                Ok(Some(Value::Number(id as i64)))
            }
            "getbufline" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
old = """fn resolve_buffer(editor: &crate::editor::Editor, spec: &str) -> Option<rvim_types::BufferId> {
    if let Ok(number) = spec.parse::<u32>() {
        return editor.buffer_by_number(number);
    }"""
new = """fn resolve_buffer(editor: &crate::editor::Editor, spec: &str) -> Option<rvim_types::BufferId> {
    if spec.is_empty() || spec == "%" {
        return Some(editor.current_buffer);
    }
    if spec == "$" {
        return editor.buffer_ids().last().copied();
    }
    if let Ok(number) = spec.parse::<u32>() {
        return editor.buffer_by_number(number);
    }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("patched")
