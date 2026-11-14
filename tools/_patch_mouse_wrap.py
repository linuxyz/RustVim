import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    fn handle_mouse(&mut self, mouse: MouseEvent) {"""
method = """    fn screen_to_buffer_pos(
        &self,
        window_id: WindowId,
        row: u16,
        text_col: u16,
        line_count: u32,
    ) -> BufPos {
        let Some(window) = self.view.windows.get(&window_id) else {
            return self.cursor();
        };
        let options = window.options.clone();
        let topline = window.viewport.topline;
        let leftcol = window.viewport.leftcol;
        let skipcol = window.viewport.skipcol as usize;
        let width = window.text_width();
        let Some(buffer) = self.buffers.get(window.buffer) else {
            return self.cursor();
        };
        let tabstop = options.tabstop.max(1);
        if !options.wrap {
            let lnum = (topline + row as u32).min(line_count.max(1));
            let display_col = leftcol + text_col as u32;
            let col = rvim_edit::byte_col(&buffer.text, lnum, display_col, tabstop);
            return BufPos::new(lnum, col);
        }
        let mut lnum = topline;
        let mut remaining = row as u32;
        while lnum <= line_count.max(1) {
            let line = buffer.text.line(lnum).unwrap_or(&[]);
            let chunks = rvim_view::display_chunks(line, width, &options);
            let skipped = if lnum == topline && skipcol > 0 && !chunks.is_empty() {
                chunks
                    .iter()
                    .position(|(start, _)| {
                        rvim_types::unicode::display_width(
                            &line[..*start],
                            0,
                            tabstop,
                        ) >= skipcol
                    })
                    .unwrap_or(chunks.len().saturating_sub(1))
            } else {
                0
            };
            let visible = chunks.len().saturating_sub(skipped);
            if remaining < visible as u32 {
                let index = skipped + remaining as usize;
                let (start, _) = chunks[index];
                let prefix = if index > skipped {
                    rvim_view::display_prefix_width(line, width, &options) as u32
                } else {
                    0
                };
                let chunk_start = rvim_types::unicode::display_width(&line[..start], 0, tabstop);
                let display_col = chunk_start + (text_col as u32).saturating_sub(prefix);
                let col = rvim_edit::byte_col(&buffer.text, lnum, display_col, tabstop);
                return BufPos::new(lnum, col);
            }
            remaining -= visible as u32;
            lnum += 1;
        }
        BufPos::new(line_count.max(1), 0)
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("patched")
