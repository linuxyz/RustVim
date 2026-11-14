import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
old = """        let id = self.create_buffer(Some(&entry.file))?;
        self.switch_to_buffer(id)?;
        self.set_cursor(BufPos::new(entry.lnum.max(1), entry.col.saturating_sub(1)));"""
new = """        let path = if entry.file.is_absolute() {
            entry.file.clone()
        } else {
            pfs::cwd().join(&entry.file)
        };
        let existing = self
            .buffer_ids()
            .into_iter()
            .find(|id| {
                self.buffers
                    .get(*id)
                    .is_some_and(|buffer| buffer.text.meta.name.as_ref() == Some(&path))
            });
        let switchbuf = self.options.get_string("switchbuf");
        if switchbuf.split(',').any(|item| item == "newtab") {
            self.tab_new(Some(&path))?;
        } else {
            let id = match existing {
                Some(id) => id,
                None => self.create_buffer(Some(&path))?,
            };
            if switchbuf
                .split(',')
                .any(|item| item == "vsplit" || item == "split")
            {
                let direction = if switchbuf.split(',').any(|item| item == "vsplit") {
                    rvim_view::SplitDir::Vertical
                } else {
                    rvim_view::SplitDir::Horizontal
                };
                self.split_window(direction, None)?;
            } else if switchbuf.split(',').any(|item| item == "useopen") {
                if let Some(window) = self
                    .view
                    .windows
                    .iter()
                    .find(|(_, window)| window.buffer == id)
                    .map(|(_, window)| window.id)
                {
                    self.view.set_current_window(window);
                    self.sync_current_buffer_to_window();
                }
            }
            self.switch_to_buffer(id)?;
        }
        self.set_cursor(BufPos::new(entry.lnum.max(1), entry.col.saturating_sub(1)));"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("patched")
