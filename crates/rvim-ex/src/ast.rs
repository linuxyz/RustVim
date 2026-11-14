use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddressKind {
    Current,
    Last,
    Line(u32),
    Mark(char),
    /// A search address. Direction is 1 for / and -1 for ?.
    Search {
        pattern: String,
        direction: i8,
    },
    /// A whole-file range written as %.
    WholeFile,
    /// No address was given. The command decides its default.
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub kind: AddressKind,
    /// Vim offset: +N, -N, +/pat/, etc.
    pub offset: i32,
    pub offset_pattern: Option<(String, i8)>,
}

impl Address {
    pub fn new(kind: AddressKind) -> Self {
        Self {
            kind,
            offset: 0,
            offset_pattern: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeSep {
    Comma,
    Semicolon,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RangeSpec {
    pub addresses: Vec<Address>,
    pub separator: Option<RangeSep>,
}

impl RangeSpec {
    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }

    pub fn is_whole_file(&self) -> bool {
        self.addresses
            .iter()
            .any(|a| a.kind == AddressKind::WholeFile)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modifier {
    Silent,
    SilentBang,
    Verbose(Option<u32>),
    NoAutocmd,
    KeepJumps,
    KeepMarks,
    KeepAlt,
    LockMarks,
    Sandbox,
    Confirm,
    Hide,
    Browse,
    Unsilent,
    Vertical,
    Horizontal,
    AboveLeft,
    BelowRight,
    LeftAbove,
    RightBelow,
    Tab,
    Top,
    BotRight,
    Topleft,
    Filter(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExCommand {
    pub range: RangeSpec,
    pub name: String,
    pub bang: bool,
    pub args: String,
    pub modifiers: Vec<Modifier>,
    pub filter: Option<String>,
    pub raw: String,
}

impl ExCommand {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            range: RangeSpec::default(),
            name: name.into(),
            bang: false,
            args: String::new(),
            modifiers: Vec::new(),
            filter: None,
            raw: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedRange {
    pub start: u32,
    pub end: u32,
}

impl ResolvedRange {
    pub fn new(start: u32, end: u32) -> Self {
        Self {
            start: start.min(end),
            end: start.max(end),
        }
    }

    pub fn line_count(self) -> u32 {
        self.end.saturating_sub(self.start) + 1
    }
}

/// Host operations needed to resolve addresses. The editor implements this.
pub trait AddressResolver {
    fn current_line(&self) -> u32;
    fn last_line(&self) -> u32;
    fn line_count(&self) -> u32;
    fn mark_line(&self, mark: char) -> Option<u32>;
    fn search_line(&self, pattern: &str, direction: i8, from: u32) -> Option<u32>;
}

impl fmt::Display for ExCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.range.addresses.is_empty() {
            write!(f, "{}", format_range(&self.range))?;
        }
        write!(f, "{}", self.name)?;
        if self.bang {
            f.write_str("!")?;
        }
        if !self.args.is_empty() {
            write!(f, " {}", self.args)?;
        }
        Ok(())
    }
}

pub fn format_range(range: &RangeSpec) -> String {
    let mut out = String::new();
    for (index, address) in range.addresses.iter().enumerate() {
        if index > 0 {
            out.push(match range.separator.unwrap_or(RangeSep::Comma) {
                RangeSep::Comma => ',',
                RangeSep::Semicolon => ';',
            });
        }
        match &address.kind {
            AddressKind::Current => out.push('.'),
            AddressKind::Last => out.push('$'),
            AddressKind::Line(line) => out.push_str(&line.to_string()),
            AddressKind::Mark(mark) => {
                out.push('\'');
                out.push(*mark);
            }
            AddressKind::Search { pattern, direction } => {
                let delim = if *direction >= 0 { '/' } else { '?' };
                out.push(delim);
                out.push_str(pattern);
                out.push(delim);
            }
            AddressKind::WholeFile => out.push('%'),
            AddressKind::None => {}
        }
        if address.offset > 0 {
            out.push('+');
            out.push_str(&address.offset.to_string());
        } else if address.offset < 0 {
            out.push_str(&address.offset.to_string());
        }
    }
    out
}
