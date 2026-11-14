//! Minimal ANSI terminal backend: raw mode, input decoding and cell rendering.
//!
//! The backend knows nothing about Vim commands. It presents frames and emits
//! semantic [`UiEvent`] values.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use rvim_platform::term;
use rvim_ui_api::{
    Attr, AttrFlags, ClipboardKind, ClipboardRequest, Color, Frame, Key, KeyEvent, KeyKind, Mods,
    MouseButton, MouseEvent, MouseKind, QuitReason, Size, UiBackend, UiCapabilities, UiError,
    UiEvent,
};

pub struct TuiBackend {
    saved_state: Option<String>,
    raw: bool,
    rx: Receiver<Option<Vec<u8>>>,
    pending: Vec<u8>,
    queue: VecDeque<UiEvent>,
    caps: UiCapabilities,
    size: Size,
    title: String,
    initialized: bool,
    paste_mode: bool,
}

impl TuiBackend {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("rvim-tui-input".to_string())
            .spawn(move || {
                let mut stdin = io::stdin();
                let mut buffer = [0u8; 4096];
                loop {
                    match stdin.read(&mut buffer) {
                        Ok(0) => {
                            let _ = tx.send(None);
                            break;
                        }
                        Ok(n) => {
                            if tx.send(Some(buffer[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            let _ = tx.send(None);
                            break;
                        }
                    }
                }
            })
            .ok();
        Self {
            saved_state: None,
            raw: false,
            rx,
            pending: Vec::new(),
            queue: VecDeque::new(),
            caps: UiCapabilities {
                color: detect_color_depth(),
                unicode: true,
                mouse: rvim_ui_api::MouseCaps {
                    click: true,
                    drag: true,
                    motion: true,
                    wheel: true,
                    sgr: true,
                    urxvt: false,
                },
                bracket_paste: true,
                focus_events: true,
                kitty_keyboard: false,
                modify_other_keys: false,
                title: true,
                clipboard: rvim_ui_api::ClipboardCaps {
                    unnamed: true,
                    selection: true,
                    plus: true,
                },
                ime: true,
                pixels: None,
                images: false,
                sync_output: true,
                in_band_resize: false,
            },
            size: platform_size(),
            title: String::new(),
            initialized: false,
            paste_mode: false,
        }
    }

    fn write_stdout(&self, bytes: &[u8]) -> Result<(), UiError> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(bytes)?;
        stdout.flush()?;
        Ok(())
    }

    fn enter_terminal(&mut self) -> Result<(), UiError> {
        let saved = stty(&["-g"])?;
        self.saved_state = Some(saved.trim().to_string());
        stty(&["raw", "-echo"])?;
        self.raw = true;
        self.size = platform_size();
        if term::supports_ansi() {
            self.write_stdout(b"\x1b[?1049h\x1b[?25l\x1b[2J\x1b[H\x1b[?2004h\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[?1004h")?;
        }
        Ok(())
    }

    fn leave_terminal(&mut self) {
        if !self.raw {
            return;
        }
        if term::supports_ansi() {
            let _ = self.write_stdout(
                b"\x1b[?1004l\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[?2004l\x1b[?25h\x1b[?1049l",
            );
        }
        if let Some(saved) = &self.saved_state {
            let _ = stty(&[saved]);
        }
        self.raw = false;
    }

    fn poll_bytes(&mut self, timeout: Duration) -> Result<bool, UiError> {
        match self.rx.recv_timeout(timeout) {
            Ok(Some(bytes)) => {
                self.pending.extend_from_slice(&bytes);
                Ok(true)
            }
            Ok(None) => {
                self.queue.push_back(UiEvent::Quit(QuitReason::Eof));
                Ok(false)
            }
            Err(RecvTimeoutError::Timeout) => Ok(false),
            Err(RecvTimeoutError::Disconnected) => {
                self.queue.push_back(UiEvent::Quit(QuitReason::Eof));
                Ok(false)
            }
        }
    }

    fn decode_available(&mut self) {
        while let Some(event) = self.decode_one() {
            self.queue.push_back(event);
        }
    }

    fn decode_one(&mut self) -> Option<UiEvent> {
        if self.pending.is_empty() {
            return None;
        }
        if self.paste_mode {
            return self.decode_paste();
        }
        if self.pending[0] == 0x1b {
            return self.decode_escape();
        }
        let byte = self.pending[0];
        if byte < 0x20 || byte == 0x7f {
            self.pending.remove(0);
            return Some(control_key(byte));
        }
        let width = utf8_width(byte);
        if width == 0 {
            self.pending.remove(0);
            return Some(UiEvent::Text("\u{FFFD}".to_string()));
        }
        if self.pending.len() < width {
            return None;
        }
        let bytes: Vec<u8> = self.pending.drain(..width).collect();
        match std::str::from_utf8(&bytes) {
            Ok(text) => {
                let ch = text.chars().next()?;
                Some(UiEvent::Key(KeyEvent {
                    key: Key::Char(ch),
                    mods: Mods::empty(),
                    kind: KeyKind::Press,
                    text: Some(text.into()),
                }))
            }
            Err(_) => Some(UiEvent::Text("\u{FFFD}".to_string())),
        }
    }

    fn decode_escape(&mut self) -> Option<UiEvent> {
        if self.pending.len() == 1 {
            self.pending.remove(0);
            return Some(UiEvent::Key(KeyEvent::new(Key::Esc)));
        }
        if self.pending[1] == b'[' {
            let final_pos = self.pending[2..]
                .iter()
                .position(|b| (0x40..=0x7e).contains(b))?;
            let final_pos = final_pos + 2;
            let final_byte = self.pending[final_pos];
            let params = String::from_utf8_lossy(&self.pending[2..final_pos]).into_owned();
            if params == "200" && final_byte == b'~' {
                self.pending.drain(..final_pos + 1);
                self.paste_mode = true;
                return self.decode_paste();
            }
            if final_byte == b'M' || final_byte == b'm' {
                let seq: Vec<u8> = self.pending.drain(..final_pos + 1).collect();
                let body = String::from_utf8_lossy(&seq[2..seq.len() - 1]).into_owned();
                return parse_mouse(&body, final_byte == b'm');
            }
            self.pending.drain(..final_pos + 1);
            if final_byte == b'I' {
                return Some(UiEvent::Focus(true));
            }
            if final_byte == b'O' {
                return Some(UiEvent::Focus(false));
            }
            return Some(UiEvent::Key(csi_key(&params, final_byte)));
        }
        if self.pending[1] == b'O' {
            if self.pending.len() < 3 {
                return None;
            }
            let code = self.pending[2];
            self.pending.drain(..3);
            return Some(UiEvent::Key(ss3_key(code)));
        }
        // Alt+key (the ESC prefix is the only encoding most terminals provide).
        let second = self.pending[1];
        if second < 0x20 || second == 0x7f {
            self.pending.remove(0);
            return Some(UiEvent::Key(KeyEvent::new(Key::Esc)));
        }
        let width = utf8_width(second);
        if width == 0 || self.pending.len() < 1 + width {
            return None;
        }
        let bytes: Vec<u8> = self.pending.drain(..1 + width).skip(1).collect();
        match std::str::from_utf8(&bytes) {
            Ok(text) => {
                let ch = text.chars().next()?;
                Some(UiEvent::Key(KeyEvent::with_mods(Key::Char(ch), Mods::ALT)))
            }
            Err(_) => Some(UiEvent::Key(KeyEvent::new(Key::Esc))),
        }
    }

    fn decode_paste(&mut self) -> Option<UiEvent> {
        const END: &[u8] = b"\x1b[201~";
        if let Some(index) = find_subslice(&self.pending, END) {
            let text = String::from_utf8_lossy(&self.pending[..index]).into_owned();
            self.pending.drain(..index + END.len());
            self.paste_mode = false;
            return Some(UiEvent::Paste(text));
        }
        None
    }

    fn resize_if_changed(&mut self) -> Option<UiEvent> {
        let size = platform_size();
        if size != self.size {
            self.size = size;
            return Some(UiEvent::Resize(size));
        }
        None
    }
}

impl Default for TuiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl UiBackend for TuiBackend {
    fn capabilities(&self) -> UiCapabilities {
        self.caps
    }

