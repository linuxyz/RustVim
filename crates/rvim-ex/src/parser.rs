use crate::ast::{Address, AddressKind, ExCommand, Modifier, RangeSep, RangeSpec};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExParseError {
    pub message: String,
    pub offset: usize,
}

impl ExParseError {
    fn at(offset: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }
}

impl std::fmt::Display for ExParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.message, self.offset)
    }
}

impl std::error::Error for ExParseError {}

pub fn parse(input: &str) -> Result<ExCommand, ExParseError> {
    let raw = input.to_string();
    let bytes = input.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() && (bytes[pos] == b':' || bytes[pos] == b' ' || bytes[pos] == b'\t') {
        pos += 1;
    }
    let mut command = ExCommand::new("");
    command.raw = raw;

    // Command modifiers are prefix words followed by whitespace.
    loop {
        skip_space(bytes, &mut pos);
        let start = pos;
        let Some(word) = read_word(bytes, &mut pos) else {
            break;
        };
        let lower = word.to_ascii_lowercase();
        if lower == "filter" {
            skip_space(bytes, &mut pos);
            if pos >= bytes.len() {
                return Ok(command);
            }
            let delimiter = bytes[pos] as char;
            if delimiter.is_ascii_alphanumeric() || delimiter == '\\' {
                return Err(ExParseError::at(pos, "E475: Invalid argument: filter"));
            }
            pos += 1;
            let mut pattern = String::new();
            while pos < bytes.len() {
                let ch = bytes[pos] as char;
                if ch == delimiter {
                    pos += 1;
                    break;
                }
                pattern.push(ch);
                pos += 1;
            }
            command.filter = Some(pattern);
            if pos >= bytes.len() {
                return Ok(command);
            }
            continue;
        }
        let modifier = match lower.as_str() {
            "silent" => Some(Modifier::Silent),
            "verbose" => {
                skip_space(bytes, &mut pos);
                let number = read_number(bytes, &mut pos);
                Some(Modifier::Verbose(number))
            }
            "noautocmd" | "noa" => Some(Modifier::NoAutocmd),
            "keepjumps" | "keepj" => Some(Modifier::KeepJumps),
            "keepmarks" | "keepm" => Some(Modifier::KeepMarks),
            "keepalt" | "keepa" => Some(Modifier::KeepAlt),
            "lockmarks" | "lockm" => Some(Modifier::LockMarks),
            "sandbox" => Some(Modifier::Sandbox),
            "confirm" | "conf" => Some(Modifier::Confirm),
            "hide" | "hid" => Some(Modifier::Hide),
            "browse" | "bro" => Some(Modifier::Browse),
            "unsilent" | "uns" => Some(Modifier::Unsilent),
            "vertical" | "vert" => Some(Modifier::Vertical),
            "horizontal" | "hor" => Some(Modifier::Horizontal),
            "aboveleft" | "abo" => Some(Modifier::AboveLeft),
            "belowright" | "bel" => Some(Modifier::BelowRight),
            "leftabove" | "lefta" => Some(Modifier::LeftAbove),
            "rightbelow" | "rightb" => Some(Modifier::RightBelow),
            "tab" => Some(Modifier::Tab),
            "top" => Some(Modifier::Top),
            "botright" | "bo" => Some(Modifier::BotRight),
            "topleft" | "to" => Some(Modifier::Topleft),
            _ => None,
        };
        if let Some(modifier) = modifier {
            command.modifiers.push(modifier);
            skip_space(bytes, &mut pos);
            if pos < bytes.len() && bytes[pos] == b'!' && lower == "silent" {
                command.modifiers.pop();
                command.modifiers.push(Modifier::SilentBang);
                pos += 1;
            }
            if pos >= bytes.len() {
                return Ok(command);
            }
            if bytes[pos] == b'"' {
                return Ok(command);
            }
            continue;
        }
        pos = start;
        break;
    }

    command.range = parse_range(input, &mut pos)?;
    skip_space(bytes, &mut pos);
    if pos < bytes.len() && bytes[pos] == b'"' {
        return Ok(command);
    }
    if pos < bytes.len() && bytes[pos] == b'!' {
        pos += 1;
        command.name = "!".to_string();
        command.args = input[pos..].trim_start().to_string();
        return Ok(command);
    }
    if pos >= bytes.len() {
        // A range with no command, such as `:42` or `:%`.
        return Ok(command);
    }
    let name_start = pos;
    while pos < bytes.len() && (bytes[pos].is_ascii_alphabetic() || bytes[pos] == b'_') {
        pos += 1;
    }
    if pos == name_start {
        if pos < bytes.len() && bytes[pos].is_ascii_digit() {
            // A bare number is a range with no command; useful for :{range}.
            command.range.addresses.push(Address::new(AddressKind::Line(
                input[name_start..].trim().parse().unwrap_or(0),
            )));
            return Ok(command);
        }
        if input[..pos].trim().is_empty() {
            return Ok(command);
        }
        return Err(ExParseError::at(pos, "command name expected"));
    }
    command.name = input[name_start..pos].to_string();
    if pos < bytes.len() && bytes[pos] == b'!' {
        command.bang = true;
        pos += 1;
    }
    // Commands like :s use a punctuation suffix but keep the name "s".
    skip_space(bytes, &mut pos);
    command.args = input[pos..].to_string();
    Ok(command)
}

