import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    fn handle_g_key(&mut self, key: KeyEvent) {"""
methods = """    fn display_chunks_for(&self, line: &[u8]) -> Vec<(usize, usize)> {
        let Some(window) = self.view.current_window() else {
            return vec![(0, line.len())];
        };
        if !window.options.wrap {
            return vec![(0, line.len())];
        }
        rvim_view::display_chunks(line, window.text_width(), &window.options)
    }

    fn display_prefix_for(&self, line: &[u8]) -> u16 {
        let Some(window) = self.view.current_window() else {
            return 0;
        };
        rvim_view::display_prefix_width(line, window.text_width(), &window.options)
    }

    fn display_tabstop(&self) -> usize {
        self.view
            .current_window()
            .map_or(8, |window| window.options.tabstop.max(1))
    }

    fn byte_for_display_col(&self, line: &[u8], start: usize, target: usize) -> usize {
        let tabstop = self.display_tabstop();
        let mut used = 0usize;
        let mut index = start;
        while index < line.len() {
            let (ch, len) = if line[index] == b'\\t' {
                ('\\t', 1)
            } else if let Some((ch, len)) = rvim_types::unicode::char_at(line, index) {
                (ch, len)
            } else {
                ('\\u{FFFD}', 1)
            };
            let cells = if ch == '\\t' {
                tabstop - (used % tabstop)
            } else {
                rvim_types::unicode::char_width(ch).max(1)
            };
            if used + cells > target {
                return index;
            }
            used += cells;
            index += len;
        }
        line.len()
    }

    fn display_col_for(&self, line: &[u8], col: u32) -> usize {
        let tabstop = self.display_tabstop();
        rvim_types::unicode::display_width(line, 0, tabstop).min(usize::MAX)
            * 0
            + rvim_types::unicode::display_width(
                &line[..(col as usize).min(line.len())],
                0,
                tabstop,
            )
    }

    fn display_chunk_index(&self, chunks: &[(usize, usize)], col: usize) -> usize {
        chunks
            .iter()
            .position(|(start, end)| col >= *start && col < *end)
            .unwrap_or_else(|| chunks.len().saturating_sub(1))
    }

    fn goto_display_edge(&mut self, middle: bool, end: bool) {
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let chunks = self.display_chunks_for(&line);
        let index = self.display_chunk_index(&chunks, pos.col as usize);
        let (start, last) = chunks[index];
        let prefix = if index > 0 {
            self.display_prefix_for(&line) as usize
        } else {
            0
        };
        let start_col = if index > 0 { start + 0 } else { start };
        let prefix_cells = prefix;
        let content_start = start_col;
        let content_end = last;
        let content_width = if content_end > content_start {
            rvim_types::unicode::display_width(
                &line[content_start..content_end],
                0,
                self.display_tabstop(),
            )
        } else {
            0
        };
        let target = if middle {
            prefix_cells + content_width / 2
        } else if end {
            prefix_cells + content_width.saturating_sub(1)
        } else {
            0
        };
        let col = self.byte_for_display_col(&line, content_start, target.saturating_sub(prefix_cells));
        self.set_cursor(BufPos::new(pos.lnum, col as u32));
    }

    fn move_display_line(&mut self, down: bool, count: u32) {
        for _ in 0..count.max(1) {
            let pos = self.cursor();
            let Some(line) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(pos.lnum))
                .map(|line| line.to_vec())
            else {
                return;
            };
            let chunks = self.display_chunks_for(&line);
            let index = self.display_chunk_index(&chunks, pos.col as usize);
            let prefix = self.display_prefix_for(&line) as usize;
            let screen = if index > 0 {
                prefix + self.display_col_for(&line, pos.col)
            } else {
                self.display_col_for(&line, pos.col)
            };
            if down && index + 1 < chunks.len() {
                let target = chunks[index + 1];
                let local = screen.saturating_sub(prefix);
                let col = self.byte_for_display_col(&line, target.0, local);
                self.set_cursor(BufPos::new(pos.lnum, col as u32));
                continue;
            }
            if !down && index > 0 {
                let target = chunks[index - 1];
                let local = screen.saturating_sub(prefix);
                let col = self.byte_for_display_col(&line, target.0, local);
                self.set_cursor(BufPos::new(pos.lnum, col as u32));
                continue;
            }
            let next_lnum = if down {
                (pos.lnum + 1).min(self.line_count().max(1))
            } else {
                pos.lnum.saturating_sub(1).max(1)
            };
            if next_lnum == pos.lnum {
                break;
            }
            let Some(next_line) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(next_lnum))
                .map(|line| line.to_vec())
            else {
                break;
            };
            let next_chunks = self.display_chunks_for(&next_line);
            let next_prefix = self.display_prefix_for(&next_line) as usize;
            let mut target_index = next_chunks.len().saturating_sub(1);
            for (chunk_index, (start, end)) in next_chunks.iter().enumerate() {
                let start_screen = if chunk_index > 0 { next_prefix } else { 0 };
                let width = rvim_types::unicode::display_width(
                    &next_line[*start..*end],
                    0,
                    self.display_tabstop(),
                );
                if screen < start_screen + width {
                    target_index = chunk_index;
                    break;
                }
            }
            let target = next_chunks[target_index];
            let local = screen.saturating_sub(if target_index > 0 { next_prefix } else { 0 });
            let col = self.byte_for_display_col(&next_line, target.0, local);
            self.set_cursor(BufPos::new(next_lnum, col as u32));
        }
    }

    fn handle_g_key(&mut self, key: KeyEvent) {"""
assert anchor in s
s = s.replace(anchor, methods, 1)
# change g match arms
s = s.replace("""            'j' => self.move_count(Motion::Down),
            'k' => self.move_count(Motion::Up),
            'h' => self.move_count(Motion::Left),
            'l' => self.move_count(Motion::Right),
            '0' => self.move_count(Motion::LineStart),
            '^' => self.move_count(Motion::FirstNonBlank),
            '$' => self.move_count(Motion::LineEnd),""", """            'j' => self.move_display_line(true, count),
            'k' => self.move_display_line(false, count),
            'h' => self.move_count(Motion::Left),
            'l' => self.move_count(Motion::Right),
            '0' => self.goto_display_edge(false, false),
            '^' => self.move_count(Motion::FirstNonBlank),
            '$' => self.goto_display_edge(false, true),
            'm' => self.goto_display_edge(true, false),""", 1)
p.write_text(s)
print("patched")
