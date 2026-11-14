import pathlib
p = pathlib.Path("crates/rvim-editor/src/viminfo.rs")
s = p.read_text()
s = s.replace("""            let Some(Some(name)) = parts.next().and_then(|text| text.chars().next().map(Some))
                .map(Some)
            else {
                continue;
            };
            let Some(name) = RegisterName::parse(name) else {
                continue;
            };""", """            let Some(name) = parts
                .next()
                .and_then(|text| text.chars().next())
                .and_then(RegisterName::parse)
            else {
                continue;
            };""", 1)
s = s.replace("""        if let Some(name) = buffer.name_string().is_empty().then(|| None).or_else(|| Some(buffer.name_string())) {""", """        let name = buffer.name_string();
        if !name.is_empty() {""", 1)
p.write_text(s)
print("viminfo cleaned")
p = pathlib.Path("crates/rvim-editor/src/lib.rs")
s = p.read_text()
if "pub mod viminfo;" not in s:
    s += "\npub mod viminfo;\n"
p.write_text(s)
print("lib patched")
