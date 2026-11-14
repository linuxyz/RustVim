use std::collections::HashMap;

use rvim_ui_api::{Attr, AttrFlags, Color};

#[derive(Clone, Debug)]
pub struct HighlightTable {
    groups: HashMap<String, Attr>,
}

impl Default for HighlightTable {
    fn default() -> Self {
        Self::new()
    }
}

impl HighlightTable {
    pub fn new() -> Self {
        let mut groups = HashMap::new();
        groups.insert("Normal".to_string(), Attr::DEFAULT);
        groups.insert(
            "StatusLine".to_string(),
            Attr {
                fg: Color::Default,
                bg: Color::Default,
                sp: Color::Default,
                flags: AttrFlags::REVERSE | AttrFlags::BOLD,
            },
        );
        groups.insert(
            "StatusLineNC".to_string(),
            Attr {
                flags: AttrFlags::REVERSE,
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "LineNr".to_string(),
            Attr {
                fg: Color::Indexed(11),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "CursorLineNr".to_string(),
            Attr {
                fg: Color::Indexed(11),
                flags: AttrFlags::BOLD,
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "NonText".to_string(),
            Attr {
                fg: Color::Indexed(12),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "CommandLine".to_string(),
            Attr {
                flags: AttrFlags::BOLD,
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "WarningMsg".to_string(),
            Attr {
                fg: Color::Indexed(9),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "ErrorMsg".to_string(),
            Attr {
                fg: Color::Indexed(9),
                flags: AttrFlags::BOLD,
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "TabLine".to_string(),
            Attr {
                flags: AttrFlags::REVERSE,
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "TabLineSel".to_string(),
            Attr {
                flags: AttrFlags::BOLD | AttrFlags::UNDERLINE,
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Comment".to_string(),
            Attr {
                fg: Color::Indexed(2),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Keyword".to_string(),
            Attr {
                fg: Color::Indexed(4),
                flags: AttrFlags::BOLD,
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "String".to_string(),
            Attr {
                fg: Color::Indexed(2),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Number".to_string(),
            Attr {
                fg: Color::Indexed(5),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Function".to_string(),
            Attr {
                fg: Color::Indexed(3),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Type".to_string(),
            Attr {
                fg: Color::Indexed(6),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Constant".to_string(),
            Attr {
                fg: Color::Indexed(5),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Identifier".to_string(),
            Attr {
                fg: Color::Indexed(6),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Statement".to_string(),
            Attr {
                fg: Color::Indexed(4),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "PreProc".to_string(),
            Attr {
                fg: Color::Indexed(5),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Special".to_string(),
            Attr {
                fg: Color::Indexed(5),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Todo".to_string(),
            Attr {
                fg: Color::Indexed(0),
                bg: Color::Indexed(11),
                flags: AttrFlags::BOLD,
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Error".to_string(),
            Attr {
                fg: Color::Indexed(15),
                bg: Color::Indexed(1),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "CursorLine".to_string(),
            Attr {
                bg: Color::Indexed(236),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "CursorLineNr".to_string(),
            Attr {
                fg: Color::Indexed(11),
                bg: Color::Indexed(236),
                flags: AttrFlags::BOLD,
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "ColorColumn".to_string(),
            Attr {
                bg: Color::Indexed(52),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "CursorColumn".to_string(),
            Attr {
                bg: Color::Indexed(238),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Visual".to_string(),
            Attr {
                fg: Color::Indexed(0),
                bg: Color::Indexed(11),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Search".to_string(),
            Attr {
                fg: Color::Indexed(0),
                bg: Color::Indexed(11),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "IncSearch".to_string(),
            Attr {
                fg: Color::Indexed(0),
                bg: Color::Indexed(14),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "Pmenu".to_string(),
            Attr {
                fg: Color::Indexed(0),
                bg: Color::Indexed(7),
                ..Attr::DEFAULT
            },
        );
        groups.insert(
            "PmenuSel".to_string(),
            Attr {
                fg: Color::Indexed(0),
                bg: Color::Indexed(11),
                ..Attr::DEFAULT
            },
        );
        Self { groups }
    }

    /// True when the group is defined.
    pub fn contains(&self, group: &str) -> bool {
        self.groups.contains_key(group)
    }

    pub fn get(&self, group: &str) -> Attr {
        self.groups.get(group).copied().unwrap_or_default()
    }

    pub fn set(&mut self, group: &str, attr: Attr) {
        self.groups.insert(group.to_string(), attr);
    }

    pub fn clear(&mut self, group: &str) {
        self.groups.remove(group);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, Attr)> + '_ {
        self.groups
            .iter()
            .map(|(name, attr)| (name.as_str(), *attr))
    }
}
