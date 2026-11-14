import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "filereadable" =>"""
arms = """            "glob" => {
                let pattern = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let as_list = args
                    .get(2)
                    .map(|value| value.echo_string() != "0")
                    .unwrap_or(false);
                let matches = glob_patterns(&pattern);
                if as_list {
                    Ok(Some(Value::list(
                        matches.into_iter().map(Value::string).collect(),
                    )))
                } else {
                    Ok(Some(Value::string(matches.join("\\n"))))
                }
            }
            "globpath" => {
                let paths = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let expression = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let as_list = args
                    .get(3)
                    .map(|value| value.echo_string() != "0")
                    .unwrap_or(false);
                let mut matches = Vec::new();
                for directory in paths.split(',') {
                    let pattern = format!("{}/{}", directory.trim_end_matches('/'), expression);
                    matches.extend(glob_patterns(&pattern));
                }
                matches.sort();
                matches.dedup();
                if as_list {
                    Ok(Some(Value::list(
                        matches.into_iter().map(Value::string).collect(),
                    )))
                } else {
                    Ok(Some(Value::string(matches.join("\\n"))))
                }
            }
            "filereadable" =>"""
assert anchor in s
s = s.replace(anchor, arms, 1)
s += '''

fn glob_patterns(pattern: &str) -> Vec<String> {
    let pattern = if let Some(rest) = pattern.strip_prefix("~/") {
        std::env::var("HOME")
            .map(|home| format!("{home}/{rest}"))
            .unwrap_or_else(|_| pattern.to_string())
    } else {
        pattern.to_string()
    };
    if !pattern.contains(['*', '?', '[']) {
        return if std::path::Path::new(&pattern).exists() {
            vec![pattern]
        } else {
            Vec::new()
        };
    }
    let mut root = String::new();
    for (index, ch) in pattern.char_indices() {
        if matches!(ch, '*' | '?' | '[') {
            root = pattern[..index].to_string();
            break;
        }
    }
    if let Some(position) = root.rfind('/') {
        root.truncate(position);
    } else {
        root = ".".to_string();
    }
    if root.is_empty() {
        root = "/".to_string();
    }
    let mut matches = Vec::new();
    walk_glob(std::path::Path::new(&root), &pattern, &mut matches, 0);
    matches.sort();
    matches.dedup();
    matches
}

fn walk_glob(
    path: &std::path::Path,
    pattern: &str,
    out: &mut Vec<String>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let child = entry.path();
        let text = child.display().to_string();
        if wild_match(pattern.as_bytes(), text.as_bytes()) {
            out.push(text.clone());
        }
        if child.is_dir() {
            walk_glob(&child, pattern, out, depth + 1);
        }
    }
}

fn wild_match(pattern: &[u8], text: &[u8]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    match pattern[0] {
        b'*' => {
            wild_match(&pattern[1..], text)
                || (!text.is_empty() && wild_match(pattern, &text[1..]))
        }
        b'?' => !text.is_empty() && wild_match(&pattern[1..], &text[1..]),
        b'[' => {
            let Some(end) = pattern.iter().position(|byte| *byte == b']') else {
                return false;
            };
            if text.is_empty() {
                return false;
            }
            let mut matched = false;
            let mut index = 1;
            let mut negate = false;
            if pattern.get(1) == Some(&b'^') {
                negate = true;
                index = 2;
            }
            while index < end {
                if index + 2 < end && pattern[index + 1] == b'-' {
                    if text[0] >= pattern[index] && text[0] <= pattern[index + 2] {
                        matched = true;
                    }
                    index += 3;
                } else {
                    if text[0] == pattern[index] {
                        matched = true;
                    }
                    index += 1;
                }
            }
            (matched != negate) && wild_match(&pattern[end + 1..], &text[1..])
        }
        byte => {
            !text.is_empty()
                && text[0] == byte
                && wild_match(&pattern[1..], &text[1..])
        }
    }
}
'''
p.write_text(s)
print("patched")
