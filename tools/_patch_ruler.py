import pathlib
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
s = s.replace("""    fn showmode_text(&self) -> Option<String> {
        None
    }""", """    fn showmode_text(&self) -> Option<String> {
        None
    }

    fn ruler_text(&self) -> Option<String> {
        None
    }""", 1)
old = """                if let Some(showcmd) = showcmd {
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
                }"""
new = """                let right = showcmd.or_else(|| host.ruler_text());
                if let Some(right) = right {
                    let width =
                        rvim_types::unicode::display_width(right.as_bytes(), 0, 8) as u16;
                    if width > 0 && width <= self.screen.cols {
                        frame.base_mut().put_str(
                            row,
                            self.screen.cols - width,
                            &right,
                            host.highlight("CommandLine"),
                        );
                    }
                }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("view patched")
p = pathlib.Path("crates/rvim-editor/src/render.rs")
s = p.read_text()
s = s.replace("""    showmode: Option<String>,""", """    showmode: Option<String>,
    ruler: Option<String>,""", 1)
s = s.replace("""    fn showmode_text(&self) -> Option<String> {
        self.showmode.clone()
    }""", """    fn showmode_text(&self) -> Option<String> {
        self.showmode.clone()
    }

    fn ruler_text(&self) -> Option<String> {
        self.ruler.clone()
    }""", 1)
s = s.replace("""        let showmode = self.showmode_text();
        let chrome = Chrome {""", """        let showmode = self.showmode_text();
        let ruler = self.ruler_text();
        let chrome = Chrome {""", 1)
s = s.replace("""            cmdline_rows: if command.is_some() || showcmd.is_some() || showmode.is_some() {
                cmdheight
            } else {
                0
            },""", """            cmdline_rows: if command.is_some()
                || showcmd.is_some()
                || showmode.is_some()
                || ruler.is_some()
            {
                cmdheight
            } else {
                0
            },""", 1)
s = s.replace("""            showcmd,
            showmode,""", """            showcmd,
            showmode,
            ruler,""", 1)
p.write_text(s)
print("render patched")
