use rvim_types::VimString;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterKind {
    Characterwise,
    Linewise,
    Blockwise,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterContent {
    pub lines: Vec<VimString>,
    pub kind: RegisterKind,
    /// True when the register was written by a delete/yank and can be shifted
    /// into numbered registers.
    pub from_operation: bool,
}

impl RegisterContent {
    pub fn new(lines: Vec<VimString>, kind: RegisterKind) -> Self {
        Self {
            lines,
            kind,
            from_operation: true,
        }
    }

    pub fn text(&self) -> VimString {
        let mut out = VimString::new();
        for (index, line) in self.lines.iter().enumerate() {
            if index > 0 {
                out.push(b'\n');
            }
            out.extend_from_slice(line.as_bytes());
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterName {
    Unnamed,
    Numbered(u8),
    Small(char),
    Big(char),
    LastInsert,
    LastCommand,
    LastSearch,
    BlackHole,
    Expression,
    Clipboard,
    Selection,
    File,
}

impl RegisterName {
    pub fn parse(ch: char) -> Option<Self> {
        match ch {
            '"' => Some(Self::Unnamed),
            '0'..='9' => Some(Self::Numbered(ch as u8 - b'0')),
            'a'..='z' => Some(Self::Small(ch)),
            'A'..='Z' => Some(Self::Big(ch)),
            '-' => Some(Self::LastInsert),
            ':' => Some(Self::LastCommand),
            '/' => Some(Self::LastSearch),
            '_' => Some(Self::BlackHole),
            '=' => Some(Self::Expression),
            '+' => Some(Self::Clipboard),
            '*' => Some(Self::Selection),
            '%' => Some(Self::File),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Registers {
    pub unnamed: Option<RegisterContent>,
    numbered: [Option<RegisterContent>; 10],
    small: [Option<RegisterContent>; 26],
    last_insert: Option<RegisterContent>,
    last_command: Option<RegisterContent>,
    last_search: Option<RegisterContent>,
}

impl Registers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: RegisterName) -> Option<&RegisterContent> {
        match name {
            RegisterName::Unnamed => self.unnamed.as_ref(),
            RegisterName::Numbered(n) => self.numbered.get(n as usize).and_then(|r| r.as_ref()),
            RegisterName::Small(ch) => self.small.get(index(ch)).and_then(|r| r.as_ref()),
            RegisterName::Big(ch) => self
                .small
                .get(index(ch.to_ascii_lowercase()))
                .and_then(|r| r.as_ref()),
            RegisterName::LastInsert => self.last_insert.as_ref(),
            RegisterName::LastCommand => self.last_command.as_ref(),
            RegisterName::LastSearch => self.last_search.as_ref(),
            RegisterName::BlackHole | RegisterName::Expression | RegisterName::File => None,
            RegisterName::Clipboard | RegisterName::Selection => self.unnamed.as_ref(),
        }
    }

    /// Registers that Vim persists in a viminfo file, newest data last.
    pub fn savable(&self) -> Vec<(RegisterName, &RegisterContent)> {
        let mut out = Vec::new();
        let mut push = |name: RegisterName| {
            if let Some(content) = self.get(name) {
                out.push((name, content));
            }
        };
        push(RegisterName::Unnamed);
        for number in 0..=9 {
            push(RegisterName::Numbered(number));
        }
        for letter in 'a'..='z' {
            push(RegisterName::Small(letter));
        }
        push(RegisterName::LastInsert);
        push(RegisterName::LastCommand);
        push(RegisterName::LastSearch);
        out
    }

    pub fn set(&mut self, name: RegisterName, mut content: RegisterContent) {
        match name {
            RegisterName::Unnamed => {
                self.unnamed = Some(content);
            }
            RegisterName::Numbered(n) => {
                let n = (n as usize).min(9);
                if n > 0 {
                    for i in (1..n).rev() {
                        self.numbered[i + 1] = self.numbered[i].take();
                    }
                }
                self.numbered[n] = Some(content);
            }
            RegisterName::Small(ch) => {
                self.small[index(ch)] = Some(content);
            }
            RegisterName::Big(ch) => {
                let idx = index(ch.to_ascii_lowercase());
                if let Some(existing) = &mut self.small[idx] {
                    existing.lines.extend(content.lines);
                } else {
                    self.small[idx] = Some(content);
                }
            }
            RegisterName::LastInsert => self.last_insert = Some(content),
            RegisterName::LastCommand => self.last_command = Some(content),
            RegisterName::LastSearch => self.last_search = Some(content),
            RegisterName::BlackHole | RegisterName::Expression | RegisterName::File => {
                content.from_operation = true;
            }
            RegisterName::Clipboard | RegisterName::Selection => {
                self.unnamed = Some(content);
            }
        }
        if !matches!(
            name,
            RegisterName::BlackHole | RegisterName::Expression | RegisterName::File
        ) {
            let mut unnamed = self.get(name).cloned();
            if let Some(content) = unnamed.as_mut() {
                content.from_operation = true;
                self.unnamed = Some(content.clone());
            }
        }
    }

    pub fn entries(&self) -> Vec<(char, &RegisterContent)> {
        let mut out = Vec::new();
        for (index, register) in self.numbered.iter().enumerate() {
            if let Some(register) = register {
                out.push((
                    std::char::from_digit(index as u32, 10).unwrap_or('0'),
                    register,
                ));
            }
        }
        for (index, register) in self.small.iter().enumerate() {
            if let Some(register) = register {
                out.push(((b'a' + index as u8) as char, register));
            }
        }
        if let Some(register) = &self.unnamed {
            out.push(('"', register));
        }
        if let Some(register) = &self.last_insert {
            out.push(('-', register));
        }
        if let Some(register) = &self.last_command {
            out.push((':', register));
        }
        if let Some(register) = &self.last_search {
            out.push(('/', register));
        }
        out
    }

    pub fn yank(&mut self, name: RegisterName, content: RegisterContent) {
        if name == RegisterName::BlackHole {
            return;
        }
        if matches!(name, RegisterName::Unnamed) {
            self.unnamed = Some(content);
            return;
        }
        self.set(name, content.clone());
        if name != RegisterName::Small('0') {
            self.unnamed = Some(content);
        }
    }

    pub fn delete(&mut self, name: RegisterName, content: RegisterContent) {
        if name == RegisterName::BlackHole {
            return;
        }
        let content = if name == RegisterName::Unnamed {
            content
        } else {
            self.set(name, content.clone());
            content
        };
        self.unnamed = Some(content.clone());
        self.numbered[0] = Some(RegisterContent {
            from_operation: true,
            ..content
        });
    }
}

fn index(ch: char) -> usize {
    (ch.to_ascii_lowercase() as u8 - b'a') as usize
}
