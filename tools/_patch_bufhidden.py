import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("pub(crate) fn unload_buffer(&mut self, id: BufferId, force: bool) -> Result<(), VimError> {", "pub(crate) fn unload_buffer(\n        &mut self,\n        id: BufferId,\n        force: bool,\n        unlist: bool,\n    ) -> Result<(), VimError> {", 1)
s = s.replace("""        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.listed = false;
            buffer.loaded = false;
        }
        self.detach_buffer_from_windows(id);""", """        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.listed = !unlist;
            buffer.loaded = false;
        }
        self.detach_buffer_from_windows(id);""", 1)
# switch_to_buffer hidden handling
old = """        self.fire_event(Event::BufEnter);
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn next_buffer"""
new = """        let old = self.alternate_buffer.unwrap_or(self.current_buffer);
        if old != id && !self.buffer_visible(old) {
            match self.options.buffer_string(old, "bufhidden").as_str() {
                "unload" => {
                    let _ = self.unload_buffer(old, true, false);
                }
                "delete" => {
                    let _ = self.unload_buffer(old, true, true);
                }
                "wipe" => {
                    let _ = self.wipe_buffer(old, true);
                }
                _ => {}
            }
        }
        self.fire_event(Event::BufEnter);
        self.request_redraw();
        Ok(())
    }

    fn buffer_visible(&self, id: BufferId) -> bool {
        self.view.windows.values().any(|window| window.buffer == id)
    }

    pub(crate) fn next_buffer"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("editor patched")
# ex calls
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
s = s.replace("editor.unload_buffer(id, command.bang)?;", "editor.unload_buffer(id, command.bang, true)?;", 1)
# bdelete arm and bunload arm both replaced first two occurrences? do targeted
s = s.replace("""        "bdelete" | "bd" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.unload_buffer(id, command.bang, true)?;
        }""", """        "bdelete" | "bd" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.unload_buffer(id, command.bang, true)?;
        }""", 1)
s = s.replace("""        "bunload" | "bun" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.unload_buffer(id, command.bang, true)?;
        }""", """        "bunload" | "bun" => {
            let id = buffer_argument(editor, command.args.as_str())?;
            editor.unload_buffer(id, command.bang, false)?;
        }""", 1)
p.write_text(s)
print("ex patched")
