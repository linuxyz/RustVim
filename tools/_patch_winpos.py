import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "winnr" => {"""
arms = """            "getwinpos" => {
                let frame = self.editor.render();
                let rect = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| rvim_types::Id::raw(window.id))
                    .and_then(|raw| {
                        frame
                            .hit_regions
                            .iter()
                            .find(|region| {
                                region.action.kind
                                    == rvim_ui_api::frame::ActionKind::Window
                                    && region.action.id == raw
                            })
                            .map(|region| region.rect)
                    });
                let (col, row, width, height) = rect.map_or((0, 0, 0, 0), |rect| {
                    (rect.col + 1, rect.row + 1, rect.width, rect.height)
                });
                Ok(Some(Value::list(vec![
                    Value::Number(col as i64),
                    Value::Number(row as i64),
                    Value::Number(width as i64),
                    Value::Number(height as i64),
                ])))
            }
            "getwinposx" => {
                let frame = self.editor.render();
                let raw = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| rvim_types::Id::raw(window.id));
                let col = raw
                    .and_then(|raw| {
                        frame.hit_regions.iter().find(|region| {
                            region.action.kind == rvim_ui_api::frame::ActionKind::Window
                                && region.action.id == raw
                        })
                    })
                    .map_or(0, |region| region.rect.col as i64 + 1);
                Ok(Some(Value::Number(col)))
            }
            "getwinposy" => {
                let frame = self.editor.render();
                let raw = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| rvim_types::Id::raw(window.id));
                let row = raw
                    .and_then(|raw| {
                        frame.hit_regions.iter().find(|region| {
                            region.action.kind == rvim_ui_api::frame::ActionKind::Window
                                && region.action.id == raw
                        })
                    })
                    .map_or(0, |region| region.rect.row as i64 + 1);
                Ok(Some(Value::Number(row)))
            }
            "winnr" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
