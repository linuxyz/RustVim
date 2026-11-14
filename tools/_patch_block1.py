import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("Visual { linewise: bool, start: BufPos },", "Visual { linewise: bool, block: bool, start: BufPos },", 1)
s = s.replace("pub last_visual: Option<(BufPos, BufPos, bool)>,", "pub last_visual: Option<(BufPos, BufPos, bool, bool)>,", 1)
# constructors v / V
s = s.replace("""                    self.mode = Mode::Visual {
                        linewise: false,
                        start: self.cursor(),
                    }""", """                    self.mode = Mode::Visual {
                        linewise: false,
                        block: false,
                        start: self.cursor(),
                    }""", 1)
s = s.replace("""                    self.mode = Mode::Visual {
                        linewise: true,
                        start: self.cursor(),
                    }""", """                    self.mode = Mode::Visual {
                        linewise: true,
                        block: false,
                        start: self.cursor(),
                    }""", 1)
# V toggle and o
s = s.replace("""                if let Mode::Visual { linewise, start } = self.mode.clone() {
                    self.mode = Mode::Visual {
                        linewise: !linewise,
                        start,
                    };""", """                if let Mode::Visual {
                    linewise, block, start,
                } = self.mode.clone()
                {
                    self.mode = Mode::Visual {
                        linewise: !linewise,
                        block: false,
                        start,
                    };""", 1)
s = s.replace("""                if let Mode::Visual { linewise, start } = self.mode.clone() {
                    let cursor = self.cursor();
                    self.mode = Mode::Visual {
                        linewise,
                        start: cursor,
                    };""", """                if let Mode::Visual {
                    linewise, block, start,
                } = self.mode.clone()
                {
                    let cursor = self.cursor();
                    self.mode = Mode::Visual {
                        linewise,
                        block,
                        start: cursor,
                    };""", 1)
# visual_range
s = s.replace("""        let Mode::Visual { linewise, start } = self.mode else {
            return None;
        };
        let end = self.cursor();
        Some(if linewise {
            rvim_edit::TextRange::linewise(start.lnum, end.lnum)
        } else {
            rvim_edit::TextRange::characterwise(start, end, true)
        })""", """        let Mode::Visual {
            linewise, block, start,
        } = self.mode
        else {
            return None;
        };
        let end = self.cursor();
        Some(if linewise {
            rvim_edit::TextRange::linewise(start.lnum, end.lnum)
        } else if block {
            rvim_edit::TextRange::blockwise(start, end)
        } else {
            rvim_edit::TextRange::characterwise(start, end, true)
        })""", 1)
# save_last_visual
s = s.replace("""        if let Mode::Visual { linewise, start } = self.mode {
            self.last_visual = Some((start, self.cursor(), linewise));
        }""", """        if let Mode::Visual {
            linewise, block, start,
        } = self.mode
        {
            self.last_visual = Some((start, self.cursor(), linewise, block));
        }""", 1)
# set_visual_marks pattern unaffected (uses ..)
# sync_visual_selection
s = s.replace("""        if let Mode::Visual { linewise, start } = self.mode {
            let end = self.cursor();
            if let Some(window) = self.view.current_window_mut() {
                window.visual = Some(rvim_view::VisualSelection {
                    start,
                    end,
                    linewise,
                });
            }
        }""", """        if let Mode::Visual {
            linewise, block, start,
        } = self.mode
        {
            let end = self.cursor();
            if let Some(window) = self.view.current_window_mut() {
                window.visual = Some(rvim_view::VisualSelection {
                    start,
                    end,
                    linewise,
                    block,
                });
            }
        }""", 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-view/src/window.rs")
s = p.read_text()
s = s.replace("""pub struct VisualSelection {
    pub start: BufPos,
    pub end: BufPos,
    pub linewise: bool,
}""", """pub struct VisualSelection {
    pub start: BufPos,
    pub end: BufPos,
    pub linewise: bool,
    pub block: bool,
}""", 1)
p.write_text(s)
print("view patched")
