import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = '''            "indent" | "cindent" => {'''
add = '''            "histadd" => {
                let kind = string_arg(0);
                let item = string_arg(1);
                let limit = self.editor.options.get_number("history").max(0) as usize;
                if let Some(history) = self.editor.history_mut(&kind) {
                    if history.back().is_some_and(|last| *last == item) {
                        return Ok(Some(Value::Number(0)));
                    }
                    history.push_back(item);
                    while history.len() > limit {
                        history.pop_front();
                    }
                    Ok(Some(Value::Number(1)))
                } else {
                    Ok(Some(Value::Number(0)))
                }
            }
            "histdel" => {
                let kind = string_arg(0);
                let spec = args.get(1).map(|value| value.echo_string());
                let Some(history) = self.editor.history_mut(&kind) else {
                    return Ok(Some(Value::Number(0)));
                };
                match spec {
                    None => {
                        history.clear();
                        Ok(Some(Value::Number(1)))
                    }
                    Some(spec) => {
                        if let Ok(index) = spec.parse::<i64>() {
                            let len = history.len() as i64;
                            let position = if index < 0 { len + index } else { index - 1 };
                            if position >= 0 && position < len {
                                history.remove(position as usize);
                                Ok(Some(Value::Number(1)))
                            } else {
                                Ok(Some(Value::Number(0)))
                            }
                        } else {
                            let before = history.len();
                            history.retain(|item| !item.contains(&spec));
                            Ok(Some(Value::Number(i64::from(history.len() != before))))
                        }
                    }
                }
            }
            "histget" => {
                let kind = string_arg(0);
                let index = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<i64>().unwrap_or(-1))
                    .unwrap_or(-1);
                let Some(history) = self.editor.history(&kind) else {
                    return Ok(Some(Value::string("")));
                };
                let len = history.len() as i64;
                let position = if index < 0 { len + index } else { index - 1 };
                let value = if position >= 0 && position < len {
                    history
                        .get(position as usize)
                        .cloned()
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                Ok(Some(Value::string(value)))
            }
            "histnr" => {
                let kind = string_arg(0);
                let value = self.editor.history(&kind).map_or(-1, |history| {
                    history.len() as i64
                });
                Ok(Some(Value::Number(value)))
            }
            "indent" | "cindent" => {'''
assert anchor in s
s = s.replace(anchor, add, 1)
p.write_text(s)
print("host patched")
