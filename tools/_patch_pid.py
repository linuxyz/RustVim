import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getenv" => {"""
arms = """            "getpid" => Ok(Some(Value::Number(std::process::id() as i64))),
            "hostname" => {
                let name = std::env::var("HOSTNAME")
                    .or_else(|_| std::env::var("COMPUTERNAME"))
                    .or_else(|_| std::fs::read_to_string("/etc/hostname").map(|text| text.trim().to_string()))
                    .unwrap_or_default();
                Ok(Some(Value::string(name)))
            }
            "getenv" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
