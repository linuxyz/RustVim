import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    pub(crate) fn tab_close(&mut self, force: bool) -> Result<(), VimError> {"""
method = """    pub(crate) fn tab_only(&mut self, force: bool) -> Result<(), VimError> {
        if !force {
            let current = self.view.current_tab;
            let modified = self
                .view
                .tabs
                .iter()
                .filter(|(id, _)| *id != current)
                .any(|(_, page)| {
                    rvim_view::layout::all_windows(&page.root).into_iter().any(|window| {
                        self.view
                            .windows
                            .get(&window)
                            .and_then(|window| self.buffers.get(window.buffer))
                            .is_some_and(|buffer| buffer.modified())
                    })
                });
            if modified {
                return Err(VimError::new(445, "Other window contains changes"));
            }
        }
        if !self.view.only_current_tab() {
            return Err(VimError::new(444, "Cannot close last tab page"));
        }
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn tab_close(&mut self, force: bool) -> Result<(), VimError> {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
s = s.replace('''        "tabonly" | "tabo" => {\n            editor.view.only_current_tab();\n            editor.sync_current_buffer_to_window();\n            editor.request_redraw();\n        }''','''        "tabonly" | "tabo" => editor.tab_only(command.bang)?,''',1)
p.write_text(s)
print("ex patched")
