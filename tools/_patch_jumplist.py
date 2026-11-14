import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getwinvar" => {"""
arms = """            "getjumplist" => {
                let mut entries = Vec::new();
                for (buffer, pos) in &self.editor.jumplist {
                    let mut dict = VimDict::new();
                    dict.set(
                        VimString::from("lnum"),
                        Value::Number(pos.lnum as i64),
                    );
                    dict.set(
                        VimString::from("col"),
                        Value::Number(pos.col as i64 + 1),
                    );
                    let filename = self
                        .editor
                        .buffers
                        .get(*buffer)
                        .map(|buffer| buffer.name_string())
                        .unwrap_or_default();
                    dict.set(VimString::from("filename"), Value::string(filename));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(vec![
                    Value::list(entries),
                    Value::Number(self.editor.jump_index as i64),
                ])))
            }
            "getchangelist" => {
                let mut entries = Vec::new();
                for (buffer, pos) in &self.editor.changelist {
                    let mut dict = VimDict::new();
                    dict.set(
                        VimString::from("lnum"),
                        Value::Number(pos.lnum as i64),
                    );
                    dict.set(
                        VimString::from("col"),
                        Value::Number(pos.col as i64 + 1),
                    );
                    let filename = self
                        .editor
                        .buffers
                        .get(*buffer)
                        .map(|buffer| buffer.name_string())
                        .unwrap_or_default();
                    dict.set(VimString::from("filename"), Value::string(filename));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(vec![
                    Value::list(entries),
                    Value::Number(self.editor.change_index as i64),
                ])))
            }
            "getwinvar" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
