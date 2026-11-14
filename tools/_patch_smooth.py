import pathlib
p = pathlib.Path("crates/rvim-view/src/window.rs")
s = p.read_text()
s = s.replace("""    pub showbreak: String,
    pub breakindent: bool,""", """    pub showbreak: String,
    pub breakindent: bool,
    pub smoothscroll: bool,""", 1)
s = s.replace("""            showbreak: String::new(),
            breakindent: false,""", """            showbreak: String::new(),
            breakindent: false,
            smoothscroll: false,""", 1)
p.write_text(s)
print("window patched")
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""            let breakindent = self
                .options
                .window_bool(rvim_types::Id::raw(id), "breakindent");""", """            let breakindent = self
                .options
                .window_bool(rvim_types::Id::raw(id), "breakindent");
            let smoothscroll = self
                .options
                .window_bool(rvim_types::Id::raw(id), "smoothscroll");""", 1)
s = s.replace("""                window.options.breakindent = breakindent;""", """                window.options.breakindent = breakindent;
                window.options.smoothscroll = smoothscroll;""", 1)
# set_cursor reset skipcol
s = s.replace("""        if let Some(window) = self.view.current_window_mut() {
            window.set_cursor(pos);
        }
    }

    pub fn clamp_cursor""", """        if let Some(window) = self.view.current_window_mut() {
            window.set_cursor(pos);
            window.viewport.skipcol = 0;
        }
    }

    pub fn clamp_cursor""", 1)
# scroll_line signature and body
old = """    fn scroll_line(&mut self, down: bool) {
        let max_top = self.line_count().max(1);
        if let Some(window) = self.view.current_window_mut() {
            if down {
                if window.viewport.topline < max_top {
                    window.viewport.topline += 1;
                }
            } else if window.viewport.topline > 1 {
                window.viewport.topline -= 1;
            }
        }
    }"""
new = """    fn scroll_line(&mut self, down: bool, count: u32) {
        let max_top = self.line_count().max(1);
        for _ in 0..count.max(1) {
            let smooth = self
                .view
                .current_window()
                .is_some_and(|window| window.options.smoothscroll && window.options.wrap);
            if smooth {
                let Some(window) = self.view.current_window() else {
                    return;
                };
                let topline = window.viewport.topline;
                let skipcol = window.viewport.skipcol as usize;
                let width = window.text_width();
                let options = window.options.clone();
                let Some(line) = self
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(topline))
                    .map(|line| line.to_vec())
                else {
                    return;
                };
                let chunks = rvim_view::display_chunks(&line, width, &options);
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
                        if let Some(window) = self.view.current_window_mut() {
                            window.viewport.skipcol = *next as u32;
                        }
                    } else if topline < max_top {
                        if let Some(window) = self.view.current_window_mut() {
                            window.viewport.topline += 1;
                            window.viewport.skipcol = 0;
                        }
                    }
                } else if index > 0 {
                    if let Some(window) = self.view.current_window_mut() {
                        window.viewport.skipcol = starts[index - 1] as u32;
                    }
                } else if topline > 1 {
                    let previous = topline - 1;
                    let previous_line = self
                        .current_buffer()
                        .and_then(|buffer| buffer.text.line(previous))
                        .map(|line| line.to_vec());
                    let previous_starts = previous_line.map(|line| {
                        rvim_view::display_chunks(&line, width, &options)
                            .iter()
                            .map(|(start, _)| {
                                rvim_types::unicode::display_width(
                                    &line[..*start],
                                    0,
                                    options.tabstop.max(1),
                                )
                            })
                            .collect::<Vec<_>>()
                    });
                    if let Some(window) = self.view.current_window_mut() {
                        window.viewport.topline = previous;
                        window.viewport.skipcol = previous_starts
                            .and_then(|starts| starts.last().copied())
                            .unwrap_or(0) as u32;
                    }
                }
            } else if let Some(window) = self.view.current_window_mut() {
                window.viewport.skipcol = 0;
                if down {
                    if window.viewport.topline < max_top {
                        window.viewport.topline += 1;
                    }
                } else if window.viewport.topline > 1 {
                    window.viewport.topline -= 1;
                }
            }
        }
    }"""
assert old in s
s = s.replace(old, new, 1)
# calls pass count
s = s.replace("""                self.scroll_line(true);""", """                self.scroll_line(true, count);""", 1)
s = s.replace("""                self.scroll_line(false);""", """                self.scroll_line(false, count);""", 1)
p.write_text(s)
print("editor patched")
