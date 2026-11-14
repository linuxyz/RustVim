import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""#[derive(Clone, Debug)]
pub struct CmdCompletion {""", """#[derive(Clone, Debug)]
pub(crate) struct TablineClick {
    pub start: u16,
    pub end: u16,
    pub function: String,
    pub minwid: u32,
}

#[derive(Clone, Debug)]
pub struct CmdCompletion {""", 1)
s = s.replace("""        let custom = self.options.get_string("tabline");
        if !custom.is_empty() {
            return Some(self.expand_tabline_template(&custom));
        }""", """        let custom = self.options.get_string("tabline");
        if !custom.is_empty() {
            return Some(self.expand_tabline_template_with_clicks(&custom).0);
        }""", 1)
start = s.index("    fn expand_tabline_template(&self, template: &str) -> String {")
end = s.index("    fn create_buffer(&mut self, path: Option<&Path>) -> Result<BufferId, VimError> {", start)
new = """    pub(crate) fn tabline_clicks(&self) -> Vec<TablineClick> {
        let show = self.options.get_number("showtabline");
        if show == 0 || (self.view.tab_count() <= 1 && show < 2) {
            return Vec::new();
        }
        let custom = self.options.get_string("tabline");
        if custom.is_empty() {
            return Vec::new();
        }
        self.expand_tabline_template_with_clicks(&custom).1
    }

    fn expand_tabline_template_with_clicks(
        &self,
        template: &str,
    ) -> (String, Vec<TablineClick>) {
        let tabs = self.view.tab_ids();
        let current_name = self
            .current_buffer()
            .map(|buffer| buffer.short_name())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "[No Name]".to_string());
        let mut out = String::new();
        let mut col = 0u16;
        let mut clicks = Vec::new();
        let mut pending: Option<(u16, String, u32)> = None;
        let mut chars = template.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                let mut buffer = [0u8; 4];
                let text = ch.encode_utf8(&mut buffer);
                out.push_str(text);
                col = col.saturating_add(rvim_types::unicode::char_width(ch).max(1) as u16);
                continue;
            }
            match chars.next() {
                Some('%') => {
                    out.push('%');
                    col = col.saturating_add(1);
                }
                Some('t') | Some('f') => {
                    out.push_str(&current_name);
                    col = col.saturating_add(
                        current_name
                            .chars()
                            .map(|ch| rvim_types::unicode::char_width(ch).max(1) as u16)
                            .sum::<u16>(),
                    );
                }
                Some('F') => {
                    let name = self
                        .current_buffer()
                        .and_then(|buffer| buffer.text.meta.name.as_ref())
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "[No Name]".to_string());
                    out.push_str(&name);
                    col = col.saturating_add(
                        name.chars()
                            .map(|ch| rvim_types::unicode::char_width(ch).max(1) as u16)
                            .sum::<u16>(),
                    );
                }
                Some('m') => {
                    if self
                        .current_buffer()
                        .is_some_and(|buffer| buffer.modified())
                    {
                        out.push_str("[+]");
                        col = col.saturating_add(3);
                    }
                }
                Some('=') => {}
                Some('#') => {
                    for next in chars.by_ref() {
                        if next == '#' {
                            break;
                        }
                    }
                }
                Some('[') => {
                    let mut function = String::new();
                    let mut terminated = false;
                    for next in chars.by_ref() {
                        if next == ']' {
                            terminated = true;
                            break;
                        }
                        function.push(next);
                    }
                    if terminated {
                        close_tabline_click(&mut pending, col, &mut clicks);
                        if !function.is_empty() {
                            pending = Some((col, function, 0));
                        }
                    }
                }
                Some(digit) if digit.is_ascii_digit() => {
                    let mut number = digit.to_digit(10).unwrap_or(0);
                    while let Some(next) = chars.peek() {
                        if let Some(value) = next.to_digit(10) {
                            number = number * 10 + value;
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if chars.peek() == Some(&'[') {
                        chars.next();
                        let mut function = String::new();
                        let mut terminated = false;
                        for next in chars.by_ref() {
                            if next == ']' {
                                terminated = true;
                                break;
                            }
                            function.push(next);
                        }
                        if terminated && !function.is_empty() {
                            close_tabline_click(&mut pending, col, &mut clicks);
                            pending = Some((col, function, number));
                        }
                    } else if chars.peek() == Some(&'T') {
                        chars.next();
                        if let Some(tab) = tabs.get(number.saturating_sub(1) as usize) {
                            let name = self
                                .view
                                .tabs
                                .get(tab)
                                .and_then(|page| self.view.windows.get(&page.current))
                                .and_then(|window| self.buffers.get(window.buffer))
                                .map(|buffer| {
                                    let name = buffer.short_name();
                                    if name.is_empty() {
                                        "[No Name]".to_string()
                                    } else {
                                        name
                                    }
                                })
                                .unwrap_or_else(|| "[No Name]".to_string());
                            let label = format!("[{number}] {name} ");
                            col = col.saturating_add(
                                label
                                    .chars()
                                    .map(|ch| rvim_types::unicode::char_width(ch).max(1) as u16)
                                    .sum::<u16>(),
                            );
                            out.push_str(&label);
                        }
                    }
                }
                Some(other) => {
                    out.push(other);
                    col = col.saturating_add(1);
                }
                None => {
                    out.push('%');
                    col = col.saturating_add(1);
                }
            }
        }
        close_tabline_click(&mut pending, col, &mut clicks);
        (out, clicks)
    }

    fn expand_tabline_template(&self, template: &str) -> String {
        self.expand_tabline_template_with_clicks(template).0
    }

"""
s = s[:start] + new + s[end:]
s += """

fn close_tabline_click(
    pending: &mut Option<(u16, String, u32)>,
    col: u16,
    clicks: &mut Vec<TablineClick>,
) {
    if let Some((start, function, minwid)) = pending.take() {
        if col > start {
            clicks.push(TablineClick {
                start,
                end: col,
                function,
                minwid,
            });
        }
    }
}
"""
p.write_text(s)
print("editor patched")
