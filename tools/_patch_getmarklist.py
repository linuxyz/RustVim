import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getpos" => {"""
arm = """            "getmarklist" => {
                let bufnr = self.editor.buffer_number_of(self.editor.current_buffer_id());
                let mut entries = Vec::new();
                if let Some(buffer) = self.editor.current_buffer() {
                    let file = buffer.name_string();
                    for (mark, pos) in &buffer.marks {
                        let mut dict = VimDict::new();
                        dict.set(VimString::from("mark"), Value::string(mark.to_string()));
                        dict.set(
                            VimString::from("pos"),
                            Value::list(vec![
                                Value::Number(bufnr as i64),
                                Value::Number(pos.lnum as i64),
                                Value::Number(pos.col as i64 + 1),
                                Value::Number(0),
                            ]),
                        );
                        dict.set(VimString::from("file"), Value::string(file.as_str()));
                        entries.push(Value::dict(dict));
                    }
                }
                Ok(Some(Value::list(entries)))
            }
            "getpos" => {"""
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("patched")
