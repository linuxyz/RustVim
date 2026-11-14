use rvim_text::TextBuffer;

/// Apply modelines after a file is loaded. Only a safe subset is accepted;
/// dangerous options are ignored, matching Vim's secure modeline policy.
pub fn modeline_options(buffer: &TextBuffer) -> Vec<String> {
    let mut result = Vec::new();
    if !buffer.meta.modeline {
        return result;
    }
    let count = buffer.meta.modelines.max(0) as u32;
    if count == 0 {
        return result;
    }
    let mut lines = Vec::new();
    for lnum in 1..=count.min(buffer.line_count()) {
        if let Some(line) = buffer.line(lnum) {
            lines.push(String::from_utf8_lossy(line).into_owned());
        }
    }
    let last_start = buffer
        .line_count()
        .saturating_sub(count)
        .saturating_add(1)
        .max(1);
    for lnum in last_start..=buffer.line_count() {
        if let Some(line) = buffer.line(lnum) {
            lines.push(String::from_utf8_lossy(line).into_owned());
        }
    }
    for line in lines {
        if let Some(options) = parse_modeline(&line) {
            if !options.is_empty() {
                result.push(options);
            }
        }
    }
    result
}

pub fn parse_modeline(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let marker = ["vim:", "vi:", "ex:"]
        .into_iter()
        .find(|marker| lower.contains(marker))?;
    let start = lower.find(marker)? + marker.len();
    let mut rest = line[start..].trim_start();
    if let Some(stripped) = rest.strip_prefix("set ") {
        rest = stripped;
    } else if let Some(stripped) = rest.strip_prefix("se ") {
        rest = stripped;
    }
    let mut end = rest.len();
    if let Some(position) = rest.find(':') {
        end = position;
    }
    let options = rest[..end].trim();
    if options.is_empty() || options.contains("secure") {
        return None;
    }
    let filtered: Vec<&str> = options
        .split_whitespace()
        .filter(|option| {
            let name = option
                .trim_start_matches("no")
                .trim_start_matches("inv")
                .split(['=', '+', '-', '^'])
                .next()
                .unwrap_or("");
            !matches!(
                name,
                "shell"
                    | "shellcmdflag"
                    | "shellpipe"
                    | "shellredir"
                    | "backupdir"
                    | "directory"
                    | "undodir"
                    | "exrc"
                    | "secure"
                    | "termcap"
            ) && !name.starts_with("t_")
        })
        .collect();
    if filtered.is_empty() {
        None
    } else {
        Some(filtered.join(" "))
    }
}
