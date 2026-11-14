use rvim_text::TextBuffer;
use rvim_types::unicode::{char_at, char_width};
use rvim_types::BufPos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharClass {
    Whitespace,
    Keyword,
    Punctuation,
}

pub fn word_class(byte: u8) -> CharClass {
    if byte == b' ' || byte == b'\t' || byte == b'\r' || byte == b'\n' {
        CharClass::Whitespace
    } else if byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80 {
        CharClass::Keyword
    } else {
        CharClass::Punctuation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    FirstNonBlank,
    LineEnd,
    GotoLine(u32),
    FileStart,
    FileEnd,
    WordForward,
    WordBackward,
    WordEnd,
    BigWordForward,
    BigWordBackward,
    BigWordEnd,
    WordEndBackward,
    BigWordEndBackward,
    ParagraphForward,
    ParagraphBackward,
    SentenceForward,
    SentenceBackward,
    MatchBracket,
    FindChar { ch: char, forward: bool, till: bool },
    RepeatFind { reverse: bool },
    Column(u32),
    FirstLine,
    LastLine,
    ScrollHalfDown,
    ScrollHalfUp,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
}

pub fn screen_col(buffer: &TextBuffer, pos: BufPos, tabstop: usize) -> u32 {
    let line = buffer.line(pos.lnum).unwrap_or(b"");
    let mut col = 0usize;
    let mut i = 0usize;
    let target = (pos.col as usize).min(line.len());
    while i < target {
        if line[i] == b'\t' {
            let stop = tabstop.max(1);
            col += stop - (col % stop);
            i += 1;
        } else if let Some((ch, len)) = char_at(line, i) {
            col += char_width(ch);
            i += len;
        } else {
            col += 1;
            i += 1;
        }
    }
    col as u32
}

pub fn byte_col(buffer: &TextBuffer, lnum: u32, screen: u32, tabstop: usize) -> u32 {
    let line = buffer.line(lnum).unwrap_or(b"");
    let mut col = 0usize;
    let mut i = 0usize;
    let target = screen as usize;
    while i < line.len() && col < target {
        if line[i] == b'\t' {
            let stop = tabstop.max(1);
            col += stop - (col % stop);
            i += 1;
        } else if let Some((ch, len)) = char_at(line, i) {
            col += char_width(ch);
            i += len;
        } else {
            col += 1;
            i += 1;
        }
    }
    i as u32
}

/// Move a normal-mode cursor. The result is always clamped to a valid byte
/// boundary. Counts are multiplied by the caller; this function performs one
/// conceptual step unless the motion carries its own count.
pub fn move_cursor(buffer: &TextBuffer, pos: BufPos, motion: Motion, tabstop: usize) -> BufPos {
    let count = buffer.line_count().max(1);
    let pos = clamp(buffer, pos);
    match motion {
        Motion::Left => prev_char(buffer, pos),
        Motion::Right => next_char(buffer, pos),
        Motion::Up => vertical(buffer, pos, -1, tabstop),
        Motion::Down => vertical(buffer, pos, 1, tabstop),
        Motion::LineStart => BufPos::new(pos.lnum, 0),
        Motion::FirstNonBlank => BufPos::new(
            pos.lnum,
            first_nonblank(buffer.line(pos.lnum).unwrap_or(b"")),
        ),
        Motion::LineEnd => {
            let len = buffer.line_len(pos.lnum);
            BufPos::new(pos.lnum, len.saturating_sub(1))
        }
        Motion::GotoLine(line) => {
            let lnum = line.max(1).min(count);
            BufPos::new(lnum, first_nonblank(buffer.line(lnum).unwrap_or(b"")))
        }
        Motion::FileStart => BufPos::new(1, first_nonblank(buffer.line(1).unwrap_or(b""))),
        Motion::FileEnd => BufPos::new(count, first_nonblank(buffer.line(count).unwrap_or(b""))),
        Motion::WordForward => word_forward(buffer, pos),
        Motion::WordBackward => word_backward(buffer, pos),
        Motion::WordEnd => word_end(buffer, pos),
        Motion::BigWordForward => big_word_forward(buffer, pos),
        Motion::BigWordBackward => big_word_backward(buffer, pos),
        Motion::BigWordEnd => big_word_end(buffer, pos),
        Motion::WordEndBackward => word_end_backward(buffer, pos),
        Motion::BigWordEndBackward => big_word_end_backward(buffer, pos),
        Motion::ParagraphForward => paragraph(buffer, pos, 1),
        Motion::ParagraphBackward => paragraph(buffer, pos, -1),
        Motion::SentenceForward => sentence(buffer, pos, 1),
        Motion::SentenceBackward => sentence(buffer, pos, -1),
        Motion::MatchBracket => match_bracket(buffer, pos),
        Motion::FindChar { ch, forward, till } => find_char(buffer, pos, ch, forward, till),
        Motion::RepeatFind { .. } => pos,
        Motion::Column(col) => {
            let target = col.saturating_sub(1);
            let byte = byte_col(buffer, pos.lnum, target, tabstop);
            let len = buffer.line_len(pos.lnum);
            BufPos::new(pos.lnum, byte.min(len.saturating_sub(1)))
        }
        Motion::FirstLine => pos,
        Motion::LastLine => pos,
        Motion::ScrollHalfDown
        | Motion::ScrollHalfUp
        | Motion::ScreenTop
        | Motion::ScreenMiddle
        | Motion::ScreenBottom => pos,
    }
}

fn clamp(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let lnum = pos.lnum.max(1).min(buffer.line_count().max(1));
    let len = buffer.line_len(lnum);
    let col = pos.col.min(len.saturating_sub(1));
    BufPos::new(lnum, col)
}

fn prev_char(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    if pos.col > 0 {
        let line = buffer.line(pos.lnum).unwrap_or(b"");
        BufPos::new(pos.lnum, prev_boundary(line, pos.col as usize) as u32)
    } else {
        pos
    }
}

fn next_char(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let len = buffer.line_len(pos.lnum);
    if len == 0 {
        return pos;
    }
    let line = buffer.line(pos.lnum).unwrap_or(b"");
    let next = next_boundary(line, pos.col as usize);
    BufPos::new(pos.lnum, next.min(len.saturating_sub(1) as usize) as u32)
}

fn prev_boundary(line: &[u8], byte: usize) -> usize {
    if byte == 0 {
        return 0;
    }
    let mut i = byte.min(line.len()) - 1;
    while i > 0 && (line[i] & 0xC0) == 0x80 {
        i -= 1;
    }
    i
}

fn next_boundary(line: &[u8], byte: usize) -> usize {
    if byte >= line.len() {
        return line.len();
    }
    let mut i = byte + 1;
    while i < line.len() && (line[i] & 0xC0) == 0x80 {
        i += 1;
    }
    i
}

fn first_nonblank(line: &[u8]) -> u32 {
    line.iter()
        .position(|b| *b != b' ' && *b != b'\t')
        .unwrap_or(0) as u32
}

fn vertical(buffer: &TextBuffer, pos: BufPos, delta: i32, tabstop: usize) -> BufPos {
    let lnum = (pos.lnum as i64 + delta as i64).clamp(1, buffer.line_count() as i64) as u32;
    let screen = screen_col(buffer, pos, tabstop);
    let len = buffer.line_len(lnum);
    if len == 0 {
        return BufPos::new(lnum, 0);
    }
    let byte = byte_col(buffer, lnum, screen, tabstop);
    BufPos::new(lnum, byte.min(len.saturating_sub(1)))
}

fn next_line_start(buffer: &TextBuffer, lnum: u32) -> Option<BufPos> {
    let next = lnum + 1;
    if next > buffer.line_count() {
        return None;
    }
    Some(BufPos::new(
        next,
        first_nonblank(buffer.line(next).unwrap_or(b"")),
    ))
}

fn word_forward(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let line = buffer.line(pos.lnum).unwrap_or(b"");
    let mut i = pos.col as usize;
    if i >= line.len() {
        return next_line_start(buffer, pos.lnum).unwrap_or(pos);
    }
    let class = word_class(line[i]);
    while i < line.len()
        && word_class(line[i]) == class
        && word_class(line[i]) != CharClass::Whitespace
    {
        i += 1;
    }
    while i < line.len() && word_class(line[i]) == CharClass::Whitespace {
        i += 1;
    }
    if i < line.len() {
        BufPos::new(pos.lnum, i as u32)
    } else {
        next_line_start(buffer, pos.lnum)
            .unwrap_or(BufPos::new(pos.lnum, line.len().saturating_sub(1) as u32))
    }
}

fn big_word_forward(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let line = buffer.line(pos.lnum).unwrap_or(b"");
    let mut i = pos.col as usize;
    if i >= line.len() {
        return next_line_start(buffer, pos.lnum).unwrap_or(pos);
    }
    while i < line.len() && line[i] != b' ' && line[i] != b'\t' {
        i += 1;
    }
    while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
        i += 1;
    }
    if i < line.len() {
        BufPos::new(pos.lnum, i as u32)
    } else {
        next_line_start(buffer, pos.lnum)
            .unwrap_or(BufPos::new(pos.lnum, line.len().saturating_sub(1) as u32))
    }
}

