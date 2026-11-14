import pathlib
p = pathlib.Path("crates/rvim-editor/src/viminfo.rs")
s = p.read_text()
s = s.replace("commands: Vec<String>,\n    searches: Vec<String>,", "commands: Vec<(String, u64)>,\n    searches: Vec<(String, u64)>,", 1)
# bar handling
old = """        if let Some(rest) = line.strip_prefix("|M,") {"""
new = """        if let Some(rest) = line.strip_prefix("|H,C,") {
            let time = rest.trim().parse::<u64>().unwrap_or(0);
            if let Some(last) = parsed.commands.last_mut() {
                last.1 = time;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("|H,S,") {
            let time = rest.trim().parse::<u64>().unwrap_or(0);
            if let Some(last) = parsed.searches.last_mut() {
                last.1 = time;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("|M,") {"""
assert old in s
s = s.replace(old, new, 1)
s = s.replace("push_unique(&mut parsed.commands, command);", "parsed.commands.push((command.to_string(), 0));", 1)
s = s.replace("push_unique(&mut parsed.searches, search);", "parsed.searches.push((search.to_string(), 0));", 1)
# write command/search merge
old2 = """    let mut commands = Vec::new();
    for command in editor.command_history.iter().rev() {
        push_unique(&mut commands, command);
    }
    for command in &existing.commands {
        push_unique(&mut commands, command);
    }
    let mut searches = Vec::new();
    for search in editor.search_history.iter().rev() {
        push_unique(&mut searches, search);
    }
    for search in &existing.searches {
        push_unique(&mut searches, search);
    }"""
new2 = """    let now = unix_now();
    let commands = merge_history(
        editor.command_history.iter().rev().map(String::as_str),
        &existing.commands,
        now,
    );
    let searches = merge_history(
        editor.search_history.iter().rev().map(String::as_str),
        &existing.searches,
        now,
    );"""
assert old2 in s
s = s.replace(old2, new2, 1)
# output add bar lines
s = s.replace("""    for command in &commands {
        out.push(':');
        out.push_str(command);
        out.push('\\n');
    }""", """    for (command, time) in &commands {
        out.push(':');
        out.push_str(command);
        out.push('\\n');
        out.push_str(&format!("|H,C,{time}\\n"));
    }""", 1)
s = s.replace("""    for search in &searches {
        out.push('/');
        out.push_str(search);
        out.push('\\n');
    }""", """    for (search, time) in &searches {
        out.push('/');
        out.push_str(search);
        out.push('\\n');
        out.push_str(&format!("|H,S,{time}\\n"));
    }""", 1)
# read loops
s = s.replace("""    for command in parsed.commands.into_iter().rev() {
        push_unique_history(&mut editor.command_history, &command);
    }""", """    for (command, _time) in parsed.commands.into_iter().rev() {
        push_unique_history(&mut editor.command_history, &command);
    }""", 1)
s = s.replace("""    for search in parsed.searches.into_iter().rev() {
        push_unique_history(&mut editor.search_history, &search);
    }""", """    for (search, _time) in parsed.searches.into_iter().rev() {
        push_unique_history(&mut editor.search_history, &search);
    }""", 1)
# remove push_unique if unused? add merge_history helper
s = s.replace("""fn push_unique(list: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !list.iter().any(|item| item == value) {
        list.push(value.to_string());
    }
}""", """fn merge_history<'a>(
    current: impl Iterator<Item = &'a str>,
    existing: &[(String, u64)],
    now: u64,
) -> Vec<(String, u64)> {
    let mut entries: Vec<(String, u64)> = Vec::new();
    for item in current {
        if !item.is_empty() && !entries.iter().any(|entry| entry.0 == item) {
            entries.push((item.to_string(), now));
        }
    }
    for (item, time) in existing {
        if let Some(entry) = entries.iter_mut().find(|entry| entry.0 == *item) {
            entry.1 = entry.1.max(*time);
        } else if !item.is_empty() {
            entries.push((item.clone(), *time));
        }
    }
    entries.sort_by(|left, right| right.1.cmp(&left.1));
    entries
}""", 1)
p.write_text(s)
print("patched")
