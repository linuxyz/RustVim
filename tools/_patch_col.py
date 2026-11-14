import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = '''            "indent" | "cindent" => {'''
add = '''            "col" => {
                let spec = string_arg(0);
                let pos = self.editor.cursor();
                let value = match spec.as_str() {
                    "$" => self
                        .editor
                        .current_buffer()
                        .map_or(1, |buffer| buffer.text.line_len(pos.lnum) + 1),
                    _ => pos.col + 1,
                };
                Ok(Some(Value::Number(value as i64)))
            }
            "line" => {
                let spec = string_arg(0);
                let value = match spec.as_str() {
                    "$" => self.editor.line_count().max(1),
                    "." => self.editor.cursor().lnum,
                    _ => self.editor.cursor().lnum,
                };
                Ok(Some(Value::Number(value as i64)))
            }
            "indent" | "cindent" => {'''
assert anchor in s
s = s.replace(anchor, add, 1)
p.write_text(s)
print("patched")
