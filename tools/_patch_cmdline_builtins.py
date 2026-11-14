import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = '''            "tabpagenr" => {'''
arms = '''            "getcmdline" => Ok(Some(Value::string(
                self.editor.cmdline.text.as_str(),
            ))),
            "getcmdtype" => {
                let kind = match self.editor.mode {
                    crate::Mode::Command(crate::CommandKind::Ex) => ":",
                    crate::Mode::Command(crate::CommandKind::Search) => "/",
                    _ => "",
                };
                Ok(Some(Value::string(kind)))
            }
            "getcmdpos" => Ok(Some(Value::Number(
                self.editor.cmdline.cursor as i64 + 1,
            ))),
            "setcmdpos" => {
                let position = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                self.editor.cmdline.cursor = position
                    .saturating_sub(1)
                    .min(self.editor.cmdline.text.len());
                Ok(Some(Value::Number(0)))
            }
            "setcmdline" => {
                let text = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let position = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                self.editor.cmdline.text = text;
                self.editor.cmdline.cursor = if position > 0 {
                    position
                        .saturating_sub(1)
                        .min(self.editor.cmdline.text.len())
                } else {
                    self.editor.cmdline.text.len()
                };
                Ok(Some(Value::Number(0)))
            }
            "tabpagenr" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
