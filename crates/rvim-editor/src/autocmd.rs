use std::collections::BTreeMap;

use rvim_types::BufferId;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Event {
    BufNewFile,
    BufReadPre,
    BufReadPost,
    BufWritePre,
    BufWritePost,
    BufEnter,
    BufLeave,
    BufAdd,
    BufDelete,
    BufUnload,
    BufWipeout,
    FileType,
    Syntax,
    OptionSet,
    TextChanged,
    TextChangedI,
    InsertEnter,
    InsertLeave,
    WinEnter,
    WinLeave,
    TabNew,
    TabClosed,
    TabEnter,
    TabLeave,
    CmdlineEnter,
    CmdlineLeave,
    VimEnter,
    VimLeavePre,
    VimLeave,
    ColorScheme,
    SourcePre,
    SourcePost,
    QuickFixCmdPre,
    QuickFixCmdPost,
    VimResized,
    FocusGained,
    FocusLost,
    DirChanged,
    CmdwinEnter,
    CmdwinLeave,
    TermResponse,
    User(String),
}

impl Event {
    pub fn parse(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        Some(match lower.as_str() {
            "bufnewfile" => Self::BufNewFile,
            "bufreadpre" => Self::BufReadPre,
            "bufreadpost" | "bufread" => Self::BufReadPost,
            "bufwritepre" => Self::BufWritePre,
            "bufwritepost" | "bufwrite" => Self::BufWritePost,
            "bufenter" => Self::BufEnter,
            "bufleave" => Self::BufLeave,
            "bufadd" => Self::BufAdd,
            "bufdelete" => Self::BufDelete,
            "bufunload" => Self::BufUnload,
            "bufwipeout" => Self::BufWipeout,
            "filetype" => Self::FileType,
            "syntax" => Self::Syntax,
            "optionset" => Self::OptionSet,
            "textchanged" => Self::TextChanged,
            "textchangedi" => Self::TextChangedI,
            "insertenter" => Self::InsertEnter,
            "insertleave" => Self::InsertLeave,
            "winenter" => Self::WinEnter,
            "winleave" => Self::WinLeave,
            "tabnew" => Self::TabNew,
            "tabclosed" => Self::TabClosed,
            "tabenter" => Self::TabEnter,
            "tableave" => Self::TabLeave,
            "cmdlineenter" => Self::CmdlineEnter,
            "cmdlineleave" => Self::CmdlineLeave,
            "vimenter" => Self::VimEnter,
            "vimleavepre" => Self::VimLeavePre,
            "vimleave" => Self::VimLeave,
            "colorscheme" => Self::ColorScheme,
            "sourcepre" => Self::SourcePre,
            "sourcepost" => Self::SourcePost,
            "quickfixcmdpre" => Self::QuickFixCmdPre,
            "quickfixcmdpost" => Self::QuickFixCmdPost,
            "vimresized" => Self::VimResized,
            "focusgained" => Self::FocusGained,
            "focuslost" => Self::FocusLost,
            "dirchanged" => Self::DirChanged,
            "cmdwinenter" => Self::CmdwinEnter,
            "cmdwinleave" => Self::CmdwinLeave,
            "termresponse" => Self::TermResponse,
            "user" => Self::User(String::new()),
            _ => return None,
        })
    }

