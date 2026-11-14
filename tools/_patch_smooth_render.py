import pathlib
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
old = """            let chunks = if wrap {
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
            };"""
new = """            let mut chunks = if wrap {
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
            };
            if lnum == topline && window.viewport.skipcol > 0 && !chunks.is_empty() {
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
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("patched")
