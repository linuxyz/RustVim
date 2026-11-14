import pathlib
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
old = """            if lnum == topline && window.viewport.skipcol > 0 && !chunks.is_empty() {
                let skipcol = window.viewport.skipcol as usize;
                let skip = chunks
                    .iter()
                    .position(|(start, _)| {
                        rvim_types::unicode::display_width(
                            &line[..*start],
                            0,
                            window.options.tabstop.max(1),
                        ) >= skipcol
                    })
                    .unwrap_or(chunks.len().saturating_sub(1));
                chunks.drain(..skip);
            }"""
new = """            if lnum == topline && window.viewport.skipcol > 0 && !chunks.is_empty() {
                let skipcol = window.viewport.skipcol as usize;
                let tabstop = window.options.tabstop.max(1);
                let mut partial = None;
                for (index, (start, end)) in chunks.iter().enumerate() {
                    let first = rvim_types::unicode::display_width(&line[..*start], 0, tabstop);
                    let last = rvim_types::unicode::display_width(&line[..*end], 0, tabstop);
                    if skipcol >= first && skipcol < last {
                        let offset =
                            byte_at_display_col(line, *start, skipcol - first, tabstop);
                        partial = Some((index, offset));
                        break;
                    }
                }
                if let Some((index, offset)) = partial {
                    let mut visible = vec![(offset, chunks[index].1)];
                    visible.extend_from_slice(&chunks[index + 1..]);
                    chunks = visible;
                } else {
                    let skip = chunks
                        .iter()
                        .position(|(start, _)| {
                            rvim_types::unicode::display_width(&line[..*start], 0, tabstop)
                                >= skipcol
                        })
                        .unwrap_or(chunks.len().saturating_sub(1));
                    chunks.drain(..skip);
                }
            }"""
assert old in s
s = s.replace(old, new, 1)
# helper before wrap_chunks
anchor = """fn skipped_chunks(window: &Window, line: &[u8], chunks: &[(usize, usize)]) -> usize {"""
helper = """fn byte_at_display_col(line: &[u8], start: usize, target: usize, tabstop: usize) -> usize {
    let mut used = 0usize;
    let mut index = start;
    while index < line.len() {
        let (ch, len) = if line[index] == b'\\t' {
            ('\\t', 1)
        } else if let Some((ch, len)) = char_at(line, index) {
            (ch, len)
        } else {
            ('\\u{FFFD}', 1)
        };
        let cells = if ch == '\\t' {
            tabstop.max(1) - (used % tabstop.max(1))
        } else {
            char_width(ch).max(1)
        };
        if used + cells > target {
            return index;
        }
        used += cells;
        index += len;
    }
    line.len()
}

fn skipped_chunks(window: &Window, line: &[u8], chunks: &[(usize, usize)]) -> usize {"""
assert anchor in s
s = s.replace(anchor, helper, 1)
p.write_text(s)
print("patched")
