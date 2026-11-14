import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
# struct
s = s.replace("""#[derive(Clone, Debug)]
pub(crate) struct TablineClick {""", """#[derive(Clone, Copy, Debug)]
struct BorderDrag {
    window: WindowId,
    vertical: bool,
    last_row: u16,
    last_col: u16,
}

#[derive(Clone, Debug)]
pub(crate) struct TablineClick {""", 1)
# field
s = s.replace("""    insert_completion: Option<InsertCompletion>,""", """    insert_completion: Option<InsertCompletion>,
    border_drag: Option<BorderDrag>,""", 1)
s = s.replace("""            insert_completion: None,""", """            insert_completion: None,
            border_drag: None,""", 1)
# early drag handling after fn handle_mouse
anchor = """    fn handle_mouse(&mut self, mouse: MouseEvent) {
        let click_count = if mouse.kind == MouseKind::Press {"""
new = """    fn handle_mouse(&mut self, mouse: MouseEvent) {
        if mouse.kind == MouseKind::Release {
            self.border_drag = None;
        }
        if let Some(drag) = self.border_drag {
            if mouse.kind == MouseKind::Drag {
                let delta = if drag.vertical {
                    mouse.col as i32 - drag.last_col as i32
                } else {
                    mouse.row as i32 - drag.last_row as i32
                };
                if delta != 0
                    && self
                        .view
                        .resize_window(drag.window, drag.vertical, delta)
                {
                    self.border_drag = Some(BorderDrag {
                        window: drag.window,
                        vertical: drag.vertical,
                        last_row: mouse.row,
                        last_col: mouse.col,
                    });
                    self.sync_window_options();
                    self.request_redraw();
                }
                return;
            }
        }
        let click_count = if mouse.kind == MouseKind::Press {"""
assert anchor in s
s = s.replace(anchor, new, 1)
# split Statusline | WindowBorder branch
old = """                ActionKind::Statusline | ActionKind::WindowBorder => {
                    let target_window = {
                        self.view
                            .windows
                            .iter()
                            .find(|(_, window)| rvim_types::Id::raw(window.id) == region.action.id)
                            .map(|(id, _)| id)
                    };
                    if let Some(window) = target_window {
                        self.view.set_current_window(window);
                        self.current_buffer = self
                            .view
                            .current_window()
                            .map(|window| window.buffer)
                            .unwrap_or(self.current_buffer);
                        self.sync_window_options();
                        self.request_redraw();
                    }
                    return;
                }"""
new2 = """                ActionKind::Statusline => {
                    let target_window = {
                        self.view
                            .windows
                            .iter()
                            .find(|(_, window)| rvim_types::Id::raw(window.id) == region.action.id)
                            .map(|(id, _)| id)
                    };
                    if let Some(window) = target_window {
                        self.view.set_current_window(window);
                        self.current_buffer = self
                            .view
                            .current_window()
                            .map(|window| window.buffer)
                            .unwrap_or(self.current_buffer);
                        self.sync_window_options();
                        self.request_redraw();
                    }
                    return;
                }
                ActionKind::WindowBorder => {
                    let target_window = {
                        self.view
                            .windows
                            .iter()
                            .find(|(_, window)| rvim_types::Id::raw(window.id) == region.action.id)
                            .map(|(id, _)| id)
                    };
                    if let Some(window) = target_window {
                        self.view.set_current_window(window);
                        self.current_buffer = self
                            .view
                            .current_window()
                            .map(|window| window.buffer)
                            .unwrap_or(self.current_buffer);
                        self.sync_window_options();
                        if mouse.kind == MouseKind::Press {
                            let vertical = self
                                .view
                                .layout_rects(0, 0)
                                .iter()
                                .find(|rect| {
                                    rvim_types::Id::raw(rect.window) == region.action.id
                                })
                                .and_then(|rect| rect.separator)
                                .map(|separator| separator.cols == 1)
                                .unwrap_or(true);
                            self.border_drag = Some(BorderDrag {
                                window,
                                vertical,
                                last_row: mouse.row,
                                last_col: mouse.col,
                            });
                        }
                        self.request_redraw();
                    }
                    return;
                }"""
assert old in s
s = s.replace(old, new2, 1)
p.write_text(s)
print("patched")
