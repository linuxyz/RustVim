use crate::clipboard::ClipboardResult;
use crate::geometry::Size;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    pub const SHIFT: Self = Self(1 << 0);
    pub const CTRL: Self = Self(1 << 1);
    pub const ALT: Self = Self(1 << 2);
    pub const SUPER: Self = Self(1 << 3);
    pub const HYPER: Self = Self(1 << 4);
    pub const META: Self = Self(1 << 5);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for Mods {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Mods {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    F(u8),
    Enter,
    Esc,
    Tab,
    Backspace,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    Mouse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyKind {
    #[default]
    Press,
    Repeat,
    Release,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub mods: Mods,
    pub kind: KeyKind,
    pub text: Option<Box<str>>,
}

impl KeyEvent {
    pub fn new(key: Key) -> Self {
        Self {
            key,
            mods: Mods::empty(),
            kind: KeyKind::Press,
            text: None,
        }
    }

    pub fn with_mods(key: Key, mods: Mods) -> Self {
        Self {
            key,
            mods,
            kind: KeyKind::Press,
            text: None,
        }
    }

    pub fn ctrl(ch: char) -> Self {
        Self::with_mods(Key::Char(ch), Mods::CTRL)
    }

    pub fn alt(ch: char) -> Self {
        Self::with_mods(Key::Char(ch), Mods::ALT)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Press,
    Release,
    Drag,
    Move,
    Wheel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub row: u16,
    pub col: u16,
    pub kind: MouseKind,
    pub button: MouseButton,
    pub mods: Mods,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeEvent {
    Preedit(String),
    Commit(String),
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TermResponse {
    DeviceAttributes(String),
    BackgroundColor(Option<(u8, u8, u8)>),
    KeyboardFlags(String),
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuitReason {
    Requested,
    Eof,
    Signal(i32),
    Fatal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    Key(KeyEvent),
    Text(String),
    Paste(String),
    Mouse(MouseEvent),
    Resize(Size),
    Focus(bool),
    TermResponse(TermResponse),
    Clipboard(ClipboardResult),
    Ime(ImeEvent),
    Wake,
    Quit(QuitReason),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseCaps {
    pub click: bool,
    pub drag: bool,
    pub motion: bool,
    pub wheel: bool,
    pub sgr: bool,
    pub urxvt: bool,
}
