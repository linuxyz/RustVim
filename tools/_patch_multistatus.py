import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
old = """            let statusline = self
                .options
                .window_string(rvim_types::Id::raw(id), "statusline");"""
new = """            let statusline = self
                .options
                .window_string(rvim_types::Id::raw(id), "statusline");
            let statuslineopt = self
                .options
                .window_string(rvim_types::Id::raw(id), "statuslineopt");
            let maxheight = statuslineopt
                .split(',')
                .find_map(|item| item.trim().strip_prefix("maxheight:"))
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(1)
                .max(1);
            let fixedheight = statuslineopt
                .split(',')
                .any(|item| item.trim() == "fixedheight");
            let newlines = statusline.matches("%@").count() as u16;"""
assert old in s
s = s.replace(old, new, 1)
s = s.replace("""                window.statusline_rows = u16::from(show_status);""", """                let wanted = if fixedheight {
                    maxheight
                } else {
                    (1 + newlines).min(maxheight)
                };
                window.statusline_rows = if show_status { wanted } else { 0 };""", 1)
p.write_text(s)
print("editor patched")
# view render multi-line
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
old2 = """        let layout = Self::statusline_layout(window, host, text_width);
        for row in 0..rows {"""
new2 = """        let layout = Self::statusline_layout(window, host, text_width);
        let lines: Vec<&str> = layout.text.split('\\n').collect();
        for row in 0..rows {"""
assert old2 in s
s = s.replace(old2, new2, 1)
old3 = """            if row == 0 {
                frame.base_mut().put_str(r, rect.status.col, &layout.text, attr);
                for click in &layout.clicks {"""
new3 = """            if let Some(text) = lines.get(row as usize) {
                frame.base_mut().put_str(r, rect.status.col, text, attr);
            }
            if row == 0 {
                for click in &layout.clicks {"""
assert old3 in s
s = s.replace(old3, new3, 1)
p.write_text(s)
print("view patched")
