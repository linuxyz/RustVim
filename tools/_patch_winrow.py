import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
old = """                let mut entries = Vec::new();
                for (index, id) in ids.iter().enumerate() {
                    let Some(window) = self.editor.view.windows.get(id) else {"""
new = """                let frame = self.editor.render();
                let mut entries = Vec::new();
                for (index, id) in ids.iter().enumerate() {
                    let Some(window) = self.editor.view.windows.get(id) else {"""
assert old in s
s = s.replace(old, new, 1)
old2 = """                    dict.set(
                        VimString::from(\"leftcol\"),
                        Value::Number(window.viewport.leftcol as i64),
                    );"""
new2 = """                    dict.set(
                        VimString::from(\"leftcol\"),
                        Value::Number(window.viewport.leftcol as i64),
                    );
                    let raw = rvim_types::Id::raw(*id);
                    let rect = frame
                        .hit_regions
                        .iter()
                        .find(|region| {
                            region.action.kind == rvim_ui_api::frame::ActionKind::Window
                                && region.action.id == raw
                        })
                        .map(|region| region.rect);
                    dict.set(
                        VimString::from(\"winrow\"),
                        Value::Number(rect.map_or(0, |rect| rect.row as i64 + 1)),
                    );
                    dict.set(
                        VimString::from(\"wincol\"),
                        Value::Number(rect.map_or(0, |rect| rect.col as i64 + 1)),
                    );"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("patched")
