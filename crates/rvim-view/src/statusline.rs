use rvim_types::unicode::char_width;
use rvim_ui_api::Attr;

#[derive(Clone, Debug)]
pub struct StatuslineOptions {
    pub mode: String,
    pub width: u16,
    pub height: u16,
    pub attrs: Attr,
}

/// One `%[FuncName]` clickable span, in display cells relative to the
/// expanded statusline text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatuslineClick {
    pub start: u16,
    pub end: u16,
    pub function: String,
    pub minwid: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatuslineLayout {
    pub text: String,
    pub clicks: Vec<StatuslineClick>,
}

struct PendingClick {
    side_right: bool,
    start: u16,
    function: String,
    minwid: u32,
}

struct RawClick {
    side_right: bool,
    start: u16,
    end: u16,
    function: String,
    minwid: u32,
}

pub fn format_default_statusline(
    name: &str,
    modified: bool,
    line: u32,
    col: u32,
    total: u32,
    mode: &str,
    width: u16,
) -> String {
    let percent = if total <= 1 {
        100
    } else {
        (((line.saturating_sub(1)) as u64 * 100) / (total.saturating_sub(1) as u64)) as u32
    };
    let mut left = format!("{name}{}", if modified { " [+]" } else { "" });
    if left.is_empty() {
        left.push_str("[No Name]");
    }
    let right = format!("{line},{col} {percent}% {mode}");
    let width = width as usize;
    if width == 0 {
        return String::new();
    }
    if text_width(&left) as usize + (text_width(&right) as usize) < width {
        let pad = width - text_width(&left) as usize - text_width(&right) as usize;
        format!("{left}{}{right}", " ".repeat(pad))
    } else if text_width(&right) as usize + 1 < width {
        let keep = width.saturating_sub(text_width(&right) as usize + 1);
        format!("{} {right}", truncate_width(&left, keep))
    } else {
        truncate_width(&right, width)
    }
}

fn text_width(text: &str) -> u16 {
    let mut width = 0usize;
    for ch in text.chars() {
        width = width.saturating_add(if ch == '\n' { 0 } else { char_width(ch).max(1) });
    }
    width.min(u16::MAX as usize) as u16
}

fn append(out: &mut String, col: &mut u16, text: &str) {
    out.push_str(text);
    *col = col.saturating_add(text_width(text));
}

fn truncate_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let cells = if ch == '\n' { 0 } else { char_width(ch).max(1) };
        if used + cells > width {
            break;
        }
        used += cells;
        out.push(ch);
    }
    out
}