fn word_backward(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let line = buffer.line(pos.lnum).unwrap_or(b"");
    let i = pos.col as usize;
    if i == 0 {
        if pos.lnum == 1 {
            return BufPos::new(1, 0);
        }
        let prev = pos.lnum - 1;
        let prev_line = buffer.line(prev).unwrap_or(b"");
        return BufPos::new(prev, first_nonblank(prev_line));
    }
    let mut j = prev_boundary(line, i);
    while j > 0 && word_class(line[j]) == CharClass::Whitespace {
        j = prev_boundary(line, j);
    }
    let class = word_class(line[j]);
    while j > 0 {
        let p = prev_boundary(line, j);
        if word_class(line[p]) != class {
            break;
        }
        j = p;
    }
    BufPos::new(pos.lnum, j as u32)
}

fn big_word_backward(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let line = buffer.line(pos.lnum).unwrap_or(b"");
    let i = pos.col as usize;
    if i == 0 {
        if pos.lnum == 1 {
            return BufPos::new(1, 0);
        }
        let prev = pos.lnum - 1;
        let prev_line = buffer.line(prev).unwrap_or(b"");
        return BufPos::new(prev, first_nonblank(prev_line));
    }
    let mut j = prev_boundary(line, i);
    while j > 0 && (line[j] == b' ' || line[j] == b'\t') {
        j = prev_boundary(line, j);
    }
    while j > 0 {
        let p = prev_boundary(line, j);
        if line[p] == b' ' || line[p] == b'\t' {
            break;
        }
        j = p;
    }
    BufPos::new(pos.lnum, j as u32)
}