    fn init(&mut self) -> Result<(), UiError> {
        if self.initialized {
            return Ok(());
        }
        if !term::is_tty() {
            return Err(UiError::new(
                "stdout is not a terminal; use --headless for scripts",
            ));
        }
        self.enter_terminal()?;
        self.initialized = true;
        Ok(())
    }

    fn present(&mut self, frame: &Frame) -> Result<(), UiError> {
        if !self.initialized {
            return Err(UiError::new("TUI backend is not initialized"));
        }
        self.size = frame.size;
        if !term::supports_ansi() {
            let composite = frame.composite();
            let mut out = String::new();
            for row in 0..frame.size.rows {
                out.push_str(&composite.row_text(row));
                out.push('\n');
            }
            self.write_stdout(out.as_bytes())?;
            return Ok(());
        }
        let mut out = String::new();
        if self.caps.sync_output {
            out.push_str("\x1b[?2026h");
        }
        out.push_str("\x1b[?25l");
        let composite = frame.composite();
        let grid = &composite;
        let mut current_attr: Option<Attr> = None;
        for row in 0..frame.size.rows {
            out.push_str(&format!("\x1b[{};1H", row + 1));
            for col in 0..frame.size.cols {
                let Some(cell) = grid.cell(row, col) else {
                    continue;
                };
                if cell.width == 0 {
                    continue;
                }
                let attr = grid.attr(cell);
                if current_attr != Some(attr) {
                    out.push_str(&sgr(attr, self.caps.color));
                    current_attr = Some(attr);
                }
                if cell.text.is_empty() {
                    out.push(' ');
                } else {
                    out.push_str(&cell.text);
                }
            }
            out.push_str("\x1b[K");
        }
        if let Some(cursor) = frame.cursor {
            out.push_str(&format!("\x1b[{};{}H", cursor.row + 1, cursor.col + 1));
            out.push_str(match cursor.shape {
                rvim_ui_api::CursorShape::Block => "\x1b[2 q",
                rvim_ui_api::CursorShape::Vertical { .. } => "\x1b[6 q",
                rvim_ui_api::CursorShape::Horizontal { .. } => "\x1b[4 q",
                rvim_ui_api::CursorShape::Underline { .. } => "\x1b[4 q",
            });
            out.push_str("\x1b[?25h");
        } else {
            out.push_str("\x1b[?25l");
        }
        if self.caps.sync_output {
            out.push_str("\x1b[?2026l");
        }
        self.write_stdout(out.as_bytes())?;
        if let Some(request) = &frame.clipboard {
            let _ = self.clipboard(request.clone());
        }
        if let Some(title) = &frame.title {
            if title != &self.title {
                self.set_title(title)?;
            }
        }
        Ok(())
    }

