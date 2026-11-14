import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "glob" => {"""
arms = """            "getenv" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                match std::env::var(&name) {
                    Ok(value) => Ok(Some(Value::string(value))),
                    Err(_) => Ok(Some(Value::Special(
                        rvim_viml::value::SpecialValue::Null,
                    ))),
                }
            }
            "setenv" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                if let Some(value) = args.get(1) {
                    match value {
                        Value::Special(rvim_viml::value::SpecialValue::Null)
                        | Value::Special(rvim_viml::value::SpecialValue::None) => {
                            std::env::remove_var(&name);
                        }
                        other => std::env::set_var(&name, other.echo_string()),
                    }
                }
                Ok(Some(Value::Number(0)))
            }
            "environ" => {
                let mut dict = VimDict::new();
                for (name, value) in std::env::vars() {
                    dict.set(VimString::from(name), Value::string(value));
                }
                Ok(Some(Value::dict(dict)))
            }
            "glob" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
