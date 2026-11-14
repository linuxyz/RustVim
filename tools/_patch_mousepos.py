import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub input_stack: Vec<std::collections::VecDeque<rvim_viml::Value>>,""", """    pub input_stack: Vec<std::collections::VecDeque<rvim_viml::Value>>,
    pub last_mouse: Option<(u16, u16)>,""", 1)
s = s.replace("""            input_stack: Vec::new(),""", """            input_stack: Vec::new(),
            last_mouse: None,""", 1)
s = s.replace("""    fn handle_mouse(&mut self, mouse: MouseEvent) {""", """    fn handle_mouse(&mut self, mouse: MouseEvent) {
        self.last_mouse = Some((mouse.row, mouse.col));""", 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
old = """            "getmousepos" => {
                let mut dict = VimDict::new();
                dict.set(VimString::from("screenrow"), Value::Number(0));
                dict.set(VimString::from("screencol"), Value::Number(0));
                dict.set(VimString::from("winid"), Value::Number(0));
                dict.set(VimString::from("winrow"), Value::Number(0));
                dict.set(VimString::from("wincol"), Value::Number(0));
                Ok(Some(Value::dict(dict)))
            }"""
new = """            "getmousepos" => {
                let (row, col) = self.editor.last_mouse.unwrap_or((0, 0));
                let frame = self.editor.render();
                let region = frame.hit_regions.iter().find(|region| {
                    region.action.kind == rvim_ui_api::frame::ActionKind::Window
                        && region.rect.row <= row
                        && row < region.rect.row + region.rect.rows
                        && region.rect.col <= col
                        && col < region.rect.col + region.rect.cols
                });
                let mut dict = VimDict::new();
                dict.set(VimString::from("screenrow"), Value::Number(row as i64 + 1));
                dict.set(VimString::from("screencol"), Value::Number(col as i64 + 1));
                dict.set(
                    VimString::from("winid"),
                    Value::Number(region.map_or(0, |region| region.action.id as i64)),
                );
                dict.set(
                    VimString::from("winrow"),
                    Value::Number(region.map_or(0, |region| {
                        row.saturating_sub(region.rect.row) as i64 + 1
                    })),
                );
                dict.set(
                    VimString::from("wincol"),
                    Value::Number(region.map_or(0, |region| {
                        col.saturating_sub(region.rect.col) as i64 + 1
                    })),
                );
                Ok(Some(Value::dict(dict)))
            }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("host patched")