fn word_end(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let line = buffer.line(pos.lnum).unwrap_or(b"");
    let mut i = pos.col as usize;
    if i >= line.len() {
        return pos;
    }
    let class = word_class(line[i]);
    if class != CharClass::Whitespace {
        while i + 1 < line.len() && word_class(line[i + 1]) == class {
            i += 1;
        }
        return BufPos::new(pos.lnum, i as u32);
    }
    // Skip whitespace and end the following word.
    while i < line.len() && word_class(line[i]) == CharClass::Whitespace {
        i += 1;
    }
    if i >= line.len() {
        if let Some(next) = next_line_start(buffer, pos.lnum) {
            if buffer.line(next.lnum).unwrap_or(b"").is_empty() {
                return next;
            }
            return word_end(buffer, next);
        }
        return pos;
    }
    let class = word_class(line[i]);
    while i + 1 < line.len() && word_class(line[i + 1]) == class {
        i += 1;
    }
    BufPos::new(pos.lnum, i as u32)
}

fn big_word_end(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let line = buffer.line(pos.lnum).unwrap_or(b"");
    let mut i = pos.col as usize;
    if i >= line.len() {
        return pos;
    }
    if line[i] != b' ' && line[i] != b'\t' {
        while i + 1 < line.len() && line[i + 1] != b' ' && line[i + 1] != b'\t' {
            i += 1;
        }
        return BufPos::new(pos.lnum, i as u32);
    }
    while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
        i += 1;
    }
    if i >= line.len() {
        return next_line_start(buffer, pos.lnum).unwrap_or(pos);
    }
    while i + 1 < line.len() && line[i + 1] != b' ' && line[i + 1] != b'\t' {
        i += 1;
    }
    BufPos::new(pos.lnum, i as u32)
}

