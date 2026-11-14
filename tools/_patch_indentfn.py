import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = '''            "filereadable" => Ok(Some(Value::Number(i64::from('''
add = '''            "indent" | "cindent" => {
                let lnum = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let line = self
                    .editor
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(lnum))
                    .unwrap_or(b"");
                let tabstop = self.editor.options.get_number("tabstop").max(1) as usize;
                let mut width = 0usize;
                for byte in line {
                    match byte {
                        b' ' => width += 1,
                        b'\\t' => width += tabstop - (width % tabstop),
                        _ => break,
                    }
                }
                Ok(Some(Value::Number(width as i64)))
            }
            "shiftwidth" => Ok(Some(Value::Number(
                self.editor
                    .options
                    .buffer_number(self.editor.current_buffer_id(), "shiftwidth"),
            ))),
            "filereadable" => Ok(Some(Value::Number(i64::from('''
assert anchor in s
s = s.replace(anchor, add, 1)
p.write_text(s)
print("patched")