    pub fn name(&self) -> &str {
        match self {
            Self::BufNewFile => "BufNewFile",
            Self::BufReadPre => "BufReadPre",
            Self::BufReadPost => "BufReadPost",
            Self::BufWritePre => "BufWritePre",
            Self::BufWritePost => "BufWritePost",
            Self::BufEnter => "BufEnter",
            Self::BufLeave => "BufLeave",
            Self::BufAdd => "BufAdd",
            Self::BufDelete => "BufDelete",
            Self::BufUnload => "BufUnload",
            Self::BufWipeout => "BufWipeout",
            Self::FileType => "FileType",
            Self::Syntax => "Syntax",
            Self::OptionSet => "OptionSet",
            Self::TextChanged => "TextChanged",
            Self::TextChangedI => "TextChangedI",
            Self::InsertEnter => "InsertEnter",
            Self::InsertLeave => "InsertLeave",
            Self::WinEnter => "WinEnter",
            Self::WinLeave => "WinLeave",
            Self::TabNew => "TabNew",
            Self::TabClosed => "TabClosed",
            Self::TabEnter => "TabEnter",
            Self::TabLeave => "TabLeave",
            Self::CmdlineEnter => "CmdlineEnter",
            Self::CmdlineLeave => "CmdlineLeave",
            Self::VimEnter => "VimEnter",
            Self::VimLeavePre => "VimLeavePre",
            Self::VimLeave => "VimLeave",
            Self::ColorScheme => "ColorScheme",
            Self::SourcePre => "SourcePre",
            Self::SourcePost => "SourcePost",
            Self::QuickFixCmdPre => "QuickFixCmdPre",
            Self::QuickFixCmdPost => "QuickFixCmdPost",
            Self::VimResized => "VimResized",
            Self::FocusGained => "FocusGained",
            Self::FocusLost => "FocusLost",
            Self::DirChanged => "DirChanged",
            Self::CmdwinEnter => "CmdwinEnter",
            Self::CmdwinLeave => "CmdwinLeave",
            Self::TermResponse => "TermResponse",
            Self::User(_) => "User",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Autocmd {
    pub id: u64,
    pub group: String,
    pub event: Event,
    pub pattern: String,
    pub command: String,
    pub buffer_local: bool,
    pub nested: bool,
    pub once: bool,
}

#[derive(Clone, Debug, Default)]
pub struct AutocmdState {
    pub groups: BTreeMap<String, bool>,
    pub current_group: Option<String>,
    pub entries: Vec<Autocmd>,
    next_id: u64,
    firing: bool,
}

impl AutocmdState {
    pub fn new() -> Self {
        let mut groups = BTreeMap::new();
        groups.insert("default".to_string(), true);
        Self {
            groups,
            current_group: None,
            entries: Vec::new(),
            next_id: 1,
            firing: false,
        }
    }

    pub fn begin_group(&mut self, name: &str) {
        if name.eq_ignore_ascii_case("END") {
            self.current_group = None;
            return;
        }
        self.groups.entry(name.to_string()).or_insert(true);
        self.current_group = Some(name.to_string());
    }

    pub fn current_group(&self) -> String {
        self.current_group
            .clone()
            .unwrap_or_else(|| "default".to_string())
    }

    pub fn add(&mut self, event: Event, pattern: String, command: String) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let buffer_local = pattern == "<buffer>";
        self.entries.push(Autocmd {
            id,
            group: self.current_group(),
            event,
            pattern,
            command,
            buffer_local,
            nested: false,
            once: false,
        });
        id
    }

    pub fn clear(&mut self, group: Option<&str>, event: Option<&Event>, pattern: Option<&str>) {
        self.entries.retain(|autocmd| {
            let group_matches = group.is_none_or(|group| autocmd.group == group);
            let event_matches = event.is_none_or(|event| &autocmd.event == event);
            let pattern_matches = pattern.is_none_or(|pattern| autocmd.pattern == pattern);
            !(group_matches && event_matches && pattern_matches)
        });
    }

    pub fn delete_group(&mut self, group: &str) {
        self.groups.remove(group);
        self.entries.retain(|autocmd| autocmd.group != group);
    }

    pub fn list(&self) -> &[Autocmd] {
        &self.entries
    }

    pub fn matching(&self, event: &Event, file: Option<&str>, buffer: BufferId) -> Vec<Autocmd> {
        self.entries
            .iter()
            .filter(|autocmd| match (event, &autocmd.event) {
                (Event::User(_), Event::User(_)) => true,
                _ => &autocmd.event == event,
            })
            .filter(|autocmd| {
                let file = if let Event::User(name) = event {
                    Some(name.as_str())
                } else {
                    file
                };
                if autocmd.buffer_local {
                    autocmd.pattern == "<buffer>"
                        || autocmd.pattern == format!("<buffer={}>", rvim_types::Id::raw(buffer))
                } else {
                    match file {
                        Some(file) => pattern_matches(&autocmd.pattern, file),
                        None => autocmd.pattern == "*",
                    }
                }
            })
            .cloned()
            .collect()
    }

    pub fn set_firing(&mut self, firing: bool) {
        self.firing = firing;
    }

    pub fn is_firing(&self) -> bool {
        self.firing
    }

    pub fn remove_ids(&mut self, ids: &[u64]) {
        self.entries.retain(|autocmd| !ids.contains(&autocmd.id));
    }
}

pub fn pattern_matches(pattern: &str, file: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let file_name = std::path::Path::new(file)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.to_string());
    glob_match(pattern.as_bytes(), file.as_bytes(), 0)
        || glob_match(pattern.as_bytes(), file_name.as_bytes(), 0)
}

fn glob_match(pattern: &[u8], text: &[u8], depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    if pattern.is_empty() {
        return text.is_empty();
    }
    match pattern[0] {
        b'*' => {
            let rest = &pattern[1..];
            if glob_match(rest, text, depth + 1) {
                return true;
            }
            for index in 0..text.len() {
                if glob_match(rest, &text[index + 1..], depth + 1) {
                    return true;
                }
            }
            false
        }
        b'?' => !text.is_empty() && glob_match(&pattern[1..], &text[1..], depth + 1),
        b'[' => {
            let Some(end) = pattern.iter().position(|byte| *byte == b']') else {
                return !text.is_empty()
                    && pattern[0] == text[0]
                    && glob_match(&pattern[1..], &text[1..], depth + 1);
            };
            if text.is_empty() {
                return false;
            }
            let class = &pattern[1..end];
            let negate = class.first() == Some(&b'^');
            let class = if negate { &class[1..] } else { class };
            let mut matched = false;
            let mut index = 0;
            while index < class.len() {
                if index + 2 < class.len() && class[index + 1] == b'-' {
                    if text[0] >= class[index] && text[0] <= class[index + 2] {
                        matched = true;
                    }
                    index += 3;
                } else {
                    if text[0] == class[index] {
                        matched = true;
                    }
                    index += 1;
                }
            }
            matched != negate && glob_match(&pattern[end + 1..], &text[1..], depth + 1)
        }
        byte => {
            !text.is_empty() && byte == text[0] && glob_match(&pattern[1..], &text[1..], depth + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_patterns() {
        assert!(pattern_matches("*.rs", "/tmp/src/main.rs"));
        assert!(pattern_matches("/tmp/src/*", "/tmp/src/lib.rs"));
        assert!(!pattern_matches("*.{rs,py}", "main.rs")); // braces are not Vim glob syntax
        assert!(pattern_matches("file?.txt", "file1.txt"));
        assert!(pattern_matches("[abc]*.txt", "beta.txt"));
        assert!(!pattern_matches("[^abc]*.txt", "beta.txt"));
    }
}
