import pathlib
p = pathlib.Path("crates/rvim-editor/src/viminfo.rs")
s = p.read_text()
s = s.replace("marks: Vec<(char, u32, u32, String)>,", "marks: Vec<(char, u32, u32, String, u64)>,", 1)
# bar M handling after R handling block; insert before existing |R? Add separate before |R block
old = """        if let Some(rest) = line.strip_prefix("|R,") {"""
new = """        if let Some(rest) = line.strip_prefix("|M,") {
            let time = rest.trim().parse::<u64>().unwrap_or(0);
            if let Some(last) = parsed.marks.last_mut() {
                last.4 = time;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("|R,") {"""
assert old in s
s = s.replace(old, new, 1)
s = s.replace("parsed.marks.push((mark, lnum, col, name.to_string()));", "parsed.marks.push((mark, lnum, col, name.to_string(), 0));", 1)
# write block
old2 = """    out.push_str("\\n# File marks:\\n");
    let mut marks: Vec<(char, u32, u32, String)> = Vec::new();
    if let Some(buffer) = editor.current_buffer() {
        let name = buffer.name_string();
        if !name.is_empty() {
            for (mark, pos) in &buffer.marks {
                if *mark == '[' || *mark == ']' {
                    continue;
                }
                marks.push((*mark, pos.lnum, pos.col, name.clone()));
            }
        }
    }
    for mark in existing.marks {
        if !marks
            .iter()
            .any(|item| item.0 == mark.0 && item.3 == mark.3)
        {
            marks.push(mark);
        }
    }
    for (mark, lnum, col, name) in marks {
        out.push('\\'');
        out.push(mark);
        out.push_str(&format!("  {lnum}  {col}  {name}\\n"));
    }"""
new2 = """    out.push_str("\\n# File marks:\\n");
    let now = unix_now();
    let mut marks: Vec<(char, u32, u32, String, u64)> = Vec::new();
    if let Some(buffer) = editor.current_buffer() {
        let name = buffer.name_string();
        if !name.is_empty() {
            for (mark, pos) in &buffer.marks {
                if *mark == '[' || *mark == ']' {
                    continue;
                }
                marks.push((*mark, pos.lnum, pos.col, name.clone(), now));
            }
        }
    }
    for mark in existing.marks {
        if let Some(entry) = marks
            .iter_mut()
            .find(|item| item.0 == mark.0 && item.3 == mark.3)
        {
            if mark.4 > now {
                *entry = mark;
            }
        } else {
            marks.push(mark);
        }
    }
    for (mark, lnum, col, name, time) in marks {
        out.push('\\'');
        out.push(mark);
        out.push_str(&format!("  {lnum}  {col}  {name}\\n"));
        out.push_str(&format!("|M,{time}\\n"));
    }"""
assert old2 in s
s = s.replace(old2, new2, 1)
s = s.replace("for (mark, lnum, col, name) in parsed.marks {", "for (mark, lnum, col, name, _time) in parsed.marks {", 1)
p.write_text(s)
print("patched")
