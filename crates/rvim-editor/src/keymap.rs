use std::time::{Duration, Instant};

use rvim_types::BufferId;
use rvim_ui_api::{Key, KeyEvent, KeyKind, Mods};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapMode {
    Normal,
    Insert,
    Visual,
    Operator,
    Command,
}

#[derive(Clone, Debug)]
pub struct Mapping {
    pub lhs: Vec<KeyEvent>,
    pub rhs: Vec<KeyEvent>,
    pub noremap: bool,
    pub nowait: bool,
    pub silent: bool,
    pub buffer: Option<BufferId>,
    pub expr: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct MappingOptions {
    pub nowait: bool,
    pub silent: bool,
    pub buffer: Option<BufferId>,
    pub unique: bool,
    pub expr: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct MappingState {
    normal: Vec<Mapping>,
    insert: Vec<Mapping>,
    visual: Vec<Mapping>,
    operator: Vec<Mapping>,
    command: Vec<Mapping>,
    pending: Vec<KeyEvent>,
    pending_since: Option<Instant>,
    timeout: Duration,
    timeout_enabled: bool,
    active_buffer: Option<BufferId>,
}

#[derive(Clone, Debug, Default)]
pub struct MappingFeed {
    pub keys: Vec<KeyEvent>,
    pub pending: bool,
    pub silent: bool,
    pub expr: Option<String>,
}

impl MappingState {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_millis(1000),
            timeout_enabled: true,
            ..Self::default()
        }
    }

    pub fn set_timeout(&mut self, millis: u64) {
        self.timeout = Duration::from_millis(millis.max(1));
    }

    pub fn set_timeout_enabled(&mut self, enabled: bool) {
        self.timeout_enabled = enabled;
    }

    pub fn add(
        &mut self,
        mode: MapMode,
        lhs: Vec<KeyEvent>,
        rhs: Vec<KeyEvent>,
        noremap: bool,
        options: MappingOptions,
    ) -> bool {
        if lhs.is_empty() {
            return false;
        }
        let map = Mapping {
            lhs,
            rhs,
            noremap,
            nowait: options.nowait,
            silent: options.silent,
            buffer: options.buffer,
            expr: options.expr,
        };
        let list = self.list_mut(mode);
        if let Some(existing) = list
            .iter_mut()
            .find(|existing| same_keys(&existing.lhs, &map.lhs) && existing.buffer == map.buffer)
        {
            if options.unique {
                return false;
            }
            *existing = map;
        } else {
            list.push(map);
        }
        true
    }

    pub fn remove(&mut self, mode: MapMode, lhs: &[KeyEvent]) {
        self.list_mut(mode)
            .retain(|mapping| !same_keys(&mapping.lhs, lhs));
    }

    pub fn clear(&mut self, mode: MapMode) {
        self.list_mut(mode).clear();
    }

    pub fn list(&self, mode: MapMode) -> &[Mapping] {
        match mode {
            MapMode::Normal => &self.normal,
            MapMode::Insert => &self.insert,
            MapMode::Visual => &self.visual,
            MapMode::Operator => &self.operator,
            MapMode::Command => &self.command,
        }
    }

    pub fn entries(&self) -> Vec<(MapMode, String)> {
        let mut out = Vec::new();
        for (mode, list) in [
            (MapMode::Normal, &self.normal),
            (MapMode::Insert, &self.insert),
            (MapMode::Visual, &self.visual),
            (MapMode::Operator, &self.operator),
            (MapMode::Command, &self.command),
        ] {
            for mapping in list {
                out.push((mode, format_keys(&mapping.lhs)));
            }
        }
        out
    }

    pub fn feed(&mut self, mode: MapMode, key: KeyEvent, buffer: BufferId) -> MappingFeed {
        self.active_buffer = Some(buffer);
        self.pending.push(key);
        self.pending_since = Some(Instant::now());
        self.resolve(mode, 10)
    }

    pub fn flush_timeout(&mut self, mode: MapMode, now: Instant) -> Option<MappingFeed> {
        if !self.timeout_enabled {
            return None;
        }
        let since = self.pending_since?;
        if self.pending.is_empty() || now.duration_since(since) < self.timeout {
            return None;
        }
        let keys = std::mem::take(&mut self.pending);
        self.pending_since = None;
        let _ = mode;
        Some(MappingFeed {
            keys,
            pending: false,
            silent: false,
            expr: None,
        })
    }

