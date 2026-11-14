use std::path::{Path, PathBuf};

pub fn expand(pattern: &str) -> Vec<PathBuf> {
    let path = Path::new(pattern);
    let absolute = path.is_absolute();
    let base = if absolute {
        PathBuf::from("/")
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };
    let components: Vec<String> = path
        .components()
        .filter_map(|component| component.as_os_str().to_str().map(str::to_string))
        .collect();
    let mut out = Vec::new();
    walk(&base, &components, &mut out);
    out.sort();
    out.dedup();
    out
}

fn walk(base: &Path, components: &[String], out: &mut Vec<PathBuf>) {
    if out.len() > 10_000 {
        return;
    }
    let Some((first, rest)) = components.split_first() else {
        if base.is_file() {
            out.push(base.to_path_buf());
        }
        return;
    };
    if first == "**" {
        walk(base, rest, out);
        let Ok(entries) = std::fs::read_dir(base) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, components, out);
            } else if rest.is_empty() {
                out.push(path);
            }
        }
        return;
    }
    if !first.contains(['*', '?', '[']) {
        let next = base.join(first);
        walk(&next, rest, out);
        return;
    }
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if wildcard_match(first, name) {
            walk(&entry.path(), rest, out);
        }
    }
}

fn wildcard_match(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let mut p = 0usize;
    let mut n = 0usize;
    let mut star = None;
    let mut star_name = 0usize;
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            star_name = n;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            star_name += 1;
            n = star_name;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}
