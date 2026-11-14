import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub quickfix: QuickfixList,""", """    pub quickfix: QuickfixList,
    pub quickfix_stack: Vec<QuickfixList>,
    pub quickfix_stack_index: usize,""", 1)
s = s.replace("""            quickfix: QuickfixList::default(),""", """            quickfix: QuickfixList::default(),
            quickfix_stack: Vec::new(),
            quickfix_stack_index: 0,""", 1)
old = """    pub(crate) fn set_quickfix(&mut self, title: String, entries: Vec<QfEntry>) {
        let count = entries.len();
        self.quickfix.set(title, entries);
        self.messages.info(format!("{count} quickfix entries"));
    }"""
new = """    pub(crate) fn set_quickfix(&mut self, title: String, entries: Vec<QfEntry>) {
        let count = entries.len();
        self.quickfix.set(title, entries);
        if self.quickfix_stack_index + 1 < self.quickfix_stack.len() {
            self.quickfix_stack.truncate(self.quickfix_stack_index + 1);
        }
        self.quickfix_stack.push(self.quickfix.clone());
        if self.quickfix_stack.len() > 10 {
            self.quickfix_stack.remove(0);
        }
        self.quickfix_stack_index = self.quickfix_stack.len().saturating_sub(1);
        self.messages.info(format!("{count} quickfix entries"));
    }

    pub(crate) fn quickfix_older(&mut self) -> Result<(), VimError> {
        if self.quickfix_stack_index == 0 {
            return Err(VimError::new(776, "No older quickfix list"));
        }
        self.quickfix_stack_index -= 1;
        self.quickfix = self
            .quickfix_stack
            .get(self.quickfix_stack_index)
            .cloned()
            .unwrap_or_default();
        self.messages
            .info(format!("quickfix list {}", self.quickfix_stack_index + 1));
        Ok(())
    }

    pub(crate) fn quickfix_newer(&mut self) -> Result<(), VimError> {
        if self.quickfix_stack_index + 1 >= self.quickfix_stack.len() {
            return Err(VimError::new(777, "No newer quickfix list"));
        }
        self.quickfix_stack_index += 1;
        self.quickfix = self
            .quickfix_stack
            .get(self.quickfix_stack_index)
            .cloned()
            .unwrap_or_default();
        self.messages
            .info(format!("quickfix list {}", self.quickfix_stack_index + 1));
        Ok(())
    }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "cnext" | "cn" | "cnf" | "lnext" | "lne" => editor.quickfix_jump(None)?,'''
arm = '''        "cnext" | "cn" | "cnf" | "lnext" | "lne" => editor.quickfix_jump(None)?,
        "colder" | "col" | "lolder" | "lo" => editor.quickfix_older()?,
        "cnewer" | "cnew" | "lnewer" | "lne" => editor.quickfix_newer()?,'''
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("ex patched")
p = pathlib.Path("crates/rvim-editor/src/registry_generated.rs")
s = p.read_text()
for name in ['colder','cnewer']:
    idx=s.find(f'name: "{name}"')
    end=s.find('implemented: false',idx)
    if idx>=0 and end>=0:
        s=s[:end]+'implemented: true'+s[end+len('implemented: false'):]
p.write_text(s)
print("registry patched")
