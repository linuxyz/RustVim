import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    pub fn read_default_viminfo(&mut self) {"""
# Check absent; add methods before set_message
anchor2 = """    pub fn set_message(&mut self, message: impl Into<String>) {"""
methods = """    pub fn read_default_viminfo(&mut self) {
        if self.options.get_string("viminfo").is_empty() {
            return;
        }
        if let Some(path) = crate::viminfo::default_path(self) {
            let _ = crate::viminfo::read(self, &path);
        }
    }

    pub fn write_default_viminfo(&self) {
        if self.options.get_string("viminfo").is_empty() {
            return;
        }
        if let Some(path) = crate::viminfo::default_path(self) {
            let _ = crate::viminfo::write(self, &path);
        }
    }

    pub fn set_message(&mut self, message: impl Into<String>) {"""
assert anchor2 in s
s = s.replace(anchor2, methods, 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-cli/src/main.rs")
s = p.read_text()
s = s.replace("""    let mut editor = make_editor(&args, size);
    open_files(&mut editor, &args)?;
    if args.recover {""", """    let mut editor = make_editor(&args, size);
    open_files(&mut editor, &args)?;
    editor.read_default_viminfo();
    if args.recover {""", 1)
s = s.replace("""    editor.fire_event(rvim_editor::autocmd::Event::VimLeavePre);
    backend.shutdown().map_err(|e| e.to_string())?;
    editor.shutdown_swaps();""", """    editor.fire_event(rvim_editor::autocmd::Event::VimLeavePre);
    editor.write_default_viminfo();
    backend.shutdown().map_err(|e| e.to_string())?;
    editor.shutdown_swaps();""", 1)
p.write_text(s)
print("cli patched")