fn parse_range(input: &str, pos: &mut usize) -> Result<RangeSpec, ExParseError> {
    let bytes = input.as_bytes();
    let mut range = RangeSpec::default();
    if *pos >= bytes.len() {
        return Ok(range);
    }
    if bytes[*pos] == b'%' {
        range.addresses.push(Address::new(AddressKind::WholeFile));
        *pos += 1;
        skip_space(bytes, pos);
        return Ok(range);
    }
    if !starts_address(bytes, *pos) {
        return Ok(range);
    }
    let first = parse_address(input, pos)?;
    range.addresses.push(first);
    skip_space(bytes, pos);
    if *pos < bytes.len() && (bytes[*pos] == b',' || bytes[*pos] == b';') {
        range.separator = Some(if bytes[*pos] == b',' {
            RangeSep::Comma
        } else {
            RangeSep::Semicolon
        });
        *pos += 1;
        skip_space(bytes, pos);
        if !starts_address(bytes, *pos) {
            range.addresses.push(Address::new(AddressKind::Current));
        } else {
            let second = parse_address(input, pos)?;
            range.addresses.push(second);
        }
    }
    Ok(range)
}

fn starts_address(bytes: &[u8], pos: usize) -> bool {
    matches!(
        bytes.get(pos),
        Some(b'.')
            | Some(b'$')
            | Some(b'\'')
            | Some(b'/')
            | Some(b'?')
            | Some(b'+')
            | Some(b'-')
            | Some(b'0'..=b'9')
    )
}

