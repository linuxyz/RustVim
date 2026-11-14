import pathlib
# add trait method in view host trait
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
anchor = """    fn message_text(&self) -> Option<&str>;"""
new = """    fn message_text(&self) -> Option<&str>;

    fn showcmd_text(&self) -> Option<String> {
        None
    }"""
assert anchor in s
s = s.replace(anchor, new, 1)
# bottom rendering showcmd
old = """        if chrome.cmdline_rows > 0 && row < rows {
            if let Some(command) = host.command_text() {
                let attr = host.highlight("CommandLine");
                frame
                    .base_mut()
                    .fill(Rect::new(row, 0, 1, self.screen.cols), ' ', attr);
                frame.base_mut().put_str(row, 0, command, attr);
            }
        }"""
new2 = """        if chrome.cmdline_rows > 0 && row < rows {
            if let Some(command) = host.command_text() {
                let attr = host.highlight("CommandLine");
                frame
                    .base_mut()
                    .fill(Rect::new(row, 0, 1, self.screen.cols), ' ', attr);
                frame.base_mut().put_str(row, 0, command, attr);
            } else if let Some(showcmd) = host.showcmd_text() {
                let width = rvim_types::unicode::display_width(showcmd.as_bytes(), 0, 8) as u16;
                if width > 0 && width <= self.screen.cols {
                    frame.base_mut().fill(
                        Rect::new(row, 0, 1, self.screen.cols),
                        ' ',
                        host.highlight("CommandLine"),
                    );
                    frame.base_mut().put_str(
                        row,
                        self.screen.cols - width,
                        &showcmd,
                        host.highlight("CommandLine"),
                    );
                }
            }
        }"""
assert old in s
s = s.replace(old, new2, 1)
p.write_text(s)
print("view patched")
# EditorHost field/method and construction
p = pathlib.Path("crates/rvim-editor/src/render.rs")
s = p.read_text()
s = s.replace("""    message: Option<String>,
    message_highlight: &'a str,""", """    message: Option<String>,
    message_highlight: &'a str,
    showcmd: Option<String>,""", 1)
s = s.replace("""    fn message_highlight(&self) -> &str {
        self.message_highlight
    }""", """    fn message_highlight(&self) -> &str {
        self.message_highlight
    }

    fn showcmd_text(&self) -> Option<String> {
        self.showcmd.clone()
    }""", 1)
s = s.replace("""        let cmdheight = self.options.get_number("cmdheight").clamp(1, 10) as u16;
        let chrome = Chrome {
            tabline_rows: if tabline.is_some() { 1 } else { 0 },
            cmdline_rows: if command.is_some() { cmdheight } else { 0 },
            message_rows: if message.is_some() { 1 } else { 0 },
        };""", """        let cmdheight = self.options.get_number("cmdheight").clamp(1, 10) as u16;
        let showcmd = self.showcmd_text();
        let chrome = Chrome {
            tabline_rows: if tabline.is_some() { 1 } else { 0 },
            cmdline_rows: if command.is_some() || showcmd.is_some() {
                cmdheight
            } else {
                0
            },
            message_rows: if message.is_some() { 1 } else { 0 },
        };""", 1)
s = s.replace("""            command,
            message,
            message_highlight,""", """            command,
            message,
            message_highlight,
            showcmd,""", 1)
p.write_text(s)
print("render patched")
