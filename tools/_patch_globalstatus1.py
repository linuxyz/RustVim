import pathlib
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
s = s.replace("""    pub last_frame: Option<Frame>,
    pub global_statusline: bool,""", """    pub last_frame: Option<Frame>,
    pub global_statusline: bool,
    pub global_statusline_rows: u16,""", 1)
s = s.replace("""            last_frame: None,
            global_statusline: false,""", """            last_frame: None,
            global_statusline: false,
            global_statusline_rows: 0,""", 1)
# layout extra
s = s.replace("""        let extra = u16::from(self.global_statusline);""", """        let extra = self.global_statusline_rows;""", 1)
# global render block
old = """        if self.global_statusline {
            let bottom = chrome.cmdline_rows + chrome.message_rows;
            let row = self.screen.rows.saturating_sub(bottom + 1);
            if let Some(window) = self.current_window() {
                let layout =
                    Self::statusline_layout(window, host, self.screen.cols.saturating_sub(1));
                let attr = host.highlight(\"StatusLine\");
                frame
                    .base_mut()
                    .fill(Rect::new(row, 0, 1, self.screen.cols), ' ', attr);
                frame.base_mut().put_str(row, 0, &layout.text, attr);
                frame.hit_regions.push(HitRegion {
                    rect: Rect::new(row, 0, 1, self.screen.cols),
                    action: ActionId {
                        kind: ActionKind::Statusline,
                        id: rvim_types::Id::raw(window.id),
                        index: 0,
                    },
                });
                for click in &layout.clicks {
                    let width = click
                        .end
                        .saturating_sub(click.start)
                        .min(self.screen.cols.saturating_sub(click.start));
                    if width == 0 {
                        continue;
                    }
                    frame.clicks.push(ClickRegion {
                        rect: Rect::new(row, click.start, 1, width),
                        function: click.function.clone(),
                        minwid: click.minwid,
                        area: ClickArea::Statusline,
                        winid: rvim_types::Id::raw(window.id),
                        tabnr: 0,
                    });
                }
            }
        }"""
new = """        if self.global_statusline {
            let bottom = chrome.cmdline_rows + chrome.message_rows;
            let rows = self.global_statusline_rows.max(1);
            let start = self.screen.rows.saturating_sub(bottom + rows);
            if let Some(window) = self.current_window() {
                let layout =
                    Self::statusline_layout(window, host, self.screen.cols.saturating_sub(1));
                let attr = host.highlight(\"StatusLine\");
                for (index, text) in layout.text.split('\\n').enumerate().take(rows as usize) {
                    let row = start + index as u16;
                    frame
                        .base_mut()
                        .fill(Rect::new(row, 0, 1, self.screen.cols), ' ', attr);
                    frame.base_mut().put_str(row, 0, text, attr);
                    frame.hit_regions.push(HitRegion {
                        rect: Rect::new(row, 0, 1, self.screen.cols),
                        action: ActionId {
                            kind: ActionKind::Statusline,
                            id: rvim_types::Id::raw(window.id),
                            index: 0,
                        },
                    });
                }
                for click in &layout.clicks {
                    let width = click
                        .end
                        .saturating_sub(click.start)
                        .min(self.screen.cols.saturating_sub(click.start));
                    if width == 0 {
                        continue;
                    }
                    frame.clicks.push(ClickRegion {
                        rect: Rect::new(start, click.start, 1, width),
                        function: click.function.clone(),
                        minwid: click.minwid,
                        area: ClickArea::Statusline,
                        winid: rvim_types::Id::raw(window.id),
                        tabnr: 0,
                    });
                }
            }
        }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("view patched")
