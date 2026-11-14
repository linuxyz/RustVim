import pathlib
p = pathlib.Path("crates/rvim-editor/src/tags.rs")
s = p.read_text()
s = s.replace("""    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }""", """    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }

    pub fn all_entries(&self) -> impl Iterator<Item = &TagEntry> {
        self.by_name.values().flatten()
    }""", 1)
p.write_text(s)
print("tags patched")
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getmarklist" => {"""
arms = """            "tagfiles" => {
                let tags = self.editor.options.get_string("tags");
                let files: Vec<Value> = tags
                    .split([',', ' '])
                    .filter(|item| !item.is_empty())
                    .map(|item| Value::string(item.to_string()))
                    .collect();
                Ok(Some(Value::list(files)))
            }
            "taglist" => {
                let pattern = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let filename = args.get(1).map(|value| value.echo_string());
                let regex = if let Some(pattern) = pattern.strip_prefix('/') {
                    rvim_regex::compile(
                        pattern.as_bytes(),
                        rvim_regex::Magic::Magic,
                        false,
                    )
                    .ok()
                } else {
                    None
                };
                let mut entries = Vec::new();
                for entry in self.editor.tag_index.all_entries() {
                    let matches = if let Some(regex) = &regex {
                        rvim_regex::is_match(regex, entry.name.as_bytes())
                    } else {
                        entry.name == pattern
                    };
                    if !matches {
                        continue;
                    }
                    if let Some(filename) = &filename {
                        if !filename.is_empty()
                            && !entry.file.to_string_lossy().ends_with(filename)
                        {
                            continue;
                        }
                    }
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("name"), Value::string(entry.name.as_str()));
                    dict.set(
                        VimString::from("filename"),
                        Value::string(entry.file.display().to_string()),
                    );
                    dict.set(VimString::from("cmd"), Value::string(entry.command.as_str()));
                    dict.set(VimString::from("kind"), Value::string(""));
                    dict.set(VimString::from("static"), Value::Number(0));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "getmarklist" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("host patched")