    fn poll_event(&mut self, timeout: Duration) -> Result<Option<UiEvent>, UiError> {
        if let Some(event) = self.queue.pop_front() {
            return Ok(Some(event));
        }
        self.decode_available();
        if let Some(event) = self.queue.pop_front() {
            return Ok(Some(event));
        }
        let start = Instant::now();
        let _ = self.poll_bytes(timeout)?;
        self.decode_available();
        if let Some(event) = self.queue.pop_front() {
            return Ok(Some(event));
        }
        if let Some(event) = self.resize_if_changed() {
            return Ok(Some(event));
        }
        if timeout > start.elapsed() {
            let remaining = timeout - start.elapsed();
            let _ = self.poll_bytes(remaining)?;
            self.decode_available();
        }
        Ok(self.queue.pop_front().or_else(|| self.resize_if_changed()))
    }

    fn resize(&mut self, size: Size) -> Result<(), UiError> {
        self.size = size;
        Ok(())
    }

    fn set_title(&mut self, title: &str) -> Result<(), UiError> {
        self.title = title.to_string();
        self.write_stdout(format!("\x1b]0;{}\x07", sanitize_title(title)).as_bytes())
    }

    fn clipboard(&mut self, request: ClipboardRequest) -> Result<(), UiError> {
        match request {
            ClipboardRequest::Set { kind, data, .. } => {
                let platform_kind = match kind {
                    ClipboardKind::Unnamed => rvim_platform::ClipboardKind::Unnamed,
                    ClipboardKind::Selection => rvim_platform::ClipboardKind::Selection,
                    ClipboardKind::Plus => rvim_platform::ClipboardKind::Plus,
                };
                rvim_platform::clipboard_set(platform_kind, &data).map_err(UiError::new)?;
            }
            ClipboardRequest::Get { .. } => {}
        }
        Ok(())
    }

