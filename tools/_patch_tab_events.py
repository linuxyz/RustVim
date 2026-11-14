import pathlib
p = pathlib.Path("crates/rvim-editor/src/autocmd.rs")
s = p.read_text()
s = s.replace("""    TabEnter,
    TabLeave,""", """    TabNew,
    TabClosed,
    TabEnter,
    TabLeave,""", 1)
s = s.replace("""            "tabenter" => Self::TabEnter,
            "tableave" => Self::TabLeave,""", """            "tabnew" => Self::TabNew,
            "tabclosed" => Self::TabClosed,
            "tabenter" => Self::TabEnter,
            "tableave" => Self::TabLeave,""", 1)
s = s.replace("""            Self::TabEnter => "TabEnter",
            Self::TabLeave => "TabLeave",""", """            Self::TabNew => "TabNew",
            Self::TabClosed => "TabClosed",
            Self::TabEnter => "TabEnter",
            Self::TabLeave => "TabLeave",""", 1)
p.write_text(s)
print("autocmd patched")
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""        let buffer = self.create_buffer(path)?;
        self.view.new_tab(buffer);
        self.current_buffer = buffer;
        self.sync_window_options();
        self.update_window_buffer();
        self.request_redraw();
        Ok(())""", """        let buffer = self.create_buffer(path)?;
        self.view.new_tab(buffer);
        self.current_buffer = buffer;
        self.sync_window_options();
        self.update_window_buffer();
        self.fire_event(Event::TabNew);
        self.request_redraw();
        Ok(())""", 1)
s = s.replace("""    pub(crate) fn tab_next(&mut self) {
        self.view.next_tab();
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.request_redraw();
    }

    pub(crate) fn tab_previous(&mut self) {
        self.view.previous_tab();
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.request_redraw();
    }""", """    pub(crate) fn tab_next(&mut self) {
        self.fire_event(Event::TabLeave);
        self.view.next_tab();
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.fire_event(Event::TabEnter);
        self.request_redraw();
    }

    pub(crate) fn tab_previous(&mut self) {
        self.fire_event(Event::TabLeave);
        self.view.previous_tab();
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.fire_event(Event::TabEnter);
        self.request_redraw();
    }""", 1)
s = s.replace("""        if !self.view.close_current_tab() {
            return Err(VimError::new(444, "Cannot close last tab page"));
        }
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.request_redraw();
        Ok(())""", """        self.fire_event(Event::TabLeave);
        if !self.view.close_current_tab() {
            return Err(VimError::new(444, "Cannot close last tab page"));
        }
        self.fire_event(Event::TabClosed);
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.fire_event(Event::TabEnter);
        self.request_redraw();
        Ok(())""", 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/registry_generated.rs")
s = p.read_text()
for name in ['TabNew','TabClosed']:
    idx = s.find(f'name: "{name}"')
    end = s.find('implemented: false', idx)
    if idx >= 0 and end >= 0:
        s = s[:end] + 'implemented: true' + s[end+len('implemented: false'):]
p.write_text(s)
print("registry patched")