    pub fn flush(&mut self) -> Vec<KeyEvent> {
        self.pending_since = None;
        std::mem::take(&mut self.pending)
    }

    pub fn deadline(&self) -> Option<Instant> {
        if !self.timeout_enabled {
            return None;
        }
        self.pending_since.map(|since| since + self.timeout)
    }

    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn visible(&self, mode: MapMode) -> Vec<Mapping> {
        self.list(mode)
            .iter()
            .filter(|mapping| mapping.buffer.is_none() || mapping.buffer == self.active_buffer)
            .cloned()
            .collect()
    }

    fn resolve(&mut self, mode: MapMode, depth: usize) -> MappingFeed {
        if depth == 0 {
            let keys = std::mem::take(&mut self.pending);
            return MappingFeed {
                keys,
                pending: false,
                silent: false,
                expr: None,
            };
        }
        let mappings = self.visible(mode);
        let exact = mappings
            .iter()
            .find(|mapping| same_keys(&mapping.lhs, &self.pending))
            .cloned();
        let has_longer = mappings.iter().any(|mapping| {
            mapping.lhs.len() > self.pending.len() && prefix(&mapping.lhs, &self.pending)
        });
        if let Some(mapping) = exact {
            if mapping.nowait || !has_longer {
                self.pending.clear();
                self.pending_since = None;
                if let Some(expression) = mapping.expr {
                    return MappingFeed {
                        keys: Vec::new(),
                        pending: false,
                        silent: mapping.silent,
                        expr: Some(expression),
                    };
                }
                if mapping.noremap {
                    return MappingFeed {
                        keys: mapping.rhs,
                        pending: false,
                        silent: mapping.silent,
                        expr: None,
                    };
                }
                let mut out = Vec::new();
                let mut state = MappingState {
                    timeout: self.timeout,
                    timeout_enabled: self.timeout_enabled,
                    ..MappingState::default()
                };
                state.active_buffer = self.active_buffer;
                let mut silent = mapping.silent;
                for key in mapping.rhs {
                    let feed = state.feed(mode, key, self.active_buffer.unwrap_or_default());
                    silent |= feed.silent;
                    out.extend(feed.keys);
                    if let Some(expression) = feed.expr {
                        return MappingFeed {
                            keys: Vec::new(),
                            pending: false,
                            silent,
                            expr: Some(expression),
                        };
                    }
                }
                out.extend(state.flush());
                return MappingFeed {
                    keys: out,
                    pending: false,
                    silent,
                    expr: None,
                };
            }
            return MappingFeed {
                keys: Vec::new(),
                pending: true,
                silent: false,
                expr: None,
            };
        }
        if has_longer {
            return MappingFeed {
                keys: Vec::new(),
                pending: true,
                silent: false,
                expr: None,
            };
        }
        let feed = MappingFeed {
            keys: std::mem::take(&mut self.pending),
            pending: false,
            silent: false,
            expr: None,
        };
        self.pending_since = None;
        feed
    }

    fn list_mut(&mut self, mode: MapMode) -> &mut Vec<Mapping> {
        match mode {
            MapMode::Normal => &mut self.normal,
            MapMode::Insert => &mut self.insert,
            MapMode::Visual => &mut self.visual,
            MapMode::Operator => &mut self.operator,
            MapMode::Command => &mut self.command,
        }
    }
}

fn same_keys(a: &[KeyEvent], b: &[KeyEvent]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.key == b.key && a.mods == b.mods)
}

fn prefix(full: &[KeyEvent], prefix: &[KeyEvent]) -> bool {
    full.len() >= prefix.len()
        && full
            .iter()
            .zip(prefix)
            .all(|(a, b)| a.key == b.key && a.mods == b.mods)
}

pub fn parse_keys(input: &str) -> Vec<KeyEvent> {
    let mut events = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '<' {
            events.push(KeyEvent::new(Key::Char(ch)));
            continue;
        }
        let mut token = String::new();
        let mut closed = false;
        for next in chars.by_ref() {
            if next == '>' {
                closed = true;
                break;
            }
            token.push(next);
        }
        if !closed {
            events.push(KeyEvent::new(Key::Char('<')));
            for ch in token.chars() {
                events.push(KeyEvent::new(Key::Char(ch)));
            }
            continue;
        }
        if let Some(event) = parse_key_token(&token) {
            events.push(event);
        } else {
            events.push(KeyEvent::new(Key::Char('<')));
            for ch in token.chars() {
                events.push(KeyEvent::new(Key::Char(ch)));
            }
            events.push(KeyEvent::new(Key::Char('>')));
        }
    }
    events
}