    fn suspend(&mut self) -> Result<(), UiError> {
        self.leave_terminal();
        #[cfg(unix)]
        {
            use std::os::raw::c_int;
            #[cfg(target_os = "macos")]
            const SIGTSTP: c_int = 18;
            #[cfg(not(target_os = "macos"))]
            const SIGTSTP: c_int = 20;
            extern "C" {
                fn kill(pid: c_int, sig: c_int) -> c_int;
                fn getpid() -> c_int;
            }
            // SAFETY: getpid/kill are async-signal-safe libc calls.
            unsafe {
                kill(getpid(), SIGTSTP);
            }
        }
        Ok(())
    }

    fn resume(&mut self) -> Result<(), UiError> {
        self.enter_terminal()?;
        self.initialized = true;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), UiError> {
        self.leave_terminal();
        self.initialized = false;
        Ok(())
    }
}

impl Drop for TuiBackend {
    fn drop(&mut self) {
        self.leave_terminal();
    }
}

fn stty(args: &[&str]) -> Result<String, UiError> {
    let output = Command::new("stty")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| UiError::new(format!("stty: {error}")))?;
    if !output.status.success() {
        return Err(UiError::new(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn control_key(byte: u8) -> UiEvent {
    match byte {
        0x08 | 0x7f => UiEvent::Key(KeyEvent::new(Key::Backspace)),
        0x09 => UiEvent::Key(KeyEvent::new(Key::Tab)),
        0x0a | 0x0d => UiEvent::Key(KeyEvent::new(Key::Enter)),
        0x00 => UiEvent::Key(KeyEvent::ctrl('@')),
        value if value < 0x1b => {
            let ch = (b'a' + value - 1) as char;
            UiEvent::Key(KeyEvent::ctrl(ch))
        }
        0x1c => UiEvent::Key(KeyEvent::ctrl('\\')),
        0x1d => UiEvent::Key(KeyEvent::ctrl(']')),
        0x1e => UiEvent::Key(KeyEvent::ctrl('^')),
        0x1f => UiEvent::Key(KeyEvent::ctrl('_')),
        _ => UiEvent::Key(KeyEvent::new(Key::Esc)),
    }
}

fn csi_key(params: &str, final_byte: u8) -> KeyEvent {
    let key = match final_byte {
        b'A' => Key::Up,
        b'B' => Key::Down,
        b'C' => Key::Right,
        b'D' => Key::Left,
        b'H' => Key::Home,
        b'F' => Key::End,
        b'Z' => Key::Tab,
        b'~' => {
            let first = params.split(';').next().unwrap_or("");
            match first {
                "1" | "7" => Key::Home,
                "2" => Key::Insert,
                "3" => Key::Delete,
                "4" | "8" => Key::End,
                "5" => Key::PageUp,
                "6" => Key::PageDown,
                "11" => Key::F(1),
                "12" => Key::F(2),
                "13" => Key::F(3),
                "14" => Key::F(4),
                "15" => Key::F(5),
                "17" => Key::F(6),
                "18" => Key::F(7),
                "19" => Key::F(8),
                "20" => Key::F(9),
                "21" => Key::F(10),
                "23" => Key::F(11),
                "24" => Key::F(12),
                _ => Key::Esc,
            }
        }
        _ => Key::Esc,
    };
    let mut mods = Mods::empty();
    if let Some(modifier) = params.split(';').nth(1) {
        if let Ok(value) = modifier.parse::<u8>() {
            let value = value.saturating_sub(1);
            if value & 1 != 0 {
                mods.insert(Mods::SHIFT);
            }
            if value & 2 != 0 {
                mods.insert(Mods::ALT);
            }
            if value & 4 != 0 {
                mods.insert(Mods::CTRL);
            }
        }
    }
    if final_byte == b'Z' {
        mods.insert(Mods::SHIFT);
    }
    KeyEvent::with_mods(key, mods)
}

fn ss3_key(code: u8) -> KeyEvent {
    let key = match code {
        b'A' => Key::Up,
        b'B' => Key::Down,
        b'C' => Key::Right,
        b'D' => Key::Left,
        b'H' => Key::Home,
        b'F' => Key::End,
        b'P' => Key::F(1),
        b'Q' => Key::F(2),
        b'R' => Key::F(3),
        b'S' => Key::F(4),
        _ => Key::Esc,
    };
    KeyEvent::new(key)
}

fn parse_mouse(body: &str, release: bool) -> Option<UiEvent> {
    let body = body.trim_start_matches('<');
    let mut parts = body.split(';');
    let button = parts.next()?.parse::<u16>().ok()?;
    let x = parts.next()?.parse::<u16>().ok()?.saturating_sub(1);
    let y = parts.next()?.parse::<u16>().ok()?.saturating_sub(1);
    let mut mods = Mods::empty();
    if button & 4 != 0 {
        mods.insert(Mods::SHIFT);
    }
    if button & 8 != 0 {
        mods.insert(Mods::ALT);
    }
    if button & 16 != 0 {
        mods.insert(Mods::CTRL);
    }
    let base = button & 0b1100_0011;
    let (kind, button) = if release {
        (MouseKind::Release, MouseButton::None)
    } else {
        match base {
            0 => (MouseKind::Press, MouseButton::Left),
            1 => (MouseKind::Press, MouseButton::Middle),
            2 => (MouseKind::Press, MouseButton::Right),
            32 => (MouseKind::Drag, MouseButton::Left),
            64 => (MouseKind::Wheel, MouseButton::WheelUp),
            65 => (MouseKind::Wheel, MouseButton::WheelDown),
            3 => (MouseKind::Release, MouseButton::None),
            _ => return None,
        }
    };
    Some(UiEvent::Mouse(MouseEvent {
        row: y,
        col: x,
        kind,
        button,
        mods,
    }))
}

fn utf8_width(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first & 0xE0 == 0xC0 {
        2
    } else if first & 0xF0 == 0xE0 {
        3
    } else if first & 0xF8 == 0xF0 {
        4
    } else {
        0
    }
}

fn platform_size() -> Size {
    let size = term::terminal_size();
    Size::new(size.rows, size.cols)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn sanitize_title(title: &str) -> String {
    title.chars().filter(|ch| !ch.is_control()).collect()
}

fn detect_color_depth() -> rvim_ui_api::backend::ColorDepth {
    use rvim_ui_api::backend::ColorDepth;
    let caps = term::capabilities();
    if !caps.ansi {
        ColorDepth::Mono
    } else if caps.truecolor {
        ColorDepth::TrueColor
    } else if caps.colors >= 256 {
        ColorDepth::Ansi256
    } else {
        ColorDepth::Ansi16
    }
}

fn ansi16_code(index: u8, foreground: bool) -> u8 {
    let index = index % 16;
    let base = if index < 8 { 30 } else { 82 };
    let code = base + (index % 8);
    if foreground {
        code
    } else {
        code + 10
    }
}

fn rgb_to_16(r: u8, g: u8, b: u8) -> u8 {
    let bright = r.max(g).max(b) > 191;
    let mut index = 0u8;
    if r > 127 {
        index |= 1;
    }
    if g > 127 {
        index |= 2;
    }
    if b > 127 {
        index |= 4;
    }
    if bright {
        index | 8
    } else {
        index
    }
}

fn sgr(attr: Attr, depth: rvim_ui_api::backend::ColorDepth) -> String {
    let mut codes: Vec<String> = vec!["0".to_string()];
    if attr.flags.contains(AttrFlags::BOLD) {
        codes.push("1".into());
    }
    if attr.flags.contains(AttrFlags::DIM) {
        codes.push("2".into());
    }
    if attr.flags.contains(AttrFlags::ITALIC) {
        codes.push("3".into());
    }
    if attr.flags.contains(AttrFlags::UNDERLINE) {
        codes.push("4".into());
    }
    if attr.flags.contains(AttrFlags::BLINK) {
        codes.push("5".into());
    }
    if attr.flags.contains(AttrFlags::REVERSE) {
        codes.push("7".into());
    }
    if attr.flags.contains(AttrFlags::STRIKETHROUGH) {
        codes.push("9".into());
    }
    append_color(&mut codes, attr.fg, true, depth);
    append_color(&mut codes, attr.bg, false, depth);
    format!("\x1b[{}m", codes.join(";"))
}

fn append_color(
    codes: &mut Vec<String>,
    color: Color,
    foreground: bool,
    depth: rvim_ui_api::backend::ColorDepth,
) {
    match color {
        Color::Default => codes.push(if foreground { "39".into() } else { "49".into() }),
        Color::Indexed(index) => match depth {
            rvim_ui_api::backend::ColorDepth::Mono => {}
            rvim_ui_api::backend::ColorDepth::Ansi16 => {
                codes.push(ansi16_code(index, foreground).to_string())
            }
            _ => codes.push(format!("{};5;{}", if foreground { 38 } else { 48 }, index)),
        },
        Color::Rgb(r, g, b) => match depth {
            rvim_ui_api::backend::ColorDepth::Mono => {}
            rvim_ui_api::backend::ColorDepth::TrueColor => codes.push(format!(
                "{};2;{};{};{}",
                if foreground { 38 } else { 48 },
                r,
                g,
                b
            )),
            rvim_ui_api::backend::ColorDepth::Ansi256 => {
                let index = rgb_to_256(r, g, b);
                codes.push(format!("{};5;{}", if foreground { 38 } else { 48 }, index));
            }
            rvim_ui_api::backend::ColorDepth::Ansi16 => {
                codes.push(ansi16_code(rgb_to_16(r, g, b), foreground).to_string())
            }
        },
    }
}

fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let ri = nearest(&LEVELS, r);
    let gi = nearest(&LEVELS, g);
    let bi = nearest(&LEVELS, b);
    (16 + 36 * ri + 6 * gi + bi) as u8
}

fn nearest(levels: &[u8], value: u8) -> usize {
    let mut best = 0;
    let mut distance = u8::MAX;
    for (index, level) in levels.iter().enumerate() {
        let delta = level.abs_diff(value);
        if delta < distance {
            distance = delta;
            best = index;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_basic_keys() {
        let mut backend = TuiBackend::new();
        backend.pending.extend_from_slice(b"\x1b[A");
        let event = backend.decode_one().unwrap();
        assert!(matches!(event, UiEvent::Key(KeyEvent { key: Key::Up, .. })));
    }

    #[test]
    fn decodes_utf8() {
        let mut backend = TuiBackend::new();
        backend.pending.extend_from_slice("界".as_bytes());
        let event = backend.decode_one().unwrap();
        assert!(matches!(
            event,
            UiEvent::Key(KeyEvent {
                key: Key::Char('界'),
                ..
            })
        ));
    }

    #[test]
    fn parses_sgr_mouse() {
        let mut backend = TuiBackend::new();
        backend.pending.extend_from_slice(b"\x1b[<0;3;4M");
        let event = backend.decode_one().unwrap();
        match event {
            UiEvent::Mouse(mouse) => {
                assert_eq!(mouse.col, 2);
                assert_eq!(mouse.row, 3);
            }
            _ => panic!("expected mouse"),
        }
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;
    use rvim_ui_api::backend::ColorDepth;

    #[test]
    fn ansi16_color_fallback() {
        let attr = Attr {
            fg: Color::Indexed(1),
            ..Attr::DEFAULT
        };
        let code = sgr(attr, ColorDepth::Ansi16);
        assert!(code.contains("31"), "{code}");
        let mono = sgr(attr, ColorDepth::Mono);
        assert!(!mono.contains("38;"), "{mono}");
    }
}
