import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """    fn set_register(&mut self, name: char, value: Value) -> VimResult<()> {"""
impl = """    fn get_register_list(&mut self, name: char, as_list: bool) -> VimResult<Value> {
        if !as_list {
            return self.get_register(name);
        }
        let Some(register) = rvim_edit::RegisterName::parse(name) else {
            return Err(VimError::new(354, "Invalid register name"));
        };
        let lines = self
            .editor
            .registers
            .get(register)
            .map(|content| {
                content
                    .lines
                    .iter()
                    .map(|line| {
                        Value::string(String::from_utf8_lossy(line.as_bytes()).into_owned())
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Value::list(lines))
    }

    fn set_register_ex(&mut self, name: char, value: Value, options: &str) -> VimResult<()> {
        let Some(register) = rvim_edit::RegisterName::parse(name) else {
            return Err(VimError::new(354, "Invalid register name"));
        };
        let lines: Vec<VimString> = match value {
            Value::List(list) => list
                .borrow()
                .items
                .iter()
                .map(|item| VimString::from(item.echo_string()))
                .collect(),
            Value::String(text) => text
                .as_bytes()
                .split(|byte| *byte == b'\\n')
                .map(VimString::from)
                .collect(),
            other => vec![VimString::from(other.echo_string())],
        };
        let kind = match options.chars().next() {
            Some('V') => rvim_edit::RegisterKind::Linewise,
            Some('b') => rvim_edit::RegisterKind::Blockwise,
            _ => rvim_edit::RegisterKind::Characterwise,
        };
        self.editor
            .registers
            .set(register, rvim_edit::RegisterContent::new(lines, kind));
        Ok(())
    }

    fn set_register(&mut self, name: char, value: Value) -> VimResult<()> {"""
assert anchor in s
s = s.replace(anchor, impl, 1)
p.write_text(s)
print("patched")
