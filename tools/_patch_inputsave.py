import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub fed_keys: std::collections::VecDeque<rvim_viml::Value>,""", """    pub fed_keys: std::collections::VecDeque<rvim_viml::Value>,
    pub input_stack: Vec<std::collections::VecDeque<rvim_viml::Value>>,""", 1)
s = s.replace("""            fed_keys: std::collections::VecDeque::new(),""", """            fed_keys: std::collections::VecDeque::new(),
            input_stack: Vec::new(),""", 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "inputsecret" => {"""
arms = """            "inputsave" => {
                let saved = std::mem::take(&mut self.editor.fed_keys);
                self.editor.input_stack.push(saved);
                Ok(Some(Value::Number(0)))
            }
            "inputrestore" => {
                let restored = self.editor.input_stack.pop();
                match restored {
                    Some(saved) => {
                        self.editor.fed_keys = saved;
                        Ok(Some(Value::Number(1)))
                    }
                    None => Ok(Some(Value::Number(0))),
                }
            }
            "inputsecret" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("host patched")