pub fn parse_key_token(token: &str) -> Option<KeyEvent> {
    if token.is_empty() {
        return None;
    }
    if token.len() == 1 {
        return Some(KeyEvent::new(Key::Char(token.chars().next()?)));
    }
    let lower = token.to_ascii_lowercase();
    let key = match lower.as_str() {
        "esc" => Some(Key::Esc),
        "cr" | "enter" | "return" => Some(Key::Enter),
        "tab" => Some(Key::Tab),
        "bs" | "backspace" => Some(Key::Backspace),
        "del" | "delete" => Some(Key::Delete),
        "ins" | "insert" => Some(Key::Insert),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pageup" | "pgup" => Some(Key::PageUp),
        "pagedown" | "pgdn" => Some(Key::PageDown),
        "up" => Some(Key::Up),
        "down" => Some(Key::Down),
        "left" => Some(Key::Left),
        "right" => Some(Key::Right),
        "space" => Some(Key::Char(' ')),
        "lt" => Some(Key::Char('<')),
        "gt" => Some(Key::Char('>')),
        "bar" => Some(Key::Char('|')),
        "leader" => Some(Key::Char('\\')),
        "nop" => None,
        _ if lower.starts_with('f') && lower[1..].parse::<u8>().is_ok() => {
            Some(Key::F(lower[1..].parse().ok()?))
        }
        _ if lower.starts_with("c-") => {
            let ch = token[2..].chars().next()?;
            let ch = match ch {
                '[' => '[',
                ']' => ']',
                '\\' => '\\',
                '^' => '^',
                '_' => '_',
                '@' => '@',
                _ => ch.to_ascii_lowercase(),
            };
            return Some(KeyEvent::with_mods(Key::Char(ch), Mods::CTRL));
        }
        _ if lower.starts_with("m-") => {
            let ch = token[2..].chars().next()?;
            return Some(KeyEvent::with_mods(Key::Char(ch), Mods::ALT));
        }
        _ if lower.starts_with("s-") => {
            let ch = token[2..].chars().next()?;
            return Some(KeyEvent::with_mods(Key::Char(ch), Mods::SHIFT));
        }
        _ => None,
    };
    key.map(KeyEvent::new)
}

pub fn format_keys(keys: &[KeyEvent]) -> String {
    let mut out = String::new();
    for key in keys {
        out.push_str(&format_key(key));
    }
    out
}

pub fn format_key(key: &KeyEvent) -> String {
    if key.mods.contains(Mods::CTRL) {
        if let Key::Char(ch) = key.key {
            return format!("<C-{ch}>");
        }
    }
    if key.mods.contains(Mods::ALT) {
        if let Key::Char(ch) = key.key {
            return format!("<M-{ch}>");
        }
    }
    match key.key {
        Key::Char('<') => "<lt>".to_string(),
        Key::Char(' ') => "<Space>".to_string(),
        Key::Char(ch) => ch.to_string(),
        Key::Esc => "<Esc>".to_string(),
        Key::Enter => "<CR>".to_string(),
        Key::Tab => "<Tab>".to_string(),
        Key::Backspace => "<BS>".to_string(),
        Key::Delete => "<Del>".to_string(),
        Key::Insert => "<Ins>".to_string(),
        Key::Home => "<Home>".to_string(),
        Key::End => "<End>".to_string(),
        Key::PageUp => "<PageUp>".to_string(),
        Key::PageDown => "<PageDown>".to_string(),
        Key::Up => "<Up>".to_string(),
        Key::Down => "<Down>".to_string(),
        Key::Left => "<Left>".to_string(),
        Key::Right => "<Right>".to_string(),
        Key::F(n) => format!("<F{n}>"),
        Key::Mouse => "<Mouse>".to_string(),
    }
}

pub fn key_press(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::empty(),
        kind: KeyKind::Press,
        text: None,
    }
}
