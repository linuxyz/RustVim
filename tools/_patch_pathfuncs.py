import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "tempname" => {"""
arms = """            "resolve" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let resolved = std::fs::canonicalize(&path)
                    .map(|path| path.display().to_string())
                    .unwrap_or(path);
                Ok(Some(Value::string(resolved)))
            }
            "simplify" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let mut parts = Vec::new();
                for part in path.split('/') {
                    match part {
                        "" | "." => {}
                        ".." => {
                            parts.pop();
                        }
                        part => parts.push(part),
                    }
                }
                let prefix = if path.starts_with('/') { "/" } else { "" };
                Ok(Some(Value::string(format!(
                    "{prefix}{}",
                    parts.join("/")
                ))))
            }
            "fnamemodify" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let modifiers = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let mut path = std::path::PathBuf::from(&name);
                let mut chars = modifiers.chars().peekable();
                while let Some(modifier) = chars.next() {
                    if modifier != ':' {
                        continue;
                    }
                    let Some(kind) = chars.next() else {
                        break;
                    };
                    match kind {
                        'p' => {
                            if path.is_relative() {
                                path = std::env::current_dir()
                                    .unwrap_or_default()
                                    .join(&path);
                            }
                        }
                        'h' => {
                            path = path
                                .parent()
                                .map(std::path::Path::to_path_buf)
                                .unwrap_or_default();
                        }
                        't' => {
                            path = path
                                .file_name()
                                .map(std::path::PathBuf::from)
                                .unwrap_or_default();
                        }
                        'r' => {
                            path = path.with_extension("");
                        }
                        'e' => {
                            let extension = path
                                .extension()
                                .map(|extension| extension.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            path = std::path::PathBuf::from(extension);
                        }
                        _ => {}
                    }
                }
                Ok(Some(Value::string(path.display().to_string())))
            }
            "tempname" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
