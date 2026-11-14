//! Parsing of compiler output according to the 'errorformat' option.
//!
//! Format strings are compiled into a list of tokens which are then matched
//! left to right against every line of the output.  The supported conversions
//! follow Vim's |errorformat| documentation, including the '+'/'-' modifiers
//! (|efm-ignore|), the '%>' restart item, the '%*' scanf-style scan classes and
//! the escaped regexp meta characters.

use std::iter::Peekable;
use std::path::PathBuf;
use std::str::Chars;

use crate::qf::QfEntry;

/// A scanf-style %* scan item.  It always consumes zero or more characters
/// belonging to the described set.
#[derive(Clone, Debug)]
enum ScanClass {
    /// A %*[a-z] style character set.  When 'negated' is set the scan stops at
    /// the first character that appears in the set.
    Set {
        negated: bool,
        chars: Vec<char>,
        ranges: Vec<(char, char)>,
    },
    /// A Vim character class such as %*\d; \D is the negated class.
    Class(char),
    /// A single literal character, used for %*\, , %*\] and %*\- .
    Char(char),
    /// Any character; used when the scan item is incomplete.
    Any,
}

impl ScanClass {
    fn contains(&self, ch: char) -> bool {
        match self {
            ScanClass::Set {
                negated,
                chars,
                ranges,
            } => {
                let listed =
                    chars.contains(&ch) || ranges.iter().any(|(lo, hi)| (*lo..=*hi).contains(&ch));
                listed != *negated
            }
            ScanClass::Class(class) => class_matches(*class, ch),
            ScanClass::Char(expected) => *expected == ch,
            ScanClass::Any => true,
        }
    }
}

/// Test whether 'ch' belongs to the Vim character class 'class'.
fn class_matches(class: char, ch: char) -> bool {
    match class {
        'd' => ch.is_ascii_digit(),
        'D' => !ch.is_ascii_digit(),
        's' => ch == ' ' || ch == '\t',
        'S' => !(ch == ' ' || ch == '\t'),
        'w' => ch.is_ascii_alphanumeric() || ch == '_',
        'W' => !(ch.is_ascii_alphanumeric() || ch == '_'),
        'a' => ch.is_ascii_alphabetic(),
        'A' => !ch.is_ascii_alphabetic(),
        'l' => ch.is_ascii_lowercase(),
        'L' => !ch.is_ascii_lowercase(),
        'u' => ch.is_ascii_uppercase(),
        'U' => !ch.is_ascii_uppercase(),
        'x' => ch.is_ascii_hexdigit(),
        'X' => !ch.is_ascii_hexdigit(),
        'o' => matches!(ch, '0'..='7'),
        'O' => !matches!(ch, '0'..='7'),
        'h' => ch.is_ascii_alphabetic() || ch == '_',
        'H' => !(ch.is_ascii_alphabetic() || ch == '_'),
        _ => false,
    }
}

fn is_scan_class(ch: char) -> bool {
    matches!(
        ch,
        'd' | 'D'
            | 's'
            | 'S'
            | 'w'
            | 'W'
            | 'a'
            | 'A'
            | 'l'
            | 'L'
            | 'u'
            | 'U'
            | 'x'
            | 'X'
            | 'o'
            | 'O'
            | 'h'
            | 'H'
    )
}

