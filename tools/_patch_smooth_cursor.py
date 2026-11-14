import pathlib
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
old = """            let width = window.text_width() as u32;
            let mut row = 0u32;
            for lnum in topline..window.cursor.lnum {
                if let Some(line) = host.buffer_line(window.buffer, lnum) {
                    let prefix = continuation_prefix(window, line);
                    let prefix_width = text_width_of(&prefix, window.options.tabstop);
                    let rest_width = width as u16 - prefix_width.min(width as u16);
                    row += wrap_chunks(
                        line,
                        width as usize,
                        rest_width.max(1) as usize,
                        window.options.tabstop,
                        window.options.linebreak,
                        &window.options.breakat,
                    )
                    .len() as u32;
                }
            }"""
new = """            let width = window.text_width() as u32;
            let mut row = 0u32;
            for lnum in topline..window.cursor.lnum {
                if let Some(line) = host.buffer_line(window.buffer, lnum) {
                    let prefix = continuation_prefix(window, line);
                    let prefix_width = text_width_of(&prefix, window.options.tabstop);
                    let rest_width = width as u16 - prefix_width.min(width as u16);
                    let chunks = wrap_chunks(
                        line,
                        width as usize,
                        rest_width.max(1) as usize,
                        window.options.tabstop,
                        window.options.linebreak,
                        &window.options.breakat,
                    );
                    let skipped = if lnum == topline {
                        skipped_chunks(window, line, &chunks)
                    } else {
                        0
                    };
                    row += chunks.len().saturating_sub(skipped) as u32;
                }
            }"""
assert old in s
s = s.replace(old, new, 1)
old2 = """            row += chunk_index as u32;
            let first = chunks.get(chunk_index).map_or(0, |chunk| chunk.0);
            let screen = host.screen_col(window.buffer, window.cursor, window.options.tabstop);
            let chunk_start_screen = rvim_types::unicode::display_width(
                &line[..first.min(line.len())],
                0,
                window.options.tabstop,
            );
            let mut col = screen.saturating_sub(chunk_start_screen as u32);
            if chunk_index > 0 {
                col += prefix_width as u32;
            }"""
new2 = """            let skipped = if window.cursor.lnum == topline {
                skipped_chunks(window, line, &chunks)
            } else {
                0
            };
            if chunk_index < skipped {
                return None;
            }
            row += (chunk_index - skipped) as u32;
            let first = chunks.get(chunk_index).map_or(0, |chunk| chunk.0);
            let screen = host.screen_col(window.buffer, window.cursor, window.options.tabstop);
            let chunk_start_screen = rvim_types::unicode::display_width(
                &line[..first.min(line.len())],
                0,
                window.options.tabstop,
            );
            let mut col = screen.saturating_sub(chunk_start_screen as u32);
            if chunk_index > skipped {
                col += prefix_width as u32;
            }"""
assert old2 in s
s = s.replace(old2, new2, 1)
# add helper after text_width_of
anchor = """fn text_width_of(text: &str, tabstop: usize) -> u16 {
    rvim_types::unicode::display_width(text.as_bytes(), 0, tabstop.max(1)) as u16
}"""
helper = """fn text_width_of(text: &str, tabstop: usize) -> u16 {
    rvim_types::unicode::display_width(text.as_bytes(), 0, tabstop.max(1)) as u16
}

fn skipped_chunks(window: &Window, line: &[u8], chunks: &[(usize, usize)]) -> usize {
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
assert anchor in s
s = s.replace(anchor, helper, 1)
p.write_text(s)
print("patched")
