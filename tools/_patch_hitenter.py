import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
old = """    pub fn handle_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Key(key) => self.handle_key(key),"""
new = """    pub fn handle_event(&mut self, event: UiEvent) {
        if self.hit_enter {
            if let UiEvent::Key(_) = event {
                self.hit_enter = false;
                self.request_redraw();
                return;
            }
        }
        match event {
            UiEvent::Key(key) => self.handle_key(key),"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("patched")