#[derive(Clone, Debug)]
enum Token {
    Literal(String),
    /// %.%# : any text (the '.*' idiom from the documentation).
    Wildcard,
    File,
    Line,
    EndLine,
    Column,
    EndColumn,
    Message,
    /// %n or %b: a number stored in QfEntry::nr.
    Number,
    Type,
    Scan(ScanClass),
    /// %o: the module name, kept as message text because QfEntry has no
    /// separate module field.
    Module,
    /// %s: text to search for, kept as the message.
    Search,
    /// %p: pointer line, used for the column.
    Pointer,
    /// %r: rest of a single-line file message.
    Rest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormatKind {
    Normal,
    /// %A/%E/%W/%I/%N: start of a multi-line message.
    Start,
    /// %C: continuation of a multi-line message.
    Continue,
    /// %Z: end of a multi-line message.
    End,
    /// %O: overread a single-line file message without adding an entry.
    Overread,
}

#[derive(Clone, Debug)]
struct Format {
    tokens: Vec<Token>,
    kind: FormatKind,
    /// '-': the matching line is not included in any output.
    ignore: bool,
    /// '+': the whole matching line is used as the message.
    whole_line: bool,
    /// '%>': start scanning the next line at this format again.
    restart: bool,
    directory: Option<bool>,
    file_stack: Option<bool>,
    error_kind: Option<char>,
}

pub fn parse(text: &str, errorformat: &str) -> Vec<QfEntry> {
    let formats = compile_formats(errorformat);
    let mut entries = Vec::new();
    let mut pending: Option<QfEntry> = None;
    let mut directories: Vec<PathBuf> = Vec::new();
    // '%>' makes the next line start scanning at the same format again.
    let mut resume_at = 0usize;
    for line in text.lines() {
        let start = resume_at;
        resume_at = 0;
        let mut matched = false;
        for (index, format) in formats.iter().enumerate().skip(start) {
            let Some(mut entry) = match_format(line, format) else {
                continue;
            };
            if !format_accepts(format, &entry) {
                continue;
            }
            matched = true;
            if format.restart {
                resume_at = index;
            }
            if format.ignore || format.kind == FormatKind::Overread {
                break;
            }
            if format.whole_line {
                apply_whole_line(&mut entry, line);
            }
            if let Some(push) = format.directory.or(format.file_stack) {
                if push {
                    directories.push(entry.file);
                } else {
                    directories.pop();
                }
                break;
            }
            if !entry.file.as_os_str().is_empty() && entry.file.is_relative() {
                if let Some(directory) = directories.last() {
                    entry.file = directory.join(&entry.file);
                }
            }
            match format.kind {
                FormatKind::Start => {
                    if let Some(previous) = pending.take() {
                        push_entry(&mut entries, previous);
                    }
                    pending = Some(entry);
                }
                FormatKind::Continue => match pending.as_mut() {
                    Some(previous) => merge_continuation(previous, &entry),
                    None => push_entry(&mut entries, entry),
                },
                FormatKind::End => match pending.take() {
                    Some(mut previous) => {
                        merge_continuation(&mut previous, &entry);
                        push_entry(&mut entries, previous);
                    }
                    None => push_entry(&mut entries, entry),
                },
                FormatKind::Normal | FormatKind::Overread => push_entry(&mut entries, entry),
            }
            break;
        }
        if !matched {
            if let Some(previous) = pending.take() {
                push_entry(&mut entries, previous);
            }
        }
    }
    if let Some(previous) = pending {
        push_entry(&mut entries, previous);
    }
    entries
}

pub fn parse_line(line: &str, errorformat: &str) -> Option<QfEntry> {
    parse_with_formats(line, &compile_formats(errorformat))
}

fn parse_with_formats(line: &str, formats: &[Format]) -> Option<QfEntry> {
    for format in formats {
        let Some(mut entry) = match_format(line, format) else {
            continue;
        };
        if !format_accepts(format, &entry) {
            continue;
        }
        if format.ignore
            || format.kind == FormatKind::Overread
            || format.directory.is_some()
            || format.file_stack.is_some()
        {
            return None;
        }
        if format.whole_line {
            apply_whole_line(&mut entry, line);
        }
        normalize(&mut entry);
        return Some(entry);
    }
    None
}

fn push_entry(entries: &mut Vec<QfEntry>, mut entry: QfEntry) {
    normalize(&mut entry);
    entries.push(entry);
}

/// Fields that were not parsed keep the sentinel values 0 / None so that a
/// multi-line continuation can fill them in.
fn normalize(entry: &mut QfEntry) {
    entry.lnum = entry.lnum.max(1);
    entry.col = entry.col.max(1);
}

/// Append the message of a %C/%Z line and fill in fields the start line did
/// not provide, like Vim's qf_parse_multiline_pfx().
fn merge_continuation(previous: &mut QfEntry, entry: &QfEntry) {
    if !entry.text.is_empty() {
        if previous.text.is_empty() {
            previous.text = entry.text.clone();
        } else {
            previous.text.push('\n');
            previous.text.push_str(&entry.text);
        }
    }
    if previous.lnum == 0 {
        previous.lnum = entry.lnum;
    }
    if previous.col == 0 {
        previous.col = entry.col;
    }
    if previous.nr.is_none() {
        previous.nr = entry.nr;
    }
    if previous.kind.is_none() {
        previous.kind = entry.kind;
    }
    if previous.file.as_os_str().is_empty() && !entry.file.as_os_str().is_empty() {
        previous.file = entry.file.clone();
    }
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

    // Blanks after the separating comma are ignored.
    while matches!(chars.peek(), Some(' ' | '\t')) {
        chars.next();
    }

    // Text before the first conversion has to match the start of the line
    // literally.  A backslash escapes the following character, as in the
    // 'errorformat' option itself (\, , \  and \\ ).
    let mut prefix = String::new();
    while let Some(&ch) = chars.peek() {
        if ch == '%' {
            break;
        }
        chars.next();
        match ch {
            '\\' => prefix.push(chars.next().unwrap_or('\\')),
            other => prefix.push(other),
        }
    }

    let mut tokens = Vec::new();
    if !prefix.is_empty() {
        tokens.push(Token::Literal(prefix));
    }
    let mut kind = FormatKind::Normal;
    let mut ignore = false;
    let mut whole_line = false;
    let mut restart = false;
    let mut directory = None;
    let mut file_stack = None;
    let mut error_kind = None;

    while let Some(ch) = chars.next() {
        if ch != '%' {
            match ch {
                '\\' => push_literal(&mut tokens, chars.next().unwrap_or('\\')),
                other => push_literal(&mut tokens, other),
            }
            continue;
        }
        let mut modifier = None;
        if matches!(chars.peek(), Some('+' | '-')) {
            modifier = chars.next();
        }
        let Some(code) = chars.next() else {
            break;
        };
        if modifier == Some('-') {
            ignore = true;
        }
        if modifier == Some('+') {
            whole_line = true;
        }
        match code {
            '%' => push_literal(&mut tokens, '%'),
            '\\' => push_literal(&mut tokens, '\\'),
            '.' => push_literal(&mut tokens, '.'),
            '#' => {
                // Vim turns %# into the regexp '*' atom.  Following a literal
                // '.' (written as %. ) it forms the common %.%# idiom for
                // "any text", which is folded into a wildcard.
                let mut wildcard = false;
                if let Some(Token::Literal(text)) = tokens.last_mut() {
                    if text.ends_with('.') {
                        text.pop();
                        wildcard = true;
                    }
                }
                if wildcard {
                    tokens.push(Token::Wildcard);
                } else {
                    push_literal(&mut tokens, '*');
                }
            }
            '^' => push_literal(&mut tokens, '^'),
            '$' => push_literal(&mut tokens, '$'),
            '[' => push_literal(&mut tokens, '['),
            '~' => push_literal(&mut tokens, '~'),
            'f' => tokens.push(Token::File),
            'b' | 'n' => tokens.push(Token::Number),
            'l' => tokens.push(Token::Line),
            'e' => tokens.push(Token::EndLine),
            'c' | 'v' => tokens.push(Token::Column),
            'k' => tokens.push(Token::EndColumn),
            't' => tokens.push(Token::Type),
            'm' => tokens.push(Token::Message),
            'r' => tokens.push(Token::Rest),
            'p' => tokens.push(Token::Pointer),
            's' => tokens.push(Token::Search),
            'o' => tokens.push(Token::Module),
            '*' => tokens.push(Token::Scan(parse_scan_class(&mut chars))),
            '>' => restart = true,
            'A' => kind = FormatKind::Start,
            'E' => {
                kind = FormatKind::Start;
                error_kind = Some('E');
            }
            'W' => {
                kind = FormatKind::Start;
                error_kind = Some('W');
            }
            'I' => {
                kind = FormatKind::Start;
                error_kind = Some('I');
            }
            'N' => {
                kind = FormatKind::Start;
                error_kind = Some('N');
            }
            'C' => kind = FormatKind::Continue,
            'Z' => kind = FormatKind::End,
            'D' => directory = Some(true),
            'X' => directory = Some(false),
            'P' => file_stack = Some(true),
            'Q' => file_stack = Some(false),
            'O' => kind = FormatKind::Overread,
            // %G only has a meaning in combination with '+' or '-'; both
            // flags were handled above.
            'G' => {}
            other => push_literal(&mut tokens, other),
        }
    }
    if tokens.is_empty() {
        return None;
    }
    Some(Format {
        tokens,
        kind,
        ignore,
        whole_line,
        restart,
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

/// Parse the body of a %* scan item.
fn parse_scan_class(chars: &mut Peekable<Chars<'_>>) -> ScanClass {
    match chars.peek().copied() {
        Some('[') => {
            chars.next();
            let negated = chars.peek() == Some(&'^');
            if negated {
                chars.next();
            }
            let mut raw = String::new();
            let mut content_started = false;
            while let Some(ch) = chars.next() {
                match ch {
                    ']' if content_started => break,
                    '\\' => {
                        raw.push('\\');
                        if let Some(next) = chars.next() {
                            raw.push(next);
                        }
                    }
                    other => raw.push(other),
                }
                content_started = true;
            }
            let (char_list, ranges) = parse_scan_set(&raw);
            ScanClass::Set {
                negated,
                chars: char_list,
                ranges,
            }
        }
        Some('\\') => {
            chars.next();
            match chars.next() {
                Some(class) if is_scan_class(class) => ScanClass::Class(class),
                Some(other) => ScanClass::Char(other),
                None => ScanClass::Char('\\'),
            }
        }
        Some(other) => {
            chars.next();
            ScanClass::Char(other)
        }
        None => ScanClass::Any,
    }
}

/// Split the text between '[' and ']' into single characters and ranges.
fn parse_scan_set(raw: &str) -> (Vec<char>, Vec<(char, char)>) {
    let mut decoded = Vec::new();
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            decoded.push(chars.next().unwrap_or('\\'));
        } else {
            decoded.push(ch);
        }
    }
    let mut literals = Vec::new();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < decoded.len() {
        if index + 2 < decoded.len() && decoded[index + 1] == '-' {
            ranges.push((decoded[index], decoded[index + 2]));
            index += 3;
        } else {
            literals.push(decoded[index]);
            index += 1;
        }
    }
    (literals, ranges)
}

/// Whether a successfully matched format produces a usable quickfix entry.
/// Formats without a file name are only useful when they are ignored, feed
/// multi-line messages, or carry the whole line as their message.
fn format_accepts(format: &Format, entry: &QfEntry) -> bool {
    !entry.file.as_os_str().is_empty()
        || matches!(
            format.kind,
            FormatKind::Continue | FormatKind::End | FormatKind::Overread
        )
        || format.ignore
        || format.whole_line
        || format.directory.is_some()
        || format.file_stack.is_some()
}

/// With '+' Vim fills the error message with the whole matching line.  When
/// the format also captured a %m the line contains, keep just the line so the
/// text is not duplicated.
fn apply_whole_line(entry: &mut QfEntry, line: &str) {
    if !entry.text.is_empty() && !line.contains(&entry.text) {
        entry.text.push('\n');
        entry.text.push_str(line);
    } else {
        entry.text = line.to_string();
    }
}

fn match_format(line: &str, format: &Format) -> Option<QfEntry> {
    let tokens = &format.tokens;
    let mut position = 0usize;
    let mut file = String::new();
    let mut lnum = 0u32;
    let mut col = 0u32;
    let mut text = String::new();
    let mut kind = format.error_kind;
    let mut nr = None;

    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::Literal(literal) => {
                if index == 0 {
                    // A literal before the first conversion is anchored.
                    if !line.starts_with(literal.as_str()) {
                        return None;
                    }
                    position = literal.len();
                } else {
                    let rest = line.get(position..)?;
                    position += rest.find(literal.as_str())? + literal.len();
                }
            }
            Token::Wildcard => {
                position = match tokens.get(index + 1) {
                    Some(Token::Literal(literal)) => {
                        let rest = line.get(position..)?;
                        position + rest.find(literal.as_str())?
                    }
                    _ => line.len(),
                };
            }
            Token::File => {
                file = capture_string(line, &mut position, tokens, index)?
                    .trim()
                    .to_string();
            }
            Token::Search => {
                text = capture_string(line, &mut position, tokens, index)?
                    .trim()
                    .to_string();
            }
            Token::Module => {
                let captured = capture_string(line, &mut position, tokens, index)?.trim();
                if !text.is_empty() && !captured.is_empty() {
                    text.push(' ');
                }
                text.push_str(captured);
            }
            Token::Rest => {
                text = line.get(position..).unwrap_or("").trim().to_string();
                position = line.len();
            }
            Token::Message => {
                text = line.get(position..).unwrap_or("").trim().to_string();
                position = line.len();
            }
            Token::Line | Token::Column | Token::EndLine | Token::EndColumn | Token::Number => {
                match parse_number(line, &mut position) {
                    Some(value) => match token {
                        Token::Line => lnum = value.max(1),
                        Token::Column => col = value.max(1),
                        Token::Number => nr = Some(value as i64),
                        _ => {}
                    },
                    None if matches!(token, Token::Number) => return None,
                    None => {}
                }
            }
            Token::Type => {
                if let Some(ch) = line.get(position..).and_then(|rest| rest.chars().next()) {
                    kind = Some(ch);
                    position += ch.len_utf8();
                }
            }
            Token::Pointer => {
                let start = position;
                while let Some(ch) = line.get(position..).and_then(|rest| rest.chars().next()) {
                    if matches!(ch, '-' | '.' | ' ' | '\t') {
                        position += ch.len_utf8();
                    } else {
                        break;
                    }
                }
                col = pointer_column(&line[start..position]);
                if index + 1 == tokens.len() {
                    text = line.get(position..).unwrap_or("").trim().to_string();
                    position = line.len();
                }
            }
            Token::Scan(class) => {
                while let Some(ch) = line.get(position..).and_then(|rest| rest.chars().next()) {
                    if class.contains(ch) {
                        position += ch.len_utf8();
                    } else {
                        break;
                    }
                }
            }
        }
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

/// Capture the text up to the next literal token (or the end of the line).
fn capture_string<'a>(
    line: &'a str,
    position: &mut usize,
    tokens: &[Token],
    index: usize,
) -> Option<&'a str> {
    let rest = line.get(*position..)?;
    let end = match tokens.get(index + 1) {
        Some(Token::Literal(literal)) if !literal.is_empty() => rest.find(literal.as_str())?,
        _ => rest.len(),
    };
    *position += end;
    Some(&rest[..end])
}

