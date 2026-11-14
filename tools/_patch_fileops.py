import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getenv" => {"""
arms = """            "tempname" => {
                use std::sync::atomic::{AtomicU64, Ordering};
                static COUNTER: AtomicU64 = AtomicU64::new(0);
                let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
                let unique = format!(
                    "rvim-{}-{}-{}",
                    std::process::id(),
                    counter,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.subsec_nanos())
                        .unwrap_or(0)
                );
                let path = std::env::temp_dir().join(unique);
                Ok(Some(Value::string(path.display().to_string())))
            }
            "mkdir" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let result = std::fs::create_dir(&path)
                    .map(|()| 0)
                    .unwrap_or(-1);
                Ok(Some(Value::Number(result)))
            }
            "delete" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let flags = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let result = if flags.contains('d') {
                    std::fs::remove_dir_all(&path)
                } else if flags.contains('D') {
                    std::fs::remove_dir(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                Ok(Some(Value::Number(result.map(|()| 0).unwrap_or(-1))))
            }
            "rename" => {
                let from = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let to = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let result = std::fs::rename(&from, &to)
                    .map(|()| 0)
                    .unwrap_or(-1);
                Ok(Some(Value::Number(result)))
            }
            "getenv" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
