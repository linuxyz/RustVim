import pathlib
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
s = s.replace("""fn wrap_chunks(
    line: &[u8],
    width: usize,
    tabstop: usize,
    linebreak: bool,
    breakat: &str,
) -> Vec<(usize, usize)> {""", """fn wrap_chunks(
    line: &[u8],
    first_width: usize,
    rest_width: usize,
    tabstop: usize,
    linebreak: bool,
    breakat: &str,
) -> Vec<(usize, usize)> {""", 1)
s = s.replace("""        if used > 0 && used + cells > width {
            let mut break_at = byte;""", """        let width = if chunks.is_empty() {
            first_width
        } else {
            rest_width
        };
        if used > 0 && used + cells > width {
            let mut break_at = byte;""", 1)
# render call
s = s.replace("""            let chunks = if wrap {
                wrap_chunks(
                    line,
                    text_width as usize,
                    window.options.tabstop.max(1),
                    window.options.linebreak,
                    &window.options.breakat,
                )
            } else {
                vec![(0, line.len())]
            };""", """            let prefix = continuation_prefix(window, line);
            let prefix_width = text_width_of(&prefix, window.options.tabstop.max(1));
            let rest_width = text_width.saturating_sub(prefix_width).max(1);
            let chunks = if wrap {
                wrap_chunks(
                    line,
                    text_width as usize,
                    rest_width as usize,
                    window.options.tabstop.max(1),
                    window.options.linebreak,
                    &window.options.breakat,
                )
            } else {
                vec![(0, line.len())]
            };""", 1)
# chunk render call adjust
old = """                Self::render_line_impl(
                    frame,
                    host,
                    window,
                    lnum,
                    screen_row,
                    rect.text.col + number_width,
                    text_width,
                    if wrap { 0 } else { leftcol },
                    line,
                    line_background,
                    &searches,
                    search_attr,
                    &styles,
                    *first_byte,
                    *last_byte,
                );"""
new = """                let chunk_col = if chunk_index > 0 {
                    frame.base_mut().put_str(
                        screen_row,
                        rect.text.col + number_width,
                        &prefix,
                        host.highlight("NonText"),
                    );
                    rect.text.col + number_width + prefix_width
                } else {
                    rect.text.col + number_width
                };
                Self::render_line_impl(
                    frame,
                    host,
                    window,
                    lnum,
                    screen_row,
                    chunk_col,
                    if chunk_index > 0 { rest_width } else { text_width },
                    if wrap { 0 } else { leftcol },
                    line,
                    line_background,
                    &searches,
                    search_attr,
                    &styles,
                    *first_byte,
                    *last_byte,
                );"""
assert old in s
s = s.replace(old, new, 1)
# cursor mapping chunks
old2 = """            for lnum in topline..window.cursor.lnum {
                if let Some(line) = host.buffer_line(window.buffer, lnum) {
                    row += wrap_chunks(
                        line,
                        width as usize,
                        window.options.tabstop,
                        window.options.linebreak,
                        &window.options.breakat,
                    )
                    .len() as u32
                        - 1;
                }
                row += 1;
            }
            let screen = host.screen_col(window.buffer, window.cursor, window.options.tabstop);
            row += screen / width;
            let col = screen % width;
            if row >= text_height as u32 {
                return None;
            }
            return Some((
                rect.text.row + row as u16,
                rect.text.col + number_width + col as u16,
            ));"""
new2 = """            for lnum in topline..window.cursor.lnum {
                if let Some(line) = host.buffer_line(window.buffer, lnum) {
                    let prefix = continuation_prefix(window, line);
                    let prefix_width =
                        text_width_of(&prefix, window.options.tabstop);
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
            }
            let line = host
                .buffer_line(window.buffer, window.cursor.lnum)
                .unwrap_or(b"");
            let prefix = continuation_prefix(window, line);
            let prefix_width = text_width_of(&prefix, window.options.tabstop);
            let rest_width = (width as u16).saturating_sub(prefix_width).max(1);
            let chunks = wrap_chunks(
                line,
                width as usize,
                rest_width as usize,
                window.options.tabstop,
                window.options.linebreak,
                &window.options.breakat,
            );
            let cursor_col = window.cursor.col as usize;
            let mut chunk_index = 0usize;
            for (index, (first, last)) in chunks.iter().enumerate() {
                if cursor_col >= *first && (cursor_col < *last || index + 1 == chunks.len()) {
                    chunk_index = index;
                    break;
                }
            }
            row += chunk_index as u32;
            let first = chunks.get(chunk_index).map_or(0, |chunk| chunk.0);
            let screen = host.screen_col(window.buffer, window.cursor, window.options.tabstop);
            let chunk_start_screen =
                rvim_types::unicode::display_width(&line[..first.min(line.len())], 0, window.options.tabstop);
            let mut col = screen.saturating_sub(chunk_start_screen as u32);
            if chunk_index > 0 {
                col += prefix_width as u32;
            }
            if row >= text_height as u32 {
                return None;
            }
            return Some((
                rect.text.row + row as u16,
                rect.text.col + number_width + col as u16,
            ));"""
assert old2 in s
s = s.replace(old2, new2, 1)
# add helpers before wrap_chunks
anchor = """fn wrap_chunks(
    line: &[u8],"""
helpers = """fn continuation_prefix(window: &Window, line: &[u8]) -> String {
    let mut prefix = window.options.showbreak.clone();
    if window.options.breakindent {
        let indent = line
            .iter()
            .copied()
            .take_while(|byte| *byte == b' ' || *byte == b'\\t')
            .collect::<Vec<u8>>();
        prefix.push_str(&String::from_utf8_lossy(&indent));
    }
    prefix
}

fn text_width_of(text: &str, tabstop: usize) -> u16 {
    rvim_types::unicode::display_width(text.as_bytes(), 0, tabstop.max(1)) as u16
}

fn wrap_chunks(
    line: &[u8],"""
assert anchor in s
s = s.replace(anchor, helpers, 1)
# update tests calls
s = s.replace("wrap_chunks(b\"hello world\", 7, 8, true, \" \")", "wrap_chunks(b\"hello world\", 7, 7, 8, true, \" \")")
s = s.replace("wrap_chunks(b\"hello world\", 7, 8, false, \" \")", "wrap_chunks(b\"hello world\", 7, 7, 8, false, \" \")")
p.write_text(s)
print("patched")