fn parse_number(line: &str, position: &mut usize) -> Option<u32> {
    let bytes = line.as_bytes();
    while bytes.get(*position) == Some(&b' ') {
        *position += 1;
    }
    let start = *position;
    while bytes
        .get(*position)
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        *position += 1;
    }
    if start == *position {
        return None;
    }
    line[start..*position].parse().ok()
}

/// Column of a %p pointer line: one based, with tabs advancing to the next
/// multiple of 8 screen columns.
fn pointer_column(pointer: &str) -> u32 {
    let mut col = 0u32;
    for ch in pointer.chars() {
        col += 1;
        if ch == '\t' {
            col += 7;
            col -= col % 8;
        }
    }
    col + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn match_first(line: &str, errorformat: &str) -> Option<QfEntry> {
        let formats = compile_formats(errorformat);
        formats.iter().find_map(|format| match_format(line, format))
    }

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
        let entries = parse("a.rs:2: E bad\n", "%f:%l: %t %m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, Some('E'));
        let entries = parse("a.rs:2: warning\n", "%W%f:%l: %m");
        assert_eq!(entries[0].kind, Some('W'));
        let entries = parse("a.rs:2: note\n", "%I%f:%l: %m");
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

    #[test]
    fn literal_prefix_is_anchored() {
        let entries = parse("src/main.rs:3: boom\n", "src/%f:%l: %m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("main.rs"));
        assert_eq!(entries[0].lnum, 3);
        // The prefix must be at the start of the line, not just present.
        let entries = parse("other/src/main.rs:3: boom\n", "src/%f:%l: %m");
        assert!(entries.is_empty(), "{entries:?}");
    }

    #[test]
    fn general_and_negated_single_line_formats() {
        // %+G: the whole line becomes the message.
        let entries = parse(
            "Building complete\nsrc/a.rs:3: boom\n",
            "%+GBuilding complete,%f:%l: %m",
        );
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert_eq!(entries[0].text, "Building complete");
        assert!(entries[0].file.as_os_str().is_empty());
        assert_eq!(entries[1].file, PathBuf::from("src/a.rs"));

        // %-G ignores the line, even when another format could match it.
        let entries = parse(
            "src/a.rs: skip me\nsrc/a.rs:3: boom\n",
            "%-G%f: skip me,%f:%l: %m",
        );
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].text, "boom");
    }

    #[test]
    fn plus_modifier_uses_the_whole_line() {
        let entries = parse("a.rs:3: boom\n", "%+A%f:%l: %m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].text, "a.rs:3: boom");
    }

    #[test]
    fn minus_modifier_suppresses_continuations() {
        let entries = parse(
            "a.rs:3: boom\n  detail: more\n",
            "%A%f:%l: %m,%-C  detail: %m",
        );
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].text, "boom");
    }

    #[test]
    fn note_start_keeps_its_kind() {
        let entries = parse("a.rs:2: note body\n", "%N%f:%l: %m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].kind, Some('N'));
    }

    #[test]
    fn scan_classes_support_ranges_and_negation() {
        let entries = parse("abc:a.rs:3: boom\n", "%*[a-z]:%f:%l: %m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("a.rs"));

        // A negated set stops at *any* character of the set, not only the
        // first one ('a' is listed, but this line stops at 'b').
        let entries = parse("zzbstatic/a.rs:3: boom\n", "%*[^ab]static/%f:%l: %m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("a.rs"));

        // %*[^ ] as used by many real-world errorformats.
        let entries = parse("header a.rs:3: boom\n", "%*[^ ] %f:%l: %m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("a.rs"));
    }

    #[test]
    fn scan_classes_match_character_classes() {
        for (efm, line) in [
            ("%*\\d:%f:%l: %m", "42:a.rs:3: boom"),
            ("%*\\D%n:%f:%l: %m", "ab:42:a.rs:3: boom"),
            ("%*\\s:%f:%l: %m", "   :a.rs:3: boom"),
            ("%*\\S:%f:%l: %m", "ab :a.rs:3: boom"),
            ("%*\\w:%f:%l: %m", "a_1:a.rs:3: boom"),
            ("%*\\W%f:%l: %m", "::a.rs:3: boom"),
            ("%*\\a:%f:%l: %m", "abc:a.rs:3: boom"),
            ("%*\\l:%f:%l: %m", "abc:a.rs:3: boom"),
            ("%*\\u:%f:%l: %m", "ABC:a.rs:3: boom"),
            ("%*\\x:%f:%l: %m", "beef:a.rs:3: boom"),
            ("%*\\o:%f:%l: %m", "765:a.rs:3: boom"),
            ("%*\\h:%f:%l: %m", "a_b:a.rs:3: boom"),
        ] {
            let entries = parse(&format!("{line}\n"), efm);
            assert_eq!(entries.len(), 1, "{efm}: {entries:?}");
            assert_eq!(entries[0].file, PathBuf::from("a.rs"), "{efm}");
            assert_eq!(entries[0].lnum, 3, "{efm}");
        }
    }

    #[test]
    fn scan_classes_accept_escaped_punctuation() {
        let entries = parse("a.rs:3:,,,boom\n", "%f:%l:%*\\,%m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].text, "boom");

        let entries = parse("a.rs:3:---boom\n", "%f:%l:%*\\-%m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].text, "boom");
    }

    #[test]
    fn escaped_literal_characters_are_unescaped() {
        // The default 'errorformat' uses \, for a literal comma.
        let entries = parse("\"a.rs\", line 3: boom\n", "\"%f\"\\, line %l: %m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("a.rs"));
        assert_eq!(entries[0].lnum, 3);
        assert_eq!(entries[0].text, "boom");
    }

    #[test]
    fn escaped_meta_characters_match_literally() {
        assert!(match_first("a.b", "a%.b").is_some());
        assert!(match_first("axb", "a%.b").is_none());
        assert!(match_first("a*b", "a%#b").is_some());
        assert!(match_first("a^b", "a%^b").is_some());
        assert!(match_first("a$b", "a%$b").is_some());
        assert!(match_first("a[b", "a%[b").is_some());
        assert!(match_first("a~b", "a%~b").is_some());
        assert!(match_first("a%b", "a%%b").is_some());
        assert!(match_first("a\\b", "a%\\b").is_some());
        // %.%# is the documentation's "any text" idiom.
        assert!(match_first("anything at all", "%.%#").is_some());
        assert!(match_first("  indented text", " %.%#").is_some());
        assert!(match_first(" leading", "%.%#").is_some());
    }

    #[test]
    fn buffer_number_is_stored_like_an_error_number() {
        let entries = parse("a.rs:2:42: oops\n", "%f:%l:%b: %m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].nr, Some(42));
        assert_eq!(entries[0].text, "oops");
    }

    #[test]
    fn search_and_rest_conversions_fill_the_message() {
        let entries = parse("a.rs:3: the text to find\n", "%f:%l: %s");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].lnum, 3);
        assert_eq!(entries[0].text, "the text to find");

        let entries = parse("a.rs:3: rest of line\n", "%f:%l: %r");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].text, "rest of line");
    }

    #[test]
    fn pointer_line_computes_the_column() {
        let entry = parse_line("   ---^ detail", "%C%p^ %m").expect("pointer line");
        assert_eq!(entry.col, 7);
        assert_eq!(entry.text, "detail");
    }

    #[test]
    fn restart_item_rescans_from_the_same_format() {
        let text = "Error in line 123 of foo.c:\nunknown variable \"i\"\n";
        let entries = parse(text, "xxx,%E%>Error in line %l of %f:,%Z%m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("foo.c"));
        assert_eq!(entries[0].lnum, 123);
        assert_eq!(entries[0].text, "unknown variable \"i\"");
    }

    #[test]
    fn overread_file_message_is_consumed() {
        let entries = parse("[ignored]\na.rs:3: boom\n", "%O[%f],%f:%l: %m");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("a.rs"));
        assert_eq!(entries[0].lnum, 3);
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

    #[test]
    fn doc_multiline_error_example() {
        let text = "Error 275 in src/a.rs\nline 42\ncolumn 3\n' ' expected after '--'\n";
        let entries = parse(
            text,
            "%EError\\ %n\\ in\\ %f,%Cline\\ %l,%Ccolumn\\ %c,%Z%m",
        );
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].file, PathBuf::from("src/a.rs"));
        assert_eq!(entries[0].nr, Some(275));
        assert_eq!(entries[0].lnum, 42);
        assert_eq!(entries[0].col, 3);
        assert_eq!(entries[0].text, "' ' expected after '--'");
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
