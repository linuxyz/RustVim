import pathlib
p = pathlib.Path("crates/rvim-editor/src/options.rs")
s = p.read_text()
old = """        {
            let bare = token.trim_start_matches("no");
            if bare.starts_with("t_") || token.starts_with("t_") {
                continue;
            }
        }"""
new = """        if token.starts_with("t_") {
            let mut name = token;
            let mut value = None;
            if let Some((left, right)) = token.split_once('=') {
                name = left;
                value = Some(right.to_string());
            }
            let query = name.ends_with('?');
            let reset = name.ends_with('&');
            let name = name.trim_end_matches(['?', '&']).to_string();
            if query {
                let value = store
                    .termcap
                    .get(&name)
                    .cloned()
                    .unwrap_or_default();
                result.messages.push(format!("{name}={value}"));
            } else if reset {
                if name == "t_Co" {
                    store.termcap.insert(
                        name.clone(),
                        rvim_platform::term::capabilities().colors.to_string(),
                    );
                } else {
                    store.termcap.remove(&name);
                }
            } else if let Some(value) = value {
                store.termcap.insert(name, value);
            }
            continue;
        }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("patched")
