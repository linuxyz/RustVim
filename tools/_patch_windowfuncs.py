import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "gettabwinvar" => {"""
arms = """            "winnr" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| ".".to_string());
                let ids = self.editor.view.window_ids_in_current_tab();
                let current = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| window.id);
                let number = if spec == "$" {
                    ids.len() as i64
                } else {
                    current
                        .and_then(|current| ids.iter().position(|id| *id == current))
                        .map_or(0, |index| index as i64 + 1)
                };
                Ok(Some(Value::Number(number)))
            }
            "winbufnr" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let ids = self.editor.view.window_ids_in_current_tab();
                let bufnr = number
                    .checked_sub(1)
                    .and_then(|index| ids.get(index))
                    .and_then(|id| self.editor.view.windows.get(id))
                    .map(|window| {
                        self.editor
                            .buffer_number_of(rvim_types::Id::raw(window.buffer))
                    })
                    .unwrap_or(0);
                Ok(Some(Value::Number(bufnr as i64)))
            }
            "winheight" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let height = number
                    .checked_sub(1)
                    .and_then(|index| {
                        self.editor
                            .view
                            .window_ids_in_current_tab()
                            .get(index)
                            .copied()
                    })
                    .and_then(|id| self.editor.view.windows.get(&id))
                    .map_or(0, |window| window.height);
                Ok(Some(Value::Number(height as i64)))
            }
            "winwidth" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let width = number
                    .checked_sub(1)
                    .and_then(|index| {
                        self.editor
                            .view
                            .window_ids_in_current_tab()
                            .get(index)
                            .copied()
                    })
                    .and_then(|id| self.editor.view.windows.get(&id))
                    .map_or(0, |window| window.width);
                Ok(Some(Value::Number(width as i64)))
            }
            "winline" => {
                let line = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| {
                        self.editor
                            .cursor()
                            .lnum
                            .saturating_sub(window.viewport.topline)
                            as i64
                            + 1
                    })
                    .unwrap_or(0);
                Ok(Some(Value::Number(line)))
            }
            "wincol" => {
                let Some(window) = self.editor.view.current_window() else {
                    return Ok(Some(Value::Number(0)));
                };
                let cursor = self.editor.cursor();
                let line = self
                    .editor
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(cursor.lnum))
                    .unwrap_or(b"");
                let width = rvim_types::unicode::display_width(
                    &line[..(cursor.col as usize).min(line.len())],
                    0,
                    window.options.tabstop.max(1),
                );
                let column = (width as u16)
                    .saturating_sub(window.viewport.leftcol as u16)
                    + 1;
                Ok(Some(Value::Number(column as i64)))
            }
            "gettabwinvar" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
