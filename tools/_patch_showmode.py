import pathlib
# trait method
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
s = s.replace("""    fn showcmd_text(&self) -> Option<String> {
        None
    }""", """    fn showcmd_text(&self) -> Option<String> {
        None
    }

    fn showmode_text(&self) -> Option<String> {
        None
    }""", 1)
# bottom rendering add showmode before showcmd branch
old = """            } else if let Some(showcmd) = host.showcmd_text() {
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
            }"""
new = """            } else {
                let showmode = host.showmode_text();
                let showcmd = host.showcmd_text();
                if showmode.is_some() || showcmd.is_some() {
                    frame.base_mut().fill(
                        Rect::new(row, 0, 1, self.screen.cols),
                        ' ',
                        host.highlight("CommandLine"),
                    );
                }
                if let Some(showmode) = showmode {
                    frame
                        .base_mut()
                        .put_str(row, 0, &showmode, host.highlight("CommandLine"));
                }
                if let Some(showcmd) = showcmd {
                    let width =
                        rvim_types::unicode::display_width(showcmd.as_bytes(), 0, 8) as u16;
                    if width > 0 && width <= self.screen.cols {
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
s = s.replace(old, new, 1)
p.write_text(s)
print("view patched")
# EditorHost
p = pathlib.Path("crates/rvim-editor/src/render.rs")
s = p.read_text()
s = s.replace("""    showcmd: Option<String>,""", """    showcmd: Option<String>,
    showmode: Option<String>,""", 1)
s = s.replace("""    fn showcmd_text(&self) -> Option<String> {
        self.showcmd.clone()
    }""", """    fn showcmd_text(&self) -> Option<String> {
        self.showcmd.clone()
    }

    fn showmode_text(&self) -> Option<String> {
        self.showmode.clone()
    }""", 1)
s = s.replace("""        let showcmd = self.showcmd_text();
        let chrome = Chrome {""", """        let showcmd = self.showcmd_text();
        let showmode = self.showmode_text();
        let chrome = Chrome {""", 1)
s = s.replace("""            cmdline_rows: if command.is_some() || showcmd.is_some() {
                cmdheight
            } else {
                0
            },""", """            cmdline_rows: if command.is_some() || showcmd.is_some() || showmode.is_some() {
                cmdheight
            } else {
                0
            },""", 1)
s = s.replace("""            message_highlight,
            showcmd,""", """            message_highlight,
            showcmd,
            showmode,""", 1)
p.write_text(s)
print("render patched")