fn word_end_backward(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let mut current = word_backward(buffer, pos);
    if current == pos {
        return pos;
    }
    let line = buffer.line(current.lnum).unwrap_or(b"");
    let mut i = current.col as usize;
    if i < line.len() && word_class(line[i]) != CharClass::Whitespace {
        let class = word_class(line[i]);
        while i + 1 < line.len() && word_class(line[i + 1]) == class {
            i += 1;
        }
        current = BufPos::new(current.lnum, i as u32);
    }
    current
}

fn big_word_end_backward(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let current = big_word_backward(buffer, pos);
    let line = buffer.line(current.lnum).unwrap_or(b"");
    let mut i = current.col as usize;
    while i + 1 < line.len() && line[i + 1] != b' ' && line[i + 1] != b'\t' {
        i += 1;
    }
    BufPos::new(current.lnum, i as u32)
}

fn paragraph(buffer: &TextBuffer, pos: BufPos, direction: i32) -> BufPos {
    let mut lnum = pos.lnum as i64;
    let last = buffer.line_count() as i64;
    loop {
        lnum += direction as i64;
        if lnum < 1 {
            return BufPos::new(1, 0);
        }
        if lnum > last {
            return BufPos::new(
                last as u32,
                first_nonblank(buffer.line(last as u32).unwrap_or(b"")),
            );
        }
        if buffer.line(lnum as u32).unwrap_or(b"").is_empty() {
            return BufPos::new(lnum as u32, 0);
        }
    }
}

fn sentence(buffer: &TextBuffer, pos: BufPos, direction: i32) -> BufPos {
    let mut lnum = pos.lnum;
    let mut guard = 0;
    while guard < buffer.line_count() + 2 {
        guard += 1;
        let line = buffer.line(lnum).unwrap_or(b"");
        if direction > 0 {
            if let Some(index) = line.iter().position(|b| matches!(b, b'.' | b'!' | b'?')) {
                let after = index + 1;
                if after >= line.len() {
                    if lnum < buffer.line_count() {
                        lnum += 1;
                        return BufPos::new(lnum, first_nonblank(buffer.line(lnum).unwrap_or(b"")));
                    }
                    return BufPos::new(lnum, line.len().saturating_sub(1) as u32);
                }
                let mut next = after;
                while next < line.len() && (line[next] == b' ' || line[next] == b'\t') {
                    next += 1;
                }
                return BufPos::new(lnum, next as u32);
            }
            if lnum >= buffer.line_count() {
                return pos;
            }
            lnum += 1;
        } else {
            if lnum <= 1 {
                return BufPos::new(1, 0);
            }
            lnum -= 1;
            let line = buffer.line(lnum).unwrap_or(b"");
            if let Some(index) = line.iter().rposition(|b| matches!(b, b'.' | b'!' | b'?')) {
                let mut next = index + 1;
                while next < line.len() && (line[next] == b' ' || line[next] == b'\t') {
                    next += 1;
                }
                return BufPos::new(lnum, next.min(line.len().saturating_sub(1)) as u32);
            }
        }
    }
    pos
}

fn match_bracket(buffer: &TextBuffer, pos: BufPos) -> BufPos {
    let line = buffer.line(pos.lnum).unwrap_or(b"");
    let Some(byte) = line.get(pos.col as usize).copied() else {
        return pos;
    };
    let (open, close, forward) = match byte {
        b'(' => (b'(', b')', true),
        b'[' => (b'[', b']', true),
        b'{' => (b'{', b'}', true),
        b')' => (b'(', b')', false),
        b']' => (b'[', b']', false),
        b'}' => (b'{', b'}', false),
        _ => return pos,
    };
    let mut depth = 0i32;
    let mut lnum = pos.lnum;
    let mut col = pos.col as usize;
    loop {
        let line = buffer.line(lnum).unwrap_or(b"");
        if line.is_empty() {
            if forward {
                lnum += 1;
                if lnum > buffer.line_count() {
                    return pos;
                }
                col = 0;
                continue;
            }
            if lnum == 1 {
                return pos;
            }
            lnum -= 1;
            col = buffer.line(lnum).unwrap_or(b"").len().saturating_sub(1);
            continue;
        }
        while let Some(byte) = line.get(col).copied() {
            if byte == open {
                depth += if forward { 1 } else { -1 };
            } else if byte == close {
                depth += if forward { -1 } else { 1 };
            }
            if depth == 0 && lnum != pos.lnum || depth == 0 && col != pos.col as usize {
                return BufPos::new(lnum, col as u32);
            }
            if forward {
                if col + 1 >= line.len() {
                    break;
                }
                col += 1;
            } else {
                if col == 0 {
                    break;
                }
                col -= 1;
            }
        }
        if forward {
            lnum += 1;
            if lnum > buffer.line_count() {
                return pos;
            }
            col = 0;
        } else {
            if lnum == 1 {
                return pos;
            }
            lnum -= 1;
            col = buffer.line(lnum).unwrap_or(b"").len().saturating_sub(1);
        }
    }
}