fn close_pending(
    pending: &mut Option<PendingClick>,
    left_col: u16,
    right_col: u16,
    raw: &mut Vec<RawClick>,
) {
    if let Some(click) = pending.take() {
        let end = if click.side_right {
            right_col
        } else {
            left_col
        };
        if end > click.start {
            raw.push(RawClick {
                side_right: click.side_right,
                start: click.start,
                end,
                function: click.function,
                minwid: click.minwid,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn expand_statusline_layout(
    template: &str,
    name: &str,
    full_name: &str,
    modified: bool,
    readonly: bool,
    filetype: &str,
    line: u32,
    col: u32,
    total: u32,
    mode: &str,
    width: u16,
) -> StatuslineLayout {
    let percent = if total <= 1 {
        100
    } else {
        ((line.saturating_sub(1) as u64 * 100) / total.saturating_sub(1) as u64) as u32
    };
    let mut left = String::new();
    let mut right = String::new();
    let mut left_col = 0u16;
    let mut right_col = 0u16;
    let mut right_side = false;
    let mut pending: Option<PendingClick> = None;
    let mut raw: Vec<RawClick> = Vec::new();

    macro_rules! put {
        ($text:expr) => {{
            let text: &str = $text;
            if right_side {
                append(&mut right, &mut right_col, text);
            } else {
                append(&mut left, &mut left_col, text);
            }
        }};
    }

    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            let mut buf = [0u8; 4];
            put!(ch.encode_utf8(&mut buf));
            continue;
        }
        let mut modifier = String::new();
        while let Some(next) = chars.peek() {
            if next.is_ascii_digit() || matches!(next, '-' | '.' | '+') {
                modifier.push(*next);
                chars.next();
            } else {
                break;
            }
        }
        let minwid = if !modifier.is_empty() && modifier.chars().all(|ch| ch.is_ascii_digit()) {
            modifier.parse::<u32>().unwrap_or(0)
        } else {
            0
        };
        match chars.next() {
            Some('%') => put!("%"),
            Some('f') | Some('t') => put!(name),
            Some('F') => put!(full_name),
            Some('m') | Some('M') => {
                if modified {
                    put!("[+]");
                }
            }
            Some('r') | Some('R') => {
                if readonly {
                    put!("[RO]");
                }
            }
            Some('h') | Some('H') => {
                if filetype == "help" {
                    put!("[Help]");
                }
            }
            Some('w') | Some('W') => {}
            Some('y') | Some('Y') => put!(filetype),
            Some('l') => put!(&line.to_string()),
            Some('L') => put!(&total.to_string()),
            Some('c') | Some('C') | Some('v') | Some('V') => put!(&col.to_string()),
            Some('o') | Some('O') => put!(&col.to_string()),
            Some('a') => put!(mode),
            Some('n') | Some('N') | Some('b') => {}
            Some('p') | Some('P') => put!(&percent.to_string()),
            Some('=') => {
                close_pending(&mut pending, left_col, right_col, &mut raw);
                right_side = true;
            }
            Some('<') | Some('*') => {}
            Some('@') => put!("\n"),
            Some('[') => {
                let mut function = String::new();
                let mut terminated = false;
                for next in chars.by_ref() {
                    if next == ']' {
                        terminated = true;
                        break;
                    }
                    function.push(next);
                }
                if terminated {
                    close_pending(&mut pending, left_col, right_col, &mut raw);
                    if !function.is_empty() {
                        let start = if right_side { right_col } else { left_col };
                        pending = Some(PendingClick {
                            side_right: right_side,
                            start,
                            function,
                            minwid,
                        });
                    }
                }
            }
            Some('{') => {
                for next in chars.by_ref() {
                    if next == '}' {
                        break;
                    }
                }
                put!("?");
            }
            Some('#') => {
                for next in chars.by_ref() {
                    if next == '#' {
                        break;
                    }
                }
            }
            Some(other) => {
                let mut buf = [0u8; 4];
                put!(other.encode_utf8(&mut buf));
            }
            None => put!("%"),
        }
    }
    close_pending(&mut pending, left_col, right_col, &mut raw);

    let width = width as usize;
    if width == 0 {
        return StatuslineLayout {
            text: String::new(),
            clicks: Vec::new(),
        };
    }
    let (text, right_offset) = if right.is_empty() {
        (truncate_width(&left, width), 0u16)
    } else if left_col as usize + (right_col as usize) < width {
        let pad = width - left_col as usize - right_col as usize;
        let mut text = left;
        text.push_str(&" ".repeat(pad));
        text.push_str(&right);
        (text, left_col.saturating_add(pad as u16))
    } else {
        let text = format!("{left} {right}");
        (truncate_width(&text, width), left_col.saturating_add(1))
    };
    let clicks = raw
        .into_iter()
        .filter_map(|click| {
            let offset = if click.side_right { right_offset } else { 0 };
            let start = click.start.saturating_add(offset);
            let end = click.end.saturating_add(offset).min(width as u16);
            (start < end).then_some(StatuslineClick {
                start,
                end,
                function: click.function,
                minwid: click.minwid,
            })
        })
        .collect();
    StatuslineLayout { text, clicks }
}

#[allow(clippy::too_many_arguments)]
pub fn expand_statusline(
    template: &str,
    name: &str,
    full_name: &str,
    modified: bool,
    readonly: bool,
    filetype: &str,
    line: u32,
    col: u32,
    total: u32,
    mode: &str,
    width: u16,
) -> String {
    expand_statusline_layout(
        template, name, full_name, modified, readonly, filetype, line, col, total, mode, width,
    )
    .text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(template: &str, width: u16) -> StatuslineLayout {
        expand_statusline_layout(
            template, "abc", "/tmp/abc", false, false, "rust", 1, 1, 10, "NORMAL", width,
        )
    }

    #[test]
    fn click_marker_wraps_statusline_text() {
        let layout = layout("%[Click]%f%[]", 10);
        assert_eq!(layout.text, "abc");
        assert_eq!(layout.clicks.len(), 1);
        assert_eq!(layout.clicks[0].start, 0);
        assert_eq!(layout.clicks[0].end, 3);
        assert_eq!(layout.clicks[0].function, "Click");
    }

    #[test]
    fn click_minwid_and_right_side_offset() {
        let layout = layout("%3[Left]%f%[]%=%[Right]%l", 20);
        assert_eq!(layout.text, "abc                1");
        assert_eq!(layout.clicks.len(), 2);
        assert_eq!(layout.clicks[0].minwid, 3);
        assert_eq!((layout.clicks[0].start, layout.clicks[0].end), (0, 3));
        assert_eq!(layout.clicks[1].function, "Right");
        assert_eq!((layout.clicks[1].start, layout.clicks[1].end), (19, 20));
    }

    #[test]
    fn click_regions_clip_at_width() {
        let layout = layout("%[Click]%f%[]", 2);
        assert_eq!(layout.text, "ab");
        assert_eq!((layout.clicks[0].start, layout.clicks[0].end), (0, 2));
    }

    #[test]
    fn adjacent_click_regions_close_implicitly() {
        let layout = layout("%[A]x%[B]y", 10);
        assert_eq!(layout.text, "xy");
        assert_eq!(layout.clicks.len(), 2);
        assert_eq!((layout.clicks[0].start, layout.clicks[0].end), (0, 1));
        assert_eq!((layout.clicks[1].start, layout.clicks[1].end), (1, 2));
    }

    #[test]
    fn click_width_uses_display_cells() {
        let layout = expand_statusline_layout(
            "%[Click]%f%[]",
            "\u{4f60}",
            "\u{4f60}",
            false,
            false,
            "",
            1,
            1,
            1,
            "NORMAL",
            10,
        );
        assert_eq!(layout.text, "\u{4f60}");
        assert_eq!((layout.clicks[0].start, layout.clicks[0].end), (0, 2));
    }
}
