import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
old = """            let chunks = self.display_chunks_for(&line);
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
            }"""
new = """            let chunks = self.display_chunks_for(&line);
            let index = self.display_chunk_index(&chunks, pos.col as usize);
            let chunk_start_screen = self.display_col_for(&line, chunks[index].0 as u32);
            let local = self
                .display_col_for(&line, pos.col)
                .saturating_sub(chunk_start_screen);
            if down && index + 1 < chunks.len() {
                let target = chunks[index + 1];
                let col = self.byte_for_display_col(&line, target.0, local);
                self.set_cursor(BufPos::new(pos.lnum, col as u32));
                continue;
            }
            if !down && index > 0 {
                let target = chunks[index - 1];
                let col = self.byte_for_display_col(&line, target.0, local);
                self.set_cursor(BufPos::new(pos.lnum, col as u32));
                continue;
            }"""
assert old in s
s = s.replace(old, new, 1)
old2 = """            let next_chunks = self.display_chunks_for(&next_line);
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
            let col = self.byte_for_display_col(&next_line, target.0, local);"""
new2 = """            let next_chunks = self.display_chunks_for(&next_line);
            let mut target_index = next_chunks.len().saturating_sub(1);
            let mut offset = 0usize;
            for (chunk_index, (start, end)) in next_chunks.iter().enumerate() {
                let width = rvim_types::unicode::display_width(
                    &next_line[*start..*end],
                    0,
                    self.display_tabstop(),
                );
                if local < offset + width {
                    target_index = chunk_index;
                    break;
                }
                offset += width;
            }
            let target = next_chunks[target_index];
            let col = self.byte_for_display_col(&next_line, target.0, local);"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("patched")
