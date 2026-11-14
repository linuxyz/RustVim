import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "isdirectory" =>"""
arms = """            "filereadable" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                Ok(Some(Value::Number(i64::from(
                    std::fs::File::open(&path).is_ok(),
                ))))
            }
            "filewritable" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let writable = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .is_ok();
                Ok(Some(Value::Number(i64::from(writable))))
            }
            "getfsize" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let size = std::fs::metadata(&path)
                    .map(|metadata| metadata.len() as i64)
                    .unwrap_or(-1);
                Ok(Some(Value::Number(size)))
            }
            "getftype" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let kind = std::fs::symlink_metadata(&path)
                    .map(|metadata| {
                        let file_type = metadata.file_type();
                        if file_type.is_symlink() {
                            "link"
                        } else if file_type.is_dir() {
                            "dir"
                        } else if file_type.is_file() {
                            "file"
                        } else {
                            ""
                        }
                    })
                    .unwrap_or("");
                Ok(Some(Value::string(kind)))
            }
            "readdir" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let mut names: Vec<String> = std::fs::read_dir(&path)
                    .map(|entries| {
                        entries
                            .filter_map(|entry| entry.ok())
                            .map(|entry| entry.file_name().to_string_lossy().into_owned())
                            .filter(|name| name != "." && name != "..")
                            .collect()
                    })
                    .unwrap_or_default();
                names.sort();
                Ok(Some(Value::list(
                    names.into_iter().map(Value::string).collect(),
                )))
            }
            "isdirectory" =>"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
