import pathlib
p = pathlib.Path("crates/rvim-view/src/window.rs")
s = p.read_text()
s = s.replace("""    pub wrap: bool,
    pub list: bool,""", """    pub wrap: bool,
    pub linebreak: bool,
    pub breakat: String,
    pub list: bool,""", 1)
s = s.replace("""            wrap: true,
            list: false,""", """            wrap: true,
            linebreak: false,
            breakat: String::new(),
            list: false,""", 1)
p.write_text(s)
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""            let wrap = self.options.window_bool(rvim_types::Id::raw(id), "wrap");""", """            let wrap = self.options.window_bool(rvim_types::Id::raw(id), "wrap");
            let linebreak = self
                .options
                .window_bool(rvim_types::Id::raw(id), "linebreak");
            let breakat = self
                .options
                .window_string(rvim_types::Id::raw(id), "breakat");""", 1)
s = s.replace("""                window.options.wrap = wrap;""", """                window.options.wrap = wrap;
                window.options.linebreak = linebreak;
                window.options.breakat = breakat;""", 1)
p.write_text(s)
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
s = s.replace("wrap_chunks(line, text_width as usize, window.options.tabstop.max(1))", "wrap_chunks(\n                    line,\n                    text_width as usize,\n                    window.options.tabstop.max(1),\n                    window.options.linebreak,\n                    &window.options.breakat,\n                )", 1)
s = s.replace("wrap_chunks(line, width as usize, window.options.tabstop).len() as u32 - 1", "wrap_chunks(\n                            line,\n                            width as usize,\n                            window.options.tabstop,\n                            window.options.linebreak,\n                            &window.options.breakat,\n                        )\n                        .len() as u32\n                            - 1", 1)
old = """fn wrap_chunks(line: &[u8], width: usize, tabstop: usize) -> Vec<(usize, usize)> {"""
new = """fn wrap_chunks(
    line: &[u8],
    width: usize,
    tabstop: usize,
    linebreak: bool,
    breakat: &str,
) -> Vec<(usize, usize)> {"""
assert old in s
s = s.replace(old, new, 1)
old2 = """        if used > 0 && used + cells > width {
            chunks.push((start, byte));
            start = byte;
            used = 0;
            continue;
        }"""
new2 = """        if used > 0 && used + cells > width {
            let mut break_at = byte;
            if linebreak && !breakat.is_empty() {
                let mut candidate = None;
                let mut index = start;
                while index < byte {
                    let (ch, len) = if line[index] == b'\\t' {
                        ('\\t', 1)
                    } else if let Some((ch, len)) = char_at(line, index) {
                        (ch, len)
                    } else {
                        ('\\u{FFFD}', 1)
                    };
                    if breakat.contains(ch) {
                        candidate = Some(index + len);
                    }
                    index += len;
                }
                if let Some(at) = candidate {
                    if at > start {
                        break_at = at;
                    }
                }
            }
            chunks.push((start, break_at));
            start = break_at;
            used = 0;
            byte = break_at;
            continue;
        }"""
assert old2 in s
s = s.replace(old2, new2, 1)
s += """

#[cfg(test)]
mod wrap_tests {
    use super::wrap_chunks;

    #[test]
    fn linebreak_prefers_breakat_boundaries() {
        assert_eq!(
            wrap_chunks(b"hello world", 7, 8, true, " "),
            vec![(0, 6), (6, 11)]
        );
        assert_eq!(
            wrap_chunks(b"hello world", 7, 8, false, " "),
            vec![(0, 7), (7, 11)]
        );
    }
}
"""
p.write_text(s)
print("patched")
