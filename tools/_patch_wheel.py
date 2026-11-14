import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
old = """        if mouse.kind == MouseKind::Wheel {
            if let Some(window) = self.view.window_mut(hit.window) {
                match mouse.button {
                    MouseButton::WheelUp => {
                        window.viewport.topline = window.viewport.topline.saturating_sub(3).max(1);
                    }
                    MouseButton::WheelDown => {
                        window.viewport.topline = (window.viewport.topline + 3).min(line_count);
                    }
                    _ => {}
                }
            }
            self.request_redraw();
            return;
        }"""
new = """        if mouse.kind == MouseKind::Wheel {
            match mouse.button {
                MouseButton::WheelUp => self.scroll_window_display(hit.window, false, 3),
                MouseButton::WheelDown => self.scroll_window_display(hit.window, true, 3),
                _ => {}
            }
            self.request_redraw();
            return;
        }"""
assert old in s
s = s.replace(old, new, 1)
anchor = """    fn screen_to_buffer_pos("""
method = """    fn scroll_window_display(&mut self, window_id: WindowId, down: bool, count: u32) {
        let Some(window) = self.view.windows.get(&window_id) else {
            return;
        };
        let buffer_id = window.buffer;
        let options = window.options.clone();
        let width = window.text_width();
        let mut topline = window.viewport.topline;
        let mut skipcol = window.viewport.skipcol as usize;
        let Some(buffer) = self.buffers.get(buffer_id) else {
            return;
        };
        let max_top = buffer.text.line_count().max(1);
        let smooth = options.smoothscroll && options.wrap;
        for _ in 0..count.max(1) {
            if !smooth {
                if down {
                    topline = (topline + 1).min(max_top);
                } else {
                    topline = topline.saturating_sub(1).max(1);
                }
                skipcol = 0;
                continue;
            }
            let Some(line) = buffer.text.line(topline) else {
                break;
            };
            let chunks = rvim_view::display_chunks(line, width, &options);
            let starts: Vec<usize> = chunks
                .iter()
                .map(|(start, _)| {
                    rvim_types::unicode::display_width(
                        &line[..*start],
                        0,
                        options.tabstop.max(1),
                    )
                })
                .collect();
            let index = starts
                .iter()
                .rposition(|start| *start <= skipcol)
                .unwrap_or(0);
            if down {
                if let Some(next) = starts.get(index + 1) {
                    skipcol = *next;
                } else if topline < max_top {
                    topline += 1;
                    skipcol = 0;
                }
            } else if index > 0 {
                skipcol = starts[index - 1];
            } else if topline > 1 {
                topline -= 1;
                skipcol = buffer
                    .text
                    .line(topline)
                    .map(|line| {
                        rvim_view::display_chunks(line, width, &options)
                            .iter()
                            .map(|(start, _)| {
                                rvim_types::unicode::display_width(
                                    &line[..*start],
                                    0,
                                    options.tabstop.max(1),
                                )
                            })
                            .last()
                            .unwrap_or(0)
                    })
                    .unwrap_or(0);
            }
        }
        if let Some(window) = self.view.window_mut(window_id) {
            window.viewport.topline = topline;
            window.viewport.skipcol = skipcol as u32;
        }
    }

    fn screen_to_buffer_pos("""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
