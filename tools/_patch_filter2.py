import pathlib
# AST
p = pathlib.Path("crates/rvim-ex/src/ast.rs")
s = p.read_text()
s = s.replace("""    pub modifiers: Vec<Modifier>,
    pub raw: String,""", """    pub modifiers: Vec<Modifier>,
    pub filter: Option<String>,
    pub raw: String,""", 1)
s = s.replace("""            modifiers: Vec::new(),
            raw: String::new(),""", """            modifiers: Vec::new(),
            filter: None,
            raw: String::new(),""", 1)
p.write_text(s)
# parser
p = pathlib.Path("crates/rvim-ex/src/parser.rs")
s = p.read_text()
anchor = """        let lower = word.to_ascii_lowercase();
        let modifier = match lower.as_str() {"""
new = """        let lower = word.to_ascii_lowercase();
        if lower == "filter" {
            skip_space(bytes, &mut pos);
            if pos >= bytes.len() {
                return Ok(command);
            }
            let delimiter = bytes[pos] as char;
            if delimiter.is_ascii_alphanumeric() || delimiter == '\\\\' {
                return Err(ExParseError::at(pos, "E475: Invalid argument: filter"));
            }
            pos += 1;
            let mut pattern = String::new();
            while pos < bytes.len() {
                let ch = bytes[pos] as char;
                if ch == delimiter {
                    pos += 1;
                    break;
                }
                pattern.push(ch);
                pos += 1;
            }
            command.filter = Some(pattern);
            if pos >= bytes.len() {
                return Ok(command);
            }
            continue;
        }
        let modifier = match lower.as_str() {"""
assert anchor in s
s = s.replace(anchor, new, 1)
p.write_text(s)
# editor message filter
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub capture: Option<String>,
}""", """    pub capture: Option<String>,
    pub filter: Option<String>,
}""", 1)
# add filter check in info/warning/error/echo; insert after message let
s = s.replace("""    pub fn info(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(capture) = &mut self.capture {""", """    pub fn info(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(filter) = &self.filter {
            if !message.contains(filter) {
                return;
            }
        }
        if let Some(capture) = &mut self.capture {""", 1)
s = s.replace("""    pub fn warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(capture) = &mut self.capture {""", """    pub fn warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(filter) = &self.filter {
            if !message.contains(filter) {
                return;
            }
        }
        if let Some(capture) = &mut self.capture {""", 1)
s = s.replace("""    pub fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(capture) = &mut self.capture {""", """    pub fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(filter) = &self.filter {
            if !message.contains(filter) {
                return;
            }
        }
        if let Some(capture) = &mut self.capture {""", 1)
s = s.replace("""    pub fn echo(&mut self, message: impl Into<String>, append: bool) {
        let message = message.into();
        if let Some(capture) = &mut self.capture {""", """    pub fn echo(&mut self, message: impl Into<String>, append: bool) {
        let message = message.into();
        if let Some(filter) = &self.filter {
            if !message.contains(filter) {
                return;
            }
        }
        if let Some(capture) = &mut self.capture {""", 1)
p.write_text(s)
print("patched")
