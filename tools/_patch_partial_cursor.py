import pathlib
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
old = """            let mut col = screen.saturating_sub(chunk_start_screen as u32);
            if chunk_index > skipped {
                col += prefix_width as u32;
            }"""
new = """            let mut col = screen.saturating_sub(chunk_start_screen as u32);
            if chunk_index == skipped && window.cursor.lnum == topline {
                let skipcol = window.viewport.skipcol as u32;
                col = col.saturating_sub(skipcol.saturating_sub(chunk_start_screen as u32));
            }
            if chunk_index > skipped {
                col += prefix_width as u32;
            }"""
assert old in s
s = s.replace(old, new, 1)
old2 = """fn skipped_chunks(window: &Window, line: &[u8], chunks: &[(usize, usize)]) -> usize {
    if window.viewport.skipcol == 0 || chunks.is_empty() {
        return 0;
    }
    chunks
        .iter()
        .position(|(start, _)| {
            rvim_types::unicode::display_width(
                &line[..*start],
                0,
                window.options.tabstop.max(1),
            ) >= window.viewport.skipcol as usize
        })
        .unwrap_or(chunks.len().saturating_sub(1))
}"""
new2 = """fn skipped_chunks(window: &Window, line: &[u8], chunks: &[(usize, usize)]) -> usize {
    if window.viewport.skipcol == 0 || chunks.is_empty() {
        return 0;
    }
    let skipcol = window.viewport.skipcol as usize;
    chunks
        .iter()
        .position(|(_, end)| {
            rvim_types::unicode::display_width(
                &line[..*end],
                0,
                window.options.tabstop.max(1),
            ) > skipcol
        })
        .unwrap_or(chunks.len().saturating_sub(1))
}"""
assert old2 in s
s = s.replace(old2, new2, 1)
p.write_text(s)
print("patched")
