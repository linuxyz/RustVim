import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
start = s.index("    pub(crate) fn delete_buffer(&mut self, id: BufferId, force: bool) -> Result<(), VimError> {")
end = s.index("    pub(crate) fn add_buffer(&mut self, path: Option<&Path>) -> Result<BufferId, VimError> {", start)
new = """    pub(crate) fn unload_buffer(&mut self, id: BufferId, force: bool) -> Result<(), VimError> {
        let Some(buffer) = self.buffers.get(id) else {
            return Err(VimError::new(94, "No matching buffer"));
        };
        if buffer.modified() && !force {
            return Err(VimError::new(
                89,
                "No write since last change for buffer (add ! to override)",
            ));
        }
        let previous = self.current_buffer;
        self.current_buffer = id;
        self.fire_event(Event::BufDelete);
        self.fire_event(Event::BufUnload);
        self.current_buffer = previous;
        self.swaps.remove(id);
        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.listed = false;
            buffer.loaded = false;
        }
        self.detach_buffer_from_windows(id);
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn wipe_buffer(&mut self, id: BufferId, force: bool) -> Result<(), VimError> {
        let Some(buffer) = self.buffers.get(id) else {
            return Err(VimError::new(94, "No matching buffer"));
        };
        if buffer.modified() && !force {
            return Err(VimError::new(
                89,
                "No write since last change for buffer (add ! to override)",
            ));
        }
        if self.buffers.len() <= 1 {
            return Err(VimError::new(85, "You can't delete the last buffer"));
        }
        let previous = self.current_buffer;
        self.current_buffer = id;
        self.fire_event(Event::BufDelete);
        self.fire_event(Event::BufUnload);
        self.fire_event(Event::BufWipeout);
        self.current_buffer = previous;
        self.swaps.remove(id);
        self.detach_buffer_from_windows(id);
        self.buffers.remove(id);
        self.request_redraw();
        Ok(())
    }

    fn detach_buffer_from_windows(&mut self, id: BufferId) {
        let next = self
            .buffer_ids()
            .into_iter()
            .find(|candidate| *candidate != id)
            .unwrap_or(self.current_buffer);
        let affected: Vec<_> = self
            .view
            .windows
            .iter()
            .filter(|(_, window)| window.buffer == id)
            .map(|(window, _)| window)
            .collect();
        for window_id in affected {
            if let Some(window) = self.view.window_mut(window_id) {
                window.buffer = next;
                window.set_cursor(BufPos::start());
            }
        }
        if self.current_buffer == id {
            self.current_buffer = next;
        }
    }

    pub(crate) fn load_buffer(&mut self, id: BufferId) -> Result<(), VimError> {
        let Some(buffer) = self.buffers.get(id) else {
            return Err(VimError::new(94, "No matching buffer"));
        };
        if buffer.loaded {
            return Ok(());
        }
        let Some(path) = buffer.text.meta.name.clone() else {
            return Ok(());
        };
        let bytes = rvim_platform::fs::read_file(&path)
            .map_err(|error| VimError::new(13, error.to_string()))?;
        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.load_bytes(&bytes);
            buffer.loaded = true;
            buffer.listed = true;
            buffer.set_modified(false);
        }
        Ok(())
    }

    pub(crate) fn add_buffer(&mut self, path: Option<&Path>) -> Result<BufferId, VimError> {
        self.create_buffer(path)
    }
"""
s = s[:start] + new + s[end:]
# add unlisted buffer helper
anchor = """    pub(crate) fn add_buffer(&mut self, path: Option<&Path>) -> Result<BufferId, VimError> {
        self.create_buffer(path)
    }
"""
add = anchor + """
    pub(crate) fn add_unlisted_buffer(&mut self, path: Option<&Path>) -> Result<BufferId, VimError> {
        let id = self.create_buffer(None)?;
        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.listed = false;
            buffer.loaded = false;
            if let Some(path) = path {
                let path = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    pfs::cwd().join(path)
                };
                buffer.set_name(path);
            }
        }
        Ok(id)
    }
"""
assert anchor in s
s = s.replace(anchor, add, 1)
p.write_text(s)
print("editor patched")
