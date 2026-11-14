import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getline" => {"""
arms = """            "getreg" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "\"".to_string());
                let as_list = args
                    .get(1)
                    .map(|value| value.echo_string() != "0")
                    .unwrap_or(false);
                let register = name
                    .chars()
                    .next()
                    .and_then(rvim_edit::RegisterName::parse)
                    .unwrap_or(rvim_edit::RegisterName::Unnamed);
                let content = self.editor.registers.get(register);
                if as_list {
                    let lines = content
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
                    return Ok(Some(Value::list(lines)));
                }
                let text = content
                    .map(|content| content.text())
                    .map(|text| String::from_utf8_lossy(text.as_bytes()).into_owned())
                    .unwrap_or_default();
                Ok(Some(Value::string(text)))
            }
            "getregtype" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "\"".to_string());
                let register = name
                    .chars()
                    .next()
                    .and_then(rvim_edit::RegisterName::parse)
                    .unwrap_or(rvim_edit::RegisterName::Unnamed);
                let kind = self
                    .editor
                    .registers
                    .get(register)
                    .map(|content| match content.kind {
                        rvim_edit::RegisterKind::Characterwise => "v",
                        rvim_edit::RegisterKind::Linewise => "V",
                        rvim_edit::RegisterKind::Blockwise => "b",
                    })
                    .unwrap_or("");
                Ok(Some(Value::string(kind)))
            }
            "setreg" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "\"".to_string());
                let register = name
                    .chars()
                    .next()
                    .and_then(rvim_edit::RegisterName::parse)
                    .unwrap_or(rvim_edit::RegisterName::Unnamed);
                let lines: Vec<VimString> = match args.get(1) {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| VimString::from(item.echo_string()))
                        .collect(),
                    Some(other) => other
                        .echo_string()
                        .split('\\n')
                        .map(VimString::from)
                        .collect(),
                    None => Vec::new(),
                };
                let options = args
                    .get(2)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let kind = match options.chars().next() {
                    Some('V') => rvim_edit::RegisterKind::Linewise,
                    Some('b') => rvim_edit::RegisterKind::Blockwise,
                    _ => rvim_edit::RegisterKind::Characterwise,
                };
                self.editor.registers.set(
                    register,
                    rvim_edit::RegisterContent::new(lines, kind),
                );
                Ok(Some(Value::Number(0)))
            }
            "getline" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
