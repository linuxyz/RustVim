import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub echo_window: bool,
    pub silent: bool,
}""", """    pub echo_window: bool,
    pub silent: bool,
    pub capture: Option<String>,
}""", 1)
s = s.replace("""    pub fn info(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.last = Some(message.clone());""", """    pub fn info(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(capture) = &mut self.capture {
            capture.push_str(&message);
            capture.push('\n');
        }
        self.last = Some(message.clone());""", 1)
s = s.replace("""    pub fn warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        if self.silent {""", """    pub fn warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(capture) = &mut self.capture {
            capture.push_str(&message);
            capture.push('\n');
        }
        if self.silent {""", 1)
s = s.replace("""    pub fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        if self.silent {""", """    pub fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(capture) = &mut self.capture {
            capture.push_str(&message);
            capture.push('\n');
        }
        if self.silent {""", 1)
s = s.replace("""    pub fn echo(&mut self, message: impl Into<String>, append: bool) {
        if self.silent {
            return;
        }
        let message = message.into();""", """    pub fn echo(&mut self, message: impl Into<String>, append: bool) {
        let message = message.into();
        if let Some(capture) = &mut self.capture {
            capture.push_str(&message);
            if !append {
                capture.push('\n');
            }
        }
        if self.silent {
            return;
        }""", 1)
p.write_text(s)
print("messages patched")