fn parse_address(input: &str, pos: &mut usize) -> Result<Address, ExParseError> {
    let bytes = input.as_bytes();
    let mut address = match bytes.get(*pos) {
        Some(b'.') => {
            *pos += 1;
            Address::new(AddressKind::Current)
        }
        Some(b'$') => {
            *pos += 1;
            Address::new(AddressKind::Last)
        }
        Some(b'\'') => {
            *pos += 1;
            let mark = *bytes
                .get(*pos)
                .ok_or_else(|| ExParseError::at(*pos, "mark expected"))?;
            if !(mark as char).is_ascii_alphabetic()
                && mark != b'\''
                && mark != b'<'
                && mark != b'>'
            {
                return Err(ExParseError::at(*pos, "invalid mark"));
            }
            *pos += 1;
            Address::new(AddressKind::Mark(mark as char))
        }
        Some(b'/') | Some(b'?') => {
            let delim = bytes[*pos];
            let direction = if delim == b'/' { 1 } else { -1 };
            *pos += 1;
            let start = *pos;
            while *pos < bytes.len() && bytes[*pos] != delim {
                if bytes[*pos] == b'\\' {
                    *pos += 1;
                }
                if *pos < bytes.len() {
                    *pos += 1;
                }
            }
            let pattern = input[start..*pos].to_string();
            if *pos < bytes.len() {
                *pos += 1;
            }
            Address::new(AddressKind::Search { pattern, direction })
        }
        Some(b'0'..=b'9') => {
            let line = read_number(bytes, pos).unwrap_or(0);
            Address::new(AddressKind::Line(line))
        }
        Some(b'+') | Some(b'-') => Address::new(AddressKind::Current),
        _ => return Err(ExParseError::at(*pos, "address expected")),
    };
    // Offsets: +N, -N, +, -, and +/pat/ (search-based offset).
    while matches!(bytes.get(*pos), Some(b'+') | Some(b'-')) {
        let sign = if bytes[*pos] == b'-' { -1 } else { 1 };
        *pos += 1;
        if matches!(bytes.get(*pos), Some(b'/') | Some(b'?')) {
            let delim = bytes[*pos];
            let direction = if delim == b'/' { 1 } else { -1 };
            *pos += 1;
            let start = *pos;
            while *pos < bytes.len() && bytes[*pos] != delim {
                *pos += 1;
            }
            let pattern = input[start..*pos].to_string();
            if *pos < bytes.len() {
                *pos += 1;
            }
            address.offset_pattern = Some((pattern, direction));
        } else {
            let number = read_number(bytes, pos).unwrap_or(1);
            address.offset += sign * number as i32;
        }
    }
    Ok(address)
}

fn read_word(bytes: &[u8], pos: &mut usize) -> Option<String> {
    let start = *pos;
    while *pos < bytes.len() && (bytes[*pos].is_ascii_alphabetic() || bytes[*pos] == b'_') {
        *pos += 1;
    }
    if *pos == start {
        None
    } else {
        Some(String::from_utf8_lossy(&bytes[start..*pos]).into_owned())
    }
}

fn read_number(bytes: &[u8], pos: &mut usize) -> Option<u32> {
    let start = *pos;
    let mut value = 0u32;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add((bytes[*pos] - b'0') as u32);
        *pos += 1;
    }
    (*pos > start).then_some(value)
}

fn skip_space(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && (bytes[*pos] == b' ' || bytes[*pos] == b'\t') {
        *pos += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_write_with_range() {
        let cmd = parse(":%w! /tmp/out").unwrap();
        assert_eq!(cmd.name, "w");
        assert!(cmd.bang);
        assert!(cmd.range.is_whole_file());
        assert_eq!(cmd.args, "/tmp/out");
    }

    #[test]
    fn parses_addresses_and_offsets() {
        let cmd = parse(":2,4+3d").unwrap();
        assert_eq!(cmd.name, "d");
        assert_eq!(cmd.range.addresses.len(), 2);
        assert_eq!(cmd.range.addresses[0].kind, AddressKind::Line(2));
        assert_eq!(cmd.range.addresses[1].offset, 3);
    }

    #[test]
    fn parses_modifiers_and_mark() {
        let cmd = parse(":silent! keepjumps 'a,$s/x/y/g").unwrap();
        assert_eq!(
            cmd.modifiers,
            vec![Modifier::SilentBang, Modifier::KeepJumps]
        );
        assert_eq!(cmd.name, "s");
        assert_eq!(cmd.range.addresses[0].kind, AddressKind::Mark('a'));
        assert_eq!(cmd.args, "/x/y/g");
    }

    #[test]
    fn parse_set_arguments_verbatim() {
        let cmd = parse(":set number relativenumber?").unwrap();
        assert_eq!(cmd.name, "set");
        assert_eq!(cmd.args, "number relativenumber?");
    }
}
