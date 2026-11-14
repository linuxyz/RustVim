use std::path::PathBuf;

use crate::qf::QfEntry;

#[derive(Clone, Debug)]
enum Token {
    Literal(String),
    File,
    Line,
    Column,
    Message,
    Number,
    Type,
    Any,
    Scan(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormatKind {
    Normal,
    Start,
    Continue,
    End,
}

#[derive(Clone, Debug)]
struct Format {
    tokens: Vec<Token>,
    ignore: bool,
    kind: FormatKind,
    directory: Option<bool>,
    file_stack: Option<bool>,
    error_kind: Option<char>,
}

pub fn parse(text: &str, errorformat: &str) -> Vec<QfEntry> {
    let formats = compile_formats(errorformat);
    let mut entries = Vec::new();
    let mut pending: Option<QfEntry> = None;
    let mut directories: Vec<PathBuf> = Vec::new();
    for line in text.lines() {
        let mut matched = false;
        for format in &formats {
            let Some(mut entry) = match_format(line, format) else {
                continue;
            };
            matched = true;
            if format.ignore {
                break;
            }
            if let Some(push) = format.directory.or(format.file_stack) {
                if push {
                    directories.push(entry.file);
                } else {
                    directories.pop();
                }
                break;
            }
            if entry.file.is_relative() {
                if let Some(directory) = directories.last() {
                    entry.file = directory.join(&entry.file);
                }
            }
            match format.kind {
                FormatKind::Start => {
                    if let Some(previous) = pending.take() {
                        entries.push(previous);
                    }
                    pending = Some(entry);
                }
                FormatKind::Continue => {
                    if let Some(previous) = pending.as_mut() {
                        previous.text.push('\n');
                        previous.text.push_str(&entry.text);
                    } else {
                        entries.push(entry);
                    }
                }
                FormatKind::End => {
                    if let Some(mut previous) = pending.take() {
                        previous.text.push('\n');
                        previous.text.push_str(&entry.text);
                        entries.push(previous);
                    } else {
                        entries.push(entry);
                    }
                }
                FormatKind::Normal => entries.push(entry),
            }
            break;
        }
        if !matched {
            if let Some(previous) = pending.take() {
                entries.push(previous);
            }
        }
    }
    if let Some(previous) = pending {
        entries.push(previous);
    }
    entries
}

pub fn parse_line(line: &str, errorformat: &str) -> Option<QfEntry> {
    parse_with_formats(line, &compile_formats(errorformat))
}

fn compile_formats(errorformat: &str) -> Vec<Format> {
    split_formats(errorformat)
        .into_iter()
        .filter_map(|format| compile_format(&format))
        .collect()
}

fn split_formats(errorformat: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    let mut depth = 0usize;
    for ch in errorformat.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' {
            current.push(ch);
            escaped = true;
        } else if ch == '(' {
            depth += 1;
            current.push(ch);
        } else if ch == ')' {
            depth = depth.saturating_sub(1);
            current.push(ch);
        } else if ch == ',' && depth == 0 {
            out.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn compile_format(format: &str) -> Option<Format> {
    let mut chars = format.chars().peekable();
    let mut prefix = String::new();
    while let Some(ch) = chars.peek() {
        if *ch == '%' {
            break;
        }
        prefix.push(chars.next()?);
    }
    let mut ignore = false;
    if prefix.contains("-G") || prefix.contains("+G") {
        ignore = prefix.contains("-G");
    }
    let mut kind = FormatKind::Normal;
    let mut directory = None;
    let mut file_stack = None;
    let mut error_kind = None;
    let mut tokens = Vec::new();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            match tokens.last_mut() {
                Some(Token::Literal(text)) => text.push(ch),
                _ => tokens.push(Token::Literal(ch.to_string())),
            }
            continue;
        }
        let mut modifier = None;
        if let Some(next) = chars.peek() {
            if matches!(next, '-' | '+' | '#') {
                modifier = Some(*next);
                chars.next();
            }
        }
        let code = chars.next()?;
        match code {
            '%' => push_literal(&mut tokens, '%'),
            'f' => tokens.push(Token::File),
            'l' => tokens.push(Token::Line),
            'c' | 'v' => tokens.push(Token::Column),
            'm' => tokens.push(Token::Message),
            'n' => tokens.push(Token::Number),
            't' => tokens.push(Token::Type),
            's' => tokens.push(Token::Message),
            'p' | 'o' | 'r' => tokens.push(Token::Any),
            '*' => {
                let pattern = if chars.peek() == Some(&'[') {
                    let mut pattern = String::new();
                    for ch in chars.by_ref() {
                        pattern.push(ch);
                        if ch == ']' {
                            break;
                        }
                    }
                    pattern
                } else if chars.peek() == Some(&'\\') {
                    chars.next();
                    let next = chars.next().unwrap_or_default();
                    format!("\\{next}")
                } else {
                    chars.next().map(|ch| ch.to_string()).unwrap_or_default()
                };
                tokens.push(Token::Scan(pattern));
            }
            '#' => tokens.push(Token::Any),
            'G' => ignore = modifier == Some('-'),
            'A' => kind = FormatKind::Start,
            'C' => kind = FormatKind::Continue,
            'Z' => kind = FormatKind::End,
            'D' => directory = Some(true),
            'X' => directory = Some(false),
            'P' => file_stack = Some(true),
            'Q' => file_stack = Some(false),
            'E' => error_kind = Some('E'),
            'W' => error_kind = Some('W'),
            'I' => error_kind = Some('I'),
            _ => push_literal(&mut tokens, code),
        }
    }
    if tokens.is_empty() {
        return None;
    }
    Some(Format {
        tokens,
        ignore,
        kind,
        directory,
        file_stack,
        error_kind,
    })
}

fn push_literal(tokens: &mut Vec<Token>, ch: char) {
    match tokens.last_mut() {
        Some(Token::Literal(text)) => text.push(ch),
        _ => tokens.push(Token::Literal(ch.to_string())),
    }
}

fn parse_with_formats(line: &str, formats: &[Format]) -> Option<QfEntry> {
    for format in formats {
        if format.directory.is_some() || format.file_stack.is_some() {
            continue;
        }
        if let Some(entry) = match_format(line, format) {
            if format.ignore {
                return None;
            }
            return Some(entry);
        }
    }
    None
}

fn match_format(line: &str, format: &Format) -> Option<QfEntry> {
    let bytes = line.as_bytes();
    let mut position = 0;
    let mut file = String::new();
    let mut lnum = 1u32;
    let mut col = 1u32;
    let mut text = String::new();
    let mut kind = format.error_kind;
    let mut nr = None;
    for (index, token) in format.tokens.iter().enumerate() {
        match token {
            Token::Literal(literal) => {
                let rest = &line[position..];
                let offset = rest.find(literal)?;
                position += offset + literal.len();
            }
            Token::File => {
                let next = format
                    .tokens
                    .get(index + 1)
                    .and_then(|token| match token {
                        Token::Literal(literal) => Some(literal.as_str()),
                        _ => None,
                    })
                    .unwrap_or("");
                let rest = &line[position..];
                let end = if next.is_empty() {
                    rest.len()
                } else {
                    rest.find(next)?
                };
                file = rest[..end].trim().to_string();
                position += end;
            }
            Token::Line | Token::Column | Token::Number => {
                while position < bytes.len() && bytes[position] == b' ' {
                    position += 1;
                }
                let start = position;
                while position < bytes.len() && bytes[position].is_ascii_digit() {
                    position += 1;
                }
                if start == position {
                    if matches!(token, Token::Line | Token::Column) {
                        continue;
                    }
                    return None;
                }
                let value = line[start..position].parse::<u32>().ok()?;
                if matches!(token, Token::Line) {
                    lnum = value.max(1);
                } else if matches!(token, Token::Column) {
                    col = value.max(1);
                } else {
                    nr = Some(value as i64);
                }
            }
            Token::Message => {
                text = line[position..].trim().to_string();
                position = line.len();
            }
            Token::Type => {
                if position < bytes.len() {
                    kind = Some(bytes[position] as char);
                    position += 1;
                }
            }
            Token::Any => {}
            Token::Scan(pattern) => {
                if let Some(set) = pattern
                    .strip_prefix("[^")
                    .and_then(|set| set.strip_suffix(']'))
                {
                    let stop = set.chars().next().unwrap_or(' ');
                    while position < bytes.len() && bytes[position] as char != stop {
                        position += 1;
                    }
                } else if let Some(set) = pattern
                    .strip_prefix('[')
                    .and_then(|set| set.strip_suffix(']'))
                {
                    while position < bytes.len() && set.contains(bytes[position] as char) {
                        position += 1;
                    }
                } else if pattern == "\\d" {
                    while position < bytes.len() && bytes[position].is_ascii_digit() {
                        position += 1;
                    }
                } else if position < bytes.len() {
                    position += 1;
                }
            }
        }
    }
    if file.is_empty() && matches!(format.kind, FormatKind::Normal | FormatKind::Start) {
        return None;
    }
    Some(QfEntry {
        file: PathBuf::from(file),
        lnum,
        col,
        text,
        kind,
        nr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_errorformats() {
        let entries = parse(
            "src/main.rs:12:5: error: bad thing\nother.rs:3:1: warning: hmm\n",
            "%f:%l:%c: %m",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].file, PathBuf::from("src/main.rs"));
        assert_eq!(entries[0].lnum, 12);
        assert_eq!(entries[0].col, 5);
        assert_eq!(entries[0].text, "error: bad thing");
    }

    #[test]
    fn parenthesized_format() {
        let entries = parse("file.c(9,2): syntax error\n", "%f(%l,%c): %m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file, PathBuf::from("file.c"));
        assert_eq!(entries[0].lnum, 9);
        assert_eq!(entries[0].col, 2);
        assert_eq!(entries[0].text, "syntax error");
    }

    #[test]
    fn error_numbers_are_captured() {
        let entries = parse("file.rs:3:7:42:oops\n", "%f:%l:%c:%n:%m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].nr, Some(42));
        assert_eq!(entries[0].text, "oops");
    }

    #[test]
    fn error_types_are_captured() {
        let entries = parse(
            "a.rs:2: E bad
",
            "%f:%l: %t %m",
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, Some('E'));
        let entries = parse(
            "a.rs:2: warning
",
            "%W%f:%l: %m",
        );
        assert_eq!(entries[0].kind, Some('W'));
        let entries = parse(
            "a.rs:2: note
",
            "%I%f:%l: %m",
        );
        assert_eq!(entries[0].kind, Some('I'));
    }

    #[test]
    fn ignore_format_filters_lines() {
        let entries = parse(
            "noise\nsrc/lib.rs:4:7: real error\n",
            "%-Gnoise,%f:%l:%c: %m",
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].lnum, 4);
    }
}

#[cfg(test)]
mod multiline_tests {
    use super::*;

    #[test]
    fn grouped_errorformat_appends_continuations() {
        let text = "src/a.rs:3: error: boom\n  detail: more\n  end: done\n";
        let entries = parse(text, "%A%f:%l: error: %m,%C  detail: %m,%Z  end: %m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("src/a.rs"));
        assert_eq!(entries[0].lnum, 3);
        assert!(entries[0].text.contains("boom"));
        assert!(entries[0].text.contains("more"), "{:?}", entries);
        assert!(entries[0].text.contains("done"));
    }
}

#[cfg(test)]
mod directory_tests {
    use super::*;

    #[test]
    fn directory_formats_adjust_relative_files() {
        let text = "Entering src\nmain.rs:3:7: boom\nLeaving src\n";
        let entries = parse(text, "%DEntering %f,%f:%l:%c: %m,%XLeaving %f");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("src/main.rs"));
    }
}

#[cfg(test)]
mod file_stack_tests {
    use super::*;

    #[test]
    fn file_stack_prefixes_relative_files() {
        let text = "Entering dir/\nfile.rs:3: boom\nLeaving dir/\n";
        let entries = parse(text, "%PEntering %f,%f:%l: %m,%QLeaving %f");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("dir/file.rs"));
    }
}
