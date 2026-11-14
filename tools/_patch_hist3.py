import pathlib
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "messages" | "mes" | "messa" => {'''
arm = '''        "history" | "his" => {
            let arg = command.args.trim();
            let kinds: Vec<(&str, &str)> = match arg {
                "" | ":" | "cmd" => vec![("cmd", ":")],
                "/" | "search" => vec![("search", "/")],
                "all" => vec![("cmd", ":"), ("search", "/")],
                _ => return Err(VimError::new(474, "E474: Invalid argument")),
            };
            let mut out = String::new();
            for (kind, _) in kinds {
                if let Some(history) = editor.history(kind) {
                    for (index, item) in history.iter().enumerate() {
                        out.push_str(&format!("{:>3}  {}\\n", index + 1, item));
                    }
                }
            }
            editor.messages.info(out.trim_end().to_string());
        }
        "messages" | "mes" | "messa" => {'''
assert anchor in s
s = s.replace(anchor, arm, 1)
p.write_text(s)
print("patched")
