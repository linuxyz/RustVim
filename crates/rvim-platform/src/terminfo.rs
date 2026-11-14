use std::path::PathBuf;

pub fn lookup_colors(term: &str) -> Option<u16> {
    if term.is_empty() {
        return None;
    }
    let first = term.as_bytes()[0] as char;
    let hex = format!("{:02x}", term.as_bytes()[0]);
    let mut directories: Vec<PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("TERMINFO") {
        directories.push(PathBuf::from(path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        directories.push(PathBuf::from(home).join(".terminfo"));
    }
    for directory in [
        "/usr/share/terminfo",
        "/usr/lib/terminfo",
        "/lib/terminfo",
        "/usr/local/share/terminfo",
    ] {
        directories.push(PathBuf::from(directory));
    }
    for directory in directories {
        for prefix in [first.to_string(), hex.clone()] {
            let path = directory.join(prefix).join(term);
            if let Ok(bytes) = std::fs::read(&path) {
                if let Some(colors) = parse_colors(&bytes) {
                    return Some(colors);
                }
            }
        }
    }
    None
}

fn parse_colors(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 12 {
        return None;
    }
    let magic = u16::from_le_bytes([bytes[0], bytes[1]]);
    let entry_size = match magic {
        0x011A => 2,
        0x021E => 4,
        _ => return None,
    };
    let names_size = u16::from_le_bytes([bytes[2], bytes[3]]) as usize;
    let bool_count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    let num_count = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
    let _str_count = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
    let _str_size = u16::from_le_bytes([bytes[10], bytes[11]]) as usize;
    if num_count <= 13 {
        return None;
    }
    let mut position = 12 + names_size + bool_count;
    if position % 2 == 1 {
        position += 1;
    }
    let offset = position + 13 * entry_size;
    if offset + entry_size > bytes.len() {
        return None;
    }
    let value = if entry_size == 2 {
        u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as u32
    } else {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    let missing = if entry_size == 2 { 0xFFFF } else { 0xFFFF_FFFF };
    if value == missing {
        Some(0)
    } else {
        Some(value.min(u16::MAX as u32) as u16)
    }
}

#[derive(Clone, Debug, Default)]
pub struct TermStrings {
    pub clear: String,
    pub cup: String,
    pub sgr0: String,
    pub civis: String,
    pub cnorm: String,
    pub smcup: String,
    pub rmcup: String,
    pub el: String,
    pub setaf: String,
    pub setab: String,
}

pub fn lookup_strings(term: &str) -> Option<TermStrings> {
    for path in candidate_paths(term) {
        if let Ok(bytes) = std::fs::read(&path) {
            if let Some(strings) = parse_strings(&bytes) {
                return Some(strings);
            }
        }
    }
    None
}

fn candidate_paths(term: &str) -> Vec<PathBuf> {
    if term.is_empty() {
        return Vec::new();
    }
    let first = term.as_bytes()[0] as char;
    let hex = format!("{:02x}", term.as_bytes()[0]);
    let mut directories: Vec<PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("TERMINFO") {
        directories.push(PathBuf::from(path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        directories.push(PathBuf::from(home).join(".terminfo"));
    }
    for directory in [
        "/usr/share/terminfo",
        "/usr/lib/terminfo",
        "/lib/terminfo",
        "/usr/local/share/terminfo",
    ] {
        directories.push(PathBuf::from(directory));
    }
    let mut paths = Vec::new();
    for directory in directories {
        for prefix in [first.to_string(), hex.clone()] {
            paths.push(directory.join(prefix).join(term));
        }
    }
    paths
}

fn parse_strings(bytes: &[u8]) -> Option<TermStrings> {
    let (entry_size, names_size, bool_count, num_count, str_count, position) = header(bytes)?;
    let position = position + num_count * entry_size;
    if position + str_count * 2 > bytes.len() {
        return None;
    }
    let offsets: Vec<u16> = (0..str_count)
        .map(|index| {
            u16::from_le_bytes([bytes[position + index * 2], bytes[position + index * 2 + 1]])
        })
        .collect();
    let table = position + str_count * 2;
    let get = |index: usize| -> String {
        let Some(offset) = offsets.get(index).copied() else {
            return String::new();
        };
        if offset == 0xFFFF {
            return String::new();
        }
        let start = table + offset as usize;
        let Some(rest) = bytes.get(start..) else {
            return String::new();
        };
        let end = rest.iter().position(|byte| *byte == 0).unwrap_or(0);
        String::from_utf8_lossy(&rest[..end]).into_owned()
    };
    let _ = (entry_size, names_size, bool_count);
    Some(TermStrings {
        clear: get(5),
        cup: get(10),
        sgr0: get(39),
        civis: get(13),
        cnorm: get(16),
        smcup: get(28),
        rmcup: get(29),
        el: get(3),
        setaf: get(359),
        setab: get(360),
    })
}

fn header(bytes: &[u8]) -> Option<(usize, usize, usize, usize, usize, usize)> {
    if bytes.len() < 12 {
        return None;
    }
    let magic = u16::from_le_bytes([bytes[0], bytes[1]]);
    let entry_size = match magic {
        0x011A => 2,
        0x021E => 4,
        _ => return None,
    };
    let names_size = u16::from_le_bytes([bytes[2], bytes[3]]) as usize;
    let bool_count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    let num_count = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
    let str_count = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
    let mut position = 12 + names_size + bool_count;
    if position % 2 == 1 {
        position += 1;
    }
    Some((
        entry_size, names_size, bool_count, num_count, str_count, position,
    ))
}

#[cfg(test)]
mod tests {
    use super::{parse_colors, parse_strings};

    #[test]
    fn parses_synthetic_terminfo_strings() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x011Au16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&40u16.to_le_bytes());
        bytes.extend_from_slice(&13u16.to_le_bytes());
        bytes.extend_from_slice(b"foo\0");
        for index in 0..40 {
            let offset: u16 = if index == 5 { 0 } else { 0xFFFF };
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        bytes.extend_from_slice(b"clear\0");
        let strings = parse_strings(&bytes).expect("strings");
        assert_eq!(strings.clear, "clear");
    }

    #[test]
    fn parses_synthetic_terminfo_colors() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x011Au16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&14u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(b"foo\0");
        for index in 0..14u16 {
            let value: u16 = if index == 13 { 256 } else { 0 };
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        assert_eq!(parse_colors(&bytes), Some(256));
    }
}

/// Formats a terminfo parameterized string (tparm) using the stack language
/// subset needed by cursor addressing and color capabilities.
pub fn tparm(capability: &str, params: &[i64]) -> Vec<u8> {
    let tokens = tokenize(capability.as_bytes());
    let mut params = params.to_vec();
    params.resize(9, 0);
    let mut stack: Vec<i64> = Vec::new();
    let mut vars = [0i64; 26];
    let mut out = Vec::new();
    let mut executing = true;
    let mut frames: Vec<Frame> = Vec::new();
    for token in tokens {
        match token {
            Token::CondStart => {
                frames.push(Frame {
                    parent: executing,
                    taken: false,
                });
                continue;
            }
            Token::Then => {
                let condition = if executing {
                    stack.pop().unwrap_or(0)
                } else {
                    0
                };
                if let Some(frame) = frames.last_mut() {
                    frame.taken = condition != 0;
                    executing = frame.parent && condition != 0;
                }
                continue;
            }
            Token::Else => {
                if let Some(frame) = frames.last_mut() {
                    executing = frame.parent && !frame.taken;
                }
                continue;
            }
            Token::EndIf => {
                if let Some(frame) = frames.pop() {
                    executing = frame.parent;
                }
                continue;
            }
            _ => {}
        }
        if !executing {
            continue;
        }
        match token {
            Token::Literal(byte) => out.push(byte),
            Token::Increment => {
                params[0] += 1;
                params[1] += 1;
            }
            Token::Param(index) => {
                stack.push(params.get(index).copied().unwrap_or(0));
            }
            Token::Const(value) => stack.push(value),
            Token::CharConst(byte) => stack.push(byte as i64),
            Token::PopVar(var) => {
                let value = stack.pop().unwrap_or(0);
                vars[(var - b'a') as usize] = value;
            }
            Token::GetVar(var) => stack.push(vars[(var - b'a') as usize]),
            Token::PrintChar => {
                let value = stack.pop().unwrap_or(0);
                out.push(value as u8);
            }
            Token::PrintDecimal => {
                let value = stack.pop().unwrap_or(0);
                out.extend_from_slice(value.to_string().as_bytes());
            }
            Token::PrintOctal => {
                let value = stack.pop().unwrap_or(0);
                out.extend_from_slice(format!("{value:o}").as_bytes());
            }
            Token::PrintHexLower => {
                let value = stack.pop().unwrap_or(0);
                out.extend_from_slice(format!("{value:x}").as_bytes());
            }
            Token::PrintHexUpper => {
                let value = stack.pop().unwrap_or(0);
                out.extend_from_slice(format!("{value:X}").as_bytes());
            }
            Token::Binary(op) => {
                let right = stack.pop().unwrap_or(0);
                let left = stack.pop().unwrap_or(0);
                stack.push(match op {
                    b'+' => left + right,
                    b'-' => left - right,
                    b'*' => left * right,
                    b'/' => {
                        if right == 0 {
                            0
                        } else {
                            left / right
                        }
                    }
                    b'm' => {
                        if right == 0 {
                            0
                        } else {
                            left % right
                        }
                    }
                    b'&' => left & right,
                    b'|' => left | right,
                    b'^' => left ^ right,
                    b'=' => i64::from(left == right),
                    b'>' => i64::from(left > right),
                    b'<' => i64::from(left < right),
                    b'A' => i64::from(left != 0 && right != 0),
                    b'O' => i64::from(left != 0 || right != 0),
                    _ => 0,
                });
            }
            Token::Not => {
                let value = stack.pop().unwrap_or(0);
                stack.push(i64::from(value == 0));
            }
            Token::Complement => {
                let value = stack.pop().unwrap_or(0);
                stack.push(!value);
            }
            Token::Ignored => {}
            Token::CondStart | Token::Then | Token::Else | Token::EndIf => {}
        }
    }
    out
}

/// Formats a cursor-addressing capability for a 1-based row/column pair.
pub fn cursor_address(cup: &str, row: u16, col: u16) -> Vec<u8> {
    tparm(cup, &[row as i64, col as i64])
}

struct Frame {
    parent: bool,
    taken: bool,
}

enum Token {
    Literal(u8),
    Increment,
    Param(usize),
    Const(i64),
    CharConst(u8),
    PopVar(u8),
    GetVar(u8),
    PrintChar,
    PrintDecimal,
    PrintOctal,
    PrintHexLower,
    PrintHexUpper,
    Binary(u8),
    Not,
    Complement,
    Ignored,
    CondStart,
    Then,
    Else,
    EndIf,
}

fn tokenize(bytes: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte != b'%' {
            tokens.push(Token::Literal(byte));
            index += 1;
            continue;
        }
        index += 1;
        let Some(&op) = bytes.get(index) else {
            break;
        };
        index += 1;
        match op {
            b'%' => tokens.push(Token::Literal(b'%')),
            b'i' => tokens.push(Token::Increment),
            b'p' => {
                let digit = bytes.get(index).copied().unwrap_or(b'0');
                index += 1;
                let number = digit.wrapping_sub(b'0') as usize;
                tokens.push(Token::Param(number.saturating_sub(1)));
            }
            b'P' => {
                let var = bytes.get(index).copied().unwrap_or(b'a');
                index += 1;
                tokens.push(Token::PopVar(var));
            }
            b'g' => {
                let var = bytes.get(index).copied().unwrap_or(b'a');
                index += 1;
                tokens.push(Token::GetVar(var));
            }
            b'\'' => {
                let value = bytes.get(index).copied().unwrap_or(0);
                index += 1;
                if bytes.get(index) == Some(&b'\'') {
                    index += 1;
                }
                tokens.push(Token::CharConst(value));
            }
            b'{' => {
                let start = index;
                while index < bytes.len() && bytes[index] != b'}' {
                    index += 1;
                }
                let text = std::str::from_utf8(&bytes[start..index]).unwrap_or("0");
                index += usize::from(index < bytes.len());
                tokens.push(Token::Const(text.parse().unwrap_or(0)));
            }
            b'd' => tokens.push(Token::PrintDecimal),
            b'x' => tokens.push(Token::PrintHexLower),
            b'X' => tokens.push(Token::PrintHexUpper),
            b'o' => tokens.push(Token::PrintOctal),
            b'c' => tokens.push(Token::PrintChar),
            b'+' | b'-' | b'*' | b'/' | b'm' | b'&' | b'|' | b'^' | b'=' | b'>' | b'<' | b'A'
            | b'O' => tokens.push(Token::Binary(op)),
            b'!' => tokens.push(Token::Not),
            b'~' => tokens.push(Token::Complement),
            b'?' => tokens.push(Token::CondStart),
            b't' => tokens.push(Token::Then),
            b'e' => tokens.push(Token::Else),
            b';' => tokens.push(Token::EndIf),
            _ => tokens.push(Token::Ignored),
        }
    }
    tokens
}

#[cfg(test)]
mod tparm_tests {
    use super::{cursor_address, tparm};

    #[test]
    fn formats_cursor_address_with_increment() {
        assert_eq!(cursor_address("\x1b[%i%p1%d;%p2%dH", 2, 3), b"\x1b[3;4H");
    }

    #[test]
    fn evaluates_conditionals_and_arithmetic() {
        let cap = "%?%p1%{1}%=%tone%eother%;";
        assert_eq!(tparm(cap, &[1]), b"one");
        assert_eq!(tparm(cap, &[0]), b"other");
        assert_eq!(tparm("%p1%p2%+%d", &[2, 40]), b"42");
        assert_eq!(tparm("%p1%x", &[255]), b"ff");
        assert_eq!(tparm("%p1%Pa%p2%Pb%ga%gb%-%d", &[50, 8]), b"42");
    }

    #[test]
    fn formats_real_terminal_cursor_capability() {
        if let Some(strings) = super::lookup_strings("xterm") {
            if !strings.cup.is_empty() {
                let out = super::cursor_address(&strings.cup, 0, 0);
                assert!(out.starts_with(b"\x1b["), "{out:?}");
            }
        }
    }

    #[test]
    fn formats_char_and_percent_literals() {
        assert_eq!(tparm("100%%", &[]), b"100%");
        assert_eq!(tparm("%'A'%c", &[]), b"A");
    }
}
