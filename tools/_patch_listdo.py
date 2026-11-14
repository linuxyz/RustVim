import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    pub(crate) fn buffer_first(&mut self) -> Result<(), VimError> {"""
method = """    pub(crate) fn execute_list_command(
        &mut self,
        kind: crate::ListDoCommand,
        command: &str,
    ) -> Result<(), VimError> {
        if command.is_empty() {
            return Err(VimError::new(471, "Argument required"));
        }
        match kind {
            crate::ListDoCommand::Buffers => {
                let original = self.current_buffer;
                let ids = self.listed_buffer_ids();
                for id in ids {
                    if !self.buffers.contains(id) {
                        continue;
                    }
                    if self.switch_to_buffer(id).is_err() {
                        continue;
                    }
                    self.execute_ex(command);
                }
                if self.buffers.contains(original) {
                    self.current_buffer = original;
                    self.update_window_buffer();
                    self.sync_window_options();
                }
            }
            crate::ListDoCommand::Windows => {
                let original = self.current_window_id();
                let ids = self.view.window_ids_in_current_tab();
                for id in ids {
                    if !self.view.windows.contains_key(&id) {
                        continue;
                    }
                    self.view.set_current_window(id);
                    self.sync_current_buffer_to_window();
                    self.sync_window_options();
                    self.execute_ex(command);
                }
                if self.view.windows.contains_key(&original) {
                    self.view.set_current_window(original);
                    self.sync_current_buffer_to_window();
                    self.sync_window_options();
                }
            }
            crate::ListDoCommand::Tabs => {
                let original = self.view.current_tab;
                let ids = self.view.tab_ids();
                for id in ids {
                    if !self.view.tabs.contains_key(&id) {
                        continue;
                    }
                    self.view.set_tab(id);
                    self.sync_current_buffer_to_window();
                    self.sync_window_options();
                    self.execute_ex(command);
                }
                if self.view.tabs.contains_key(&original) {
                    self.view.set_tab(original);
                    self.sync_current_buffer_to_window();
                    self.sync_window_options();
                }
            }
            crate::ListDoCommand::Arguments => {
                let original = self.current_buffer;
                let args = self.arglist.clone();
                for path in args {
                    self.open_file(&path)?;
                    self.execute_ex(command);
                }
                if self.buffers.contains(original) {
                    self.current_buffer = original;
                    self.update_window_buffer();
                    self.sync_window_options();
                }
            }
        }
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn buffer_first(&mut self) -> Result<(), VimError> {"""
assert anchor in s
s = s.replace(anchor, method, 1)
# add enum near CommandKind
s = s.replace("""pub enum CommandKind {
    Ex,
    Search,
}""", """pub enum CommandKind {
    Ex,
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListDoCommand {
    Buffers,
    Windows,
    Tabs,
    Arguments,
}""", 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor2 = """        "bfirst" | "brewind" | "br" => editor.buffer_first()?,"""
arms = """        "bufdo" => editor.execute_list_command(crate::ListDoCommand::Buffers, command.args.trim())?,
        "windo" => editor.execute_list_command(crate::ListDoCommand::Windows, command.args.trim())?,
        "tabdo" => editor.execute_list_command(crate::ListDoCommand::Tabs, command.args.trim())?,
        "argdo" => editor.execute_list_command(crate::ListDoCommand::Arguments, command.args.trim())?,
        "bfirst" | "brewind" | "br" => editor.buffer_first()?,"""
assert anchor2 in s
s = s.replace(anchor2, arms, 1)
p.write_text(s)
print("ex patched")
p = pathlib.Path("crates/rvim-editor/src/registry_generated.rs")
s = p.read_text()
for name in ['argdo','bufdo','tabdo','windo']:
    idx = s.find(f'name: "{name}"')
    end = s.find('implemented: false', idx)
    if idx >= 0 and end >= 0:
        s = s[:end] + 'implemented: true' + s[end+len('implemented: false'):]
p.write_text(s)
print("registry patched")
