use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QfEntry {
    pub file: PathBuf,
    pub lnum: u32,
    pub col: u32,
    pub text: String,
    pub kind: Option<char>,
    pub nr: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct QuickfixList {
    pub title: String,
    pub entries: Vec<QfEntry>,
    pub current: usize,
    pub context: Option<rvim_viml::Value>,
}

impl QuickfixList {
    pub fn set(&mut self, title: String, entries: Vec<QfEntry>) {
        self.title = title;
        self.current = 0;
        self.entries = entries;
    }

    pub fn current(&self) -> Option<&QfEntry> {
        self.entries.get(self.current)
    }

    pub fn advance(&mut self) -> Option<&QfEntry> {
        if self.entries.is_empty() {
            return None;
        }
        self.current = (self.current + 1).min(self.entries.len() - 1);
        self.current()
    }

    pub fn previous(&mut self) -> Option<&QfEntry> {
        if self.entries.is_empty() {
            return None;
        }
        self.current = self.current.saturating_sub(1);
        self.current()
    }

    pub fn lines(&self) -> Vec<String> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                format!(
                    "{:>3} {}:{}:{}: {}",
                    index + 1,
                    entry.file.display(),
                    entry.lnum,
                    entry.col,
                    entry.text
                )
            })
            .collect()
    }
}

pub fn parse_error_lines(text: &str) -> Vec<QfEntry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        if let Some(entry) = parse_error_line(line) {
            entries.push(entry);
        }
    }
    entries
}

pub fn parse_error_line(line: &str) -> Option<QfEntry> {
    let mut fields = line.splitn(3, ':');
    let file = fields.next()?.trim();
    let line_number = fields.next()?.trim().parse::<u32>().ok()?;
    let rest = fields.next()?.trim();
    let (col, text) = if let Some((col, text)) = rest.split_once(':') {
        col.trim()
            .parse::<u32>()
            .ok()
            .map(|col| (col, text.trim().to_string()))
            .unwrap_or((1, rest.to_string()))
    } else {
        (1, rest.to_string())
    };
    if file.is_empty() || line_number == 0 {
        return None;
    }
    Some(QfEntry {
        file: PathBuf::from(file),
        lnum: line_number,
        col,
        text,
        kind: None,
        nr: None,
    })
}