fn find_char(buffer: &TextBuffer, pos: BufPos, needle: char, forward: bool, till: bool) -> BufPos {
    let line = buffer.line(pos.lnum).unwrap_or(b"");
    let needle = needle as u32;
    let mut i = pos.col as usize;
    if forward {
        while i + 1 < line.len() {
            i += 1;
            if let Some((ch, _)) = char_at(line, i) {
                if ch as u32 == needle {
                    if till {
                        i = prev_boundary(line, i);
                    }
                    return BufPos::new(pos.lnum, i as u32);
                }
            }
        }
    } else {
        while i > 0 {
            i = prev_boundary(line, i);
            if let Some((ch, _)) = char_at(line, i) {
                if ch as u32 == needle {
                    if till {
                        i = next_boundary(line, i).min(line.len().saturating_sub(1));
                    }
                    return BufPos::new(pos.lnum, i as u32);
                }
            }
        }
    }
    pos
}

/// Returns the visual selection range for an operator/motion, inclusive of the
/// character under the cursor for characterwise motions.
pub fn motion_target(
    buffer: &TextBuffer,
    pos: BufPos,
    motion: Motion,
    count: u32,
) -> (BufPos, bool) {
    let mut current = pos;
    let mut inclusive = matches!(
        motion,
        Motion::WordEnd | Motion::BigWordEnd | Motion::FindChar { .. } | Motion::MatchBracket
    );
    let times = count.max(1);
    for _ in 0..times {
        let next = move_cursor(buffer, current, motion, 8);
        if next == current {
            break;
        }
        current = next;
    }
    if motion == Motion::LineEnd {
        inclusive = true;
    }
    (current, inclusive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvim_text::TextBuffer;

    fn buffer(text: &str) -> TextBuffer {
        let mut b = TextBuffer::new();
        b.load_from_bytes(text.as_bytes());
        b
    }

    #[test]
    fn basic_motions() {
        let b = buffer("hello world\nsecond line\n");
        assert_eq!(
            move_cursor(&b, BufPos::new(1, 0), Motion::Right, 8),
            BufPos::new(1, 1)
        );
        assert_eq!(
            move_cursor(&b, BufPos::new(1, 0), Motion::LineEnd, 8),
            BufPos::new(1, 10)
        );
        assert_eq!(
            move_cursor(&b, BufPos::new(1, 0), Motion::WordForward, 8),
            BufPos::new(1, 6)
        );
        assert_eq!(
            move_cursor(&b, BufPos::new(1, 6), Motion::WordForward, 8),
            BufPos::new(2, 0)
        );
        assert_eq!(
            move_cursor(&b, BufPos::new(1, 6), Motion::WordBackward, 8),
            BufPos::new(1, 0)
        );
    }

    #[test]
    fn bracket_matching() {
        let b = buffer("a (b [c] d) e");
        assert_eq!(
            move_cursor(&b, BufPos::new(1, 2), Motion::MatchBracket, 8),
            BufPos::new(1, 10)
        );
        assert_eq!(
            move_cursor(&b, BufPos::new(1, 10), Motion::MatchBracket, 8),
            BufPos::new(1, 2)
        );
    }

    #[test]
    fn find_char_till() {
        let b = buffer("hello world");
        assert_eq!(
            move_cursor(
                &b,
                BufPos::new(1, 0),
                Motion::FindChar {
                    ch: 'o',
                    forward: true,
                    till: false
                },
                8
            ),
            BufPos::new(1, 4)
        );
        assert_eq!(
            move_cursor(
                &b,
                BufPos::new(1, 0),
                Motion::FindChar {
                    ch: 'o',
                    forward: true,
                    till: true
                },
                8
            ),
            BufPos::new(1, 3)
        );
    }
}
