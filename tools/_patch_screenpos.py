import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "screenchar" => {"""
arm = """            "screenpos" => {
                let winid = args
                    .first()
                    .map(|value| value.echo_string().parse::<u64>().unwrap_or(0))
                    .unwrap_or(0);
                let lnum = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let col = args
                    .get(2)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let window_id = rvim_types::WindowId::from_raw(winid);
                let frame = self.editor.render();
                let Some(window) = self.editor.view.windows.get(&window_id) else {
                    let mut dict = VimDict::new();
                    for name in ["row", "col", "curscol", "endcol"] {
                        dict.set(VimString::from(name), Value::Number(0));
                    }
                    return Ok(Some(Value::dict(dict)));
                };
                let raw = rvim_types::Id::raw(window_id);
                let rect = frame
                    .hit_regions
                    .iter()
                    .find(|region| {
                        region.action.kind == rvim_ui_api::frame::ActionKind::Window
                            && region.action.id == raw
                    })
                    .map(|region| region.rect);
                let Some(rect) = rect else {
                    let mut dict = VimDict::new();
                    for name in ["row", "col", "curscol", "endcol"] {
                        dict.set(VimString::from(name), Value::Number(0));
                    }
                    return Ok(Some(Value::dict(dict)));
                };
                let line = self
                    .editor
                    .buffers
                    .get(window.buffer)
                    .and_then(|buffer| buffer.text.line(lnum))
                    .unwrap_or(b"");
                let width = rvim_types::unicode::display_width(
                    &line[..(col as usize - 1).min(line.len())],
                    0,
                    window.options.tabstop.max(1),
                ) as u16;
                let row = rect
                    .row
                    .saturating_add(lnum.saturating_sub(window.viewport.topline) as u16)
                    .saturating_add(1);
                let column = rect
                    .col
                    .saturating_add(width)
                    .saturating_sub(window.viewport.leftcol)
                    .saturating_add(1);
                let mut dict = VimDict::new();
                dict.set(VimString::from("row"), Value::Number(row as i64));
                dict.set(VimString::from("col"), Value::Number(column as i64));
                dict.set(VimString::from("curscol"), Value::Number(column as i64));
                dict.set(VimString::from("endcol"), Value::Number(column as i64));
                Ok(Some(Value::dict(dict)))
            }
            "screenchar" => {"""
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("patched")
