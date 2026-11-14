import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "virtcol" => {"""
arms = """            "screenchar" => {
                let row = args
                    .first()
                    .map(|value| value.echo_string().parse::<u16>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let col = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u16>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let frame = self.editor.render();
                let text = frame
                    .base()
                    .cell(row - 1, col - 1)
                    .map(|cell| cell.text.clone())
                    .unwrap_or_default();
                match text.chars().next() {
                    Some(ch) if ch.is_ascii() => Ok(Some(Value::Number(ch as i64))),
                    Some(ch) => Ok(Some(Value::string(ch.to_string()))),
                    None => Ok(Some(Value::Number(0))),
                }
            }
            "screenstring" => {
                let row = args
                    .first()
                    .map(|value| value.echo_string().parse::<u16>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let col = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u16>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let length = args
                    .get(2)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let frame = self.editor.render();
                let text = frame.base().row_text(row - 1);
                let result: String = text
                    .chars()
                    .skip(col.saturating_sub(1) as usize)
                    .take(length)
                    .collect();
                Ok(Some(Value::string(result)))
            }
            "virtcol" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
