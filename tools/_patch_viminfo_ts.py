import pathlib
p = pathlib.Path("crates/rvim-editor/src/viminfo.rs")
s = p.read_text()
s = s.replace("registers: Vec<(RegisterName, RegisterKind, Vec<VimString>)>,", "registers: Vec<(RegisterName, RegisterKind, Vec<VimString>, u64)>,", 1)
# bar handling before skip
old = """    for line in text.lines() {
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with('|')
            || line.starts_with('*')
        {
            continue;
        }"""
new = """    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("|R,") {
            let time = rest.trim().parse::<u64>().unwrap_or(0);
            if let Some((name, kind, lines)) = pending.take() {
                parsed.registers.push((name, kind, lines, time));
            } else if let Some(last) = parsed.registers.last_mut() {
                last.3 = time;
            }
            continue;
        }
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with('|')
            || line.starts_with('*')
        {
            continue;
        }"""
assert old in s
s = s.replace(old, new, 1)
s = s.replace("parsed.registers.push((name, kind, lines));", "parsed.registers.push((name, kind, lines, 0));")
# write block
old2 = """    out.push_str("\\n# Registers:\\n");
    let mut current_names = Vec::new();
    for (name, content) in editor.registers.savable() {
        let Some(ch) = register_char(name) else {
            continue;
        };
        current_names.push(ch);
        write_register(&mut out, ch, content);
    }
    for (name, kind, lines) in &existing.registers {
        let Some(ch) = register_char(*name) else {
            continue;
        };
        if current_names.contains(&ch) {
            continue;
        }
        write_register(&mut out, ch, &RegisterContent::new(lines.clone(), *kind));
    }"""
new2 = """    out.push_str("\\n# Registers:\\n");
    let now = unix_now();
    let mut entries: Vec<(char, RegisterContent, u64)> = Vec::new();
    for (name, content) in editor.registers.savable() {
        if let Some(ch) = register_char(name) {
            entries.push((ch, content.clone(), now));
        }
    }
    for (name, kind, lines, time) in &existing.registers {
        let Some(ch) = register_char(*name) else {
            continue;
        };
        if let Some(entry) = entries.iter_mut().find(|entry| entry.0 == ch) {
            if *time > now {
                entry.1 = RegisterContent::new(lines.clone(), *kind);
                entry.2 = *time;
            }
        } else {
            entries.push((ch, RegisterContent::new(lines.clone(), *kind), *time));
        }
    }
    for (ch, content, time) in entries {
        write_register(&mut out, ch, &content);
        out.push_str(&format!("|R,{time}\\n"));
    }"""
assert old2 in s
s = s.replace(old2, new2, 1)
# read loop
s = s.replace("for (name, kind, lines) in parsed.registers {", "for (name, kind, lines, _time) in parsed.registers {", 1)
# helper
s += '''

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
'''
p.write_text(s)
print("patched")
