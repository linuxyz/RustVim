import pathlib
p = pathlib.Path("crates/rvim-edit/src/register.rs")
s = p.read_text()
anchor = """    pub fn set(&mut self, name: RegisterName, mut content: RegisterContent) {"""
method = """    /// Registers that Vim persists in a viminfo file, newest data last.
    pub fn savable(&self) -> Vec<(RegisterName, &RegisterContent)> {
        let mut out = Vec::new();
        let mut push = |name: RegisterName| {
            if let Some(content) = self.get(name) {
                out.push((name, content));
            }
        };
        push(RegisterName::Unnamed);
        for number in 0..=9 {
            push(RegisterName::Numbered(number));
        }
        for letter in 'a'..='z' {
            push(RegisterName::Small(letter));
        }
        push(RegisterName::LastInsert);
        push(RegisterName::LastCommand);
        push(RegisterName::LastSearch);
        out
    }

    pub fn set(&mut self, name: RegisterName, mut content: RegisterContent) {"""
assert anchor in s
s = s.replace(anchor, method, 1)
p.write_text(s)
print("registers patched")
# add oldfiles field
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub command_history: VecDeque<String>,
    pub search_history: VecDeque<String>,""", """    pub command_history: VecDeque<String>,
    pub search_history: VecDeque<String>,
    pub oldfiles: Vec<String>,""", 1)
s = s.replace("""            command_history: VecDeque::new(),
            search_history: VecDeque::new(),""", """            command_history: VecDeque::new(),
            search_history: VecDeque::new(),
            oldfiles: Vec::new(),""", 1)
p.write_text(s)
print("editor patched")
