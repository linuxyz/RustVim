use std::collections::HashMap;

use rvim_types::VimString;

use crate::options_extra::EXTRA_SPECS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionType {
    Bool,
    Number,
    String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionScope {
    Global,
    Buffer,
    Window,
    Tab,
    GlobalLocalBuffer,
    GlobalLocalWindow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultValue {
    Bool(bool),
    Number(i64),
    Str(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionValue {
    Bool(bool),
    Number(i64),
    String(VimString),
}

impl OptionValue {
    pub fn as_bool(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0,
            Self::String(value) => !value.is_empty(),
        }
    }

    pub fn as_number(&self) -> i64 {
        match self {
            Self::Number(value) => *value,
            Self::Bool(value) => i64::from(*value),
            Self::String(value) => value.to_string_lossy().trim().parse().unwrap_or(0),
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            Self::Bool(value) => (if *value { "1" } else { "0" }).to_string(),
            Self::Number(value) => value.to_string(),
            Self::String(value) => value.to_string_lossy().into_owned(),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
        }
    }
}

pub struct OptionSpec {
    pub name: &'static str,
    pub short: &'static str,
    pub kind: OptionType,
    pub scope: OptionScope,
    pub default: DefaultValue,
    pub doc: &'static str,
    pub flags: &'static str,
    /// `has()` feature required for this option to exist, if any.
    pub feature: Option<&'static str>,
}

impl OptionSpec {
    /// Options gated behind an unimplemented feature are absent from `:set`
    /// and from `exists('+name')`, just like a Vim built without that feature.
    pub fn available(&self) -> bool {
        self.feature.map(rvim_types::has_feature).unwrap_or(true)
    }
}

macro_rules! spec {
    ($name:literal, $short:literal, $kind:ident, $scope:ident, $default:expr, $doc:literal, flags = $flags:literal) => {
        OptionSpec {
            name: $name,
            short: $short,
            kind: OptionType::$kind,
            scope: OptionScope::$scope,
            default: $default,
            doc: $doc,
            flags: $flags,
            feature: None,
        }
    };
    ($name:literal, $short:literal, $kind:ident, $scope:ident, $default:expr, $doc:literal, flags = $flags:literal, feature = $feature:literal) => {
        OptionSpec {
            name: $name,
            short: $short,
            kind: OptionType::$kind,
            scope: OptionScope::$scope,
            default: $default,
            doc: $doc,
            flags: $flags,
            feature: Some($feature),
        }
    };
    ($name:literal, $short:literal, $kind:ident, $scope:ident, $default:expr, $doc:literal) => {
        OptionSpec {
            name: $name,
            short: $short,
            kind: OptionType::$kind,
            scope: OptionScope::$scope,
            default: $default,
            doc: $doc,
            flags: "",
            feature: None,
        }
    };
    ($name:literal, $short:literal, $kind:ident, $scope:ident, $default:expr, $doc:literal, $feature:literal) => {
        OptionSpec {
            name: $name,
            short: $short,
            kind: OptionType::$kind,
            scope: OptionScope::$scope,
            default: $default,
            doc: $doc,
            flags: "",
            feature: Some($feature),
        }
    };
}

/// Core option table. The remaining Vim options are generated into the same
/// table as their semantics land; an unknown option fails explicitly instead of
/// being silently accepted.
pub static CORE_SPECS: &[OptionSpec] = &[
    spec!(
        "autoindent",
        "ai",
        Bool,
        Buffer,
        DefaultValue::Bool(false),
        "'autoindent'"
    ),
    spec!(
        "autoread",
        "ar",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'autoread'"
    ),
    spec!(
        "autowrite",
        "aw",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'autowrite'"
    ),
    spec!(
        "background",
        "bg",
        String,
        Global,
        DefaultValue::Str("light"),
        "'background'"
    ),
    spec!(
        "backspace",
        "bs",
        String,
        Global,
        DefaultValue::Str("indent,eol,start"),
        "'backspace'"
    ),
    spec!(
        "backup",
        "bk",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'backup'"
    ),
    spec!(
        "bomb",
        "bomb",
        Bool,
        Buffer,
        DefaultValue::Bool(false),
        "'bomb'"
    ),
    spec!(
        "buftype",
        "bt",
        String,
        Buffer,
        DefaultValue::Str(""),
        "'buftype'"
    ),
    spec!(
        "clipboard",
        "cb",
        String,
        Global,
        DefaultValue::Str(""),
        "'clipboard'"
    ),
    spec!(
        "cmdheight",
        "ch",
        Number,
        Global,
        DefaultValue::Number(1),
        "'cmdheight'"
    ),
    spec!(
        "colorcolumn",
        "cc",
        String,
        Window,
        DefaultValue::Str(""),
        "'colorcolumn'"
    ),
    spec!(
        "compatible",
        "cp",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'compatible'"
    ),
    spec!(
        "confirm",
        "cf",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'confirm'"
    ),
    spec!(
        "cursorcolumn",
        "cuc",
        Bool,
        Window,
        DefaultValue::Bool(false),
        "'cursorcolumn'"
    ),
    spec!(
        "cursorline",
        "cul",
        Bool,
        Window,
        DefaultValue::Bool(false),
        "'cursorline'"
    ),
    spec!(
        "endofline",
        "eol",
        Bool,
        Buffer,
        DefaultValue::Bool(true),
        "'endofline'"
    ),
    spec!(
        "equalalways",
        "ea",
        Bool,
        Global,
        DefaultValue::Bool(true),
        "'equalalways'"
    ),
    spec!(
        "expandtab",
        "et",
        Bool,
        Buffer,
        DefaultValue::Bool(false),
        "'expandtab'"
    ),
    spec!(
        "fileencoding",
        "fenc",
        String,
        Buffer,
        DefaultValue::Str(""),
        "'fileencoding'"
    ),
    spec!(
        "fileformat",
        "ff",
        String,
        Buffer,
        DefaultValue::Str("unix"),
        "'fileformat'"
    ),
    spec!(
        "fileformats",
        "ffs",
        String,
        Global,
        DefaultValue::Str("unix,dos"),
        "'fileformats'"
    ),
    spec!(
        "filetype",
        "ft",
        String,
        Buffer,
        DefaultValue::Str(""),
        "'filetype'"
    ),
    spec!(
        "fixendofline",
        "fixeol",
        Bool,
        Buffer,
        DefaultValue::Bool(true),
        "'fixendofline'"
    ),
    spec!(
        "foldcolumn",
        "fdc",
        Number,
        Window,
        DefaultValue::Number(0),
        "'foldcolumn'"
    ),
    spec!(
        "hidden",
        "hid",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'hidden'"
    ),
    spec!(
        "hlsearch",
        "hls",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'hlsearch'"
    ),
    spec!(
        "ignorecase",
        "ic",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'ignorecase'"
    ),
    spec!(
        "incsearch",
        "is",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'incsearch'"
    ),
    spec!(
        "laststatus",
        "ls",
        Number,
        Global,
        DefaultValue::Number(1),
        "'laststatus'"
    ),
    spec!(
        "list",
        "list",
        Bool,
        Window,
        DefaultValue::Bool(false),
        "'list'"
    ),
    spec!(
        "listchars",
        "lcs",
        String,
        Global,
        DefaultValue::Str("eol:$"),
        "'listchars'"
    ),
    spec!(
        "magic",
        "magic",
        Bool,
        Global,
        DefaultValue::Bool(true),
        "'magic'"
    ),
    spec!(
        "modeline",
        "ml",
        Bool,
        Buffer,
        DefaultValue::Bool(true),
        "'modeline'"
    ),
    spec!(
        "modelines",
        "mls",
        Number,
        Global,
        DefaultValue::Number(5),
        "'modelines'"
    ),
    spec!(
        "modifiable",
        "ma",
        Bool,
        Buffer,
        DefaultValue::Bool(true),
        "'modifiable'"
    ),
    spec!(
        "modified",
        "mod",
        Bool,
        Buffer,
        DefaultValue::Bool(false),
        "'modified'"
    ),
    spec!(
        "mouse",
        "mouse",
        String,
        Global,
        DefaultValue::Str(""),
        "'mouse'"
    ),
    spec!(
        "number",
        "nu",
        Bool,
        Window,
        DefaultValue::Bool(false),
        "'number'"
    ),
    spec!(
        "numberformat",
        "nfn",
        String,
        Window,
        DefaultValue::Str("%d"),
        "'numberformat'"
    ),
    spec!(
        "numberwidth",
        "nuw",
        Number,
        Window,
        DefaultValue::Number(4),
        "'numberwidth'"
    ),
    spec!(
        "readonly",
        "ro",
        Bool,
        Buffer,
        DefaultValue::Bool(false),
        "'readonly'"
    ),
    spec!(
        "relativenumber",
        "rnu",
        Bool,
        Window,
        DefaultValue::Bool(false),
        "'relativenumber'"
    ),
    spec!(
        "ruler",
        "ru",
        Bool,
        Global,
        DefaultValue::Bool(true),
        "'ruler'"
    ),
    spec!(
        "scrolloff",
        "so",
        Number,
        GlobalLocalWindow,
        DefaultValue::Number(0),
        "'scrolloff'"
    ),
    spec!(
        "shell",
        "sh",
        String,
        Global,
        DefaultValue::Str("/bin/sh"),
        "'shell'",
        flags = "P_STRING|P_EXPAND|P_VI_DEF|P_SECURE"
    ),
    spec!(
        "shiftwidth",
        "sw",
        Number,
        Buffer,
        DefaultValue::Number(8),
        "'shiftwidth'"
    ),
    spec!(
        "shortmess",
        "shm",
        String,
        Global,
        DefaultValue::Str("filnxtToOS"),
        "'shortmess'"
    ),
    spec!(
        "showcmd",
        "sc",
        Bool,
        Global,
        DefaultValue::Bool(true),
        "'showcmd'"
    ),
    spec!(
        "showmode",
        "smd",
        Bool,
        Global,
        DefaultValue::Bool(true),
        "'showmode'"
    ),
    spec!(
        "showtabline",
        "stal",
        Number,
        Global,
        DefaultValue::Number(1),
        "'showtabline'"
    ),
    spec!(
        "sidescrolloff",
        "siso",
        Number,
        GlobalLocalWindow,
        DefaultValue::Number(0),
        "'sidescrolloff'"
    ),
    spec!(
        "signcolumn",
        "scl",
        String,
        Window,
        DefaultValue::Str("auto"),
        "'signcolumn'"
    ),
    spec!(
        "smartcase",
        "scs",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'smartcase'"
    ),
    spec!(
        "smartindent",
        "si",
        Bool,
        Buffer,
        DefaultValue::Bool(false),
        "'smartindent'"
    ),
    spec!(
        "splitbelow",
        "sb",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'splitbelow'"
    ),
    spec!(
        "splitright",
        "spr",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'splitright'"
    ),
    spec!(
        "statusline",
        "stl",
        String,
        GlobalLocalWindow,
        DefaultValue::Str(""),
        "'statusline'"
    ),
    spec!(
        "swapfile",
        "swf",
        Bool,
        Buffer,
        DefaultValue::Bool(true),
        "'swapfile'"
    ),
    spec!(
        "tabstop",
        "ts",
        Number,
        Buffer,
        DefaultValue::Number(8),
        "'tabstop'"
    ),
    spec!(
        "termguicolors",
        "tgc",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'termguicolors'",
        "termguicolors"
    ),
    spec!(
        "textwidth",
        "tw",
        Number,
        Buffer,
        DefaultValue::Number(0),
        "'textwidth'"
    ),
    spec!(
        "timeout",
        "to",
        Bool,
        Global,
        DefaultValue::Bool(true),
        "'timeout'"
    ),
    spec!(
        "timeoutlen",
        "tm",
        Number,
        Global,
        DefaultValue::Number(1000),
        "'timeoutlen'"
    ),
    spec!(
        "title",
        "title",
        Bool,
        Global,
        DefaultValue::Bool(false),
        "'title'"
    ),
    spec!(
        "undolevels",
        "ul",
        Number,
        Global,
        DefaultValue::Number(1000),
        "'undolevels'"
    ),
    spec!(
        "updatetime",
        "ut",
        Number,
        Global,
        DefaultValue::Number(4000),
        "'updatetime'"
    ),
    spec!(
        "verbose",
        "vbs",
        Number,
        Global,
        DefaultValue::Number(0),
        "'verbose'"
    ),
    spec!(
        "viminfo",
        "vi",
        String,
        Global,
        DefaultValue::Str("'100,<50,s10,h"),
        "'viminfo'",
        flags = "P_STRING|P_ONECOMMA|P_NODUP|P_SECURE",
        feature = "viminfo"
    ),
    spec!(
        "wildmenu",
        "wmnu",
        Bool,
        Global,
        DefaultValue::Bool(true),
        "'wildmenu'"
    ),
    spec!(
        "wrap",
        "wrap",
        Bool,
        Window,
        DefaultValue::Bool(true),
        "'wrap'"
    ),
    spec!(
        "wrapscan",
        "ws",
        Bool,
        Global,
        DefaultValue::Bool(true),
        "'wrapscan'"
    ),
];

/// Every metadata row, including options gated behind an unimplemented
/// feature. Inventory audits use this; `:set` and completion use `specs`.
pub fn all_specs() -> impl Iterator<Item = &'static OptionSpec> {
    CORE_SPECS.iter().chain(EXTRA_SPECS.iter())
}

/// Options that exist in this build.
pub fn specs() -> impl Iterator<Item = &'static OptionSpec> {
    all_specs().filter(|spec| spec.available())
}

pub fn all_spec(name: &str) -> Option<&'static OptionSpec> {
    all_specs().find(|spec| spec.name == name)
}

pub fn spec(name: &str) -> Option<&'static OptionSpec> {
    all_spec(name).filter(|spec| spec.available())
}

pub fn spec_by_short(short: &str) -> Option<&'static OptionSpec> {
    all_specs().find(|spec| spec.available() && spec.short == short)
}

pub fn resolve(name: &str) -> Option<&'static OptionSpec> {
    if let Some(spec) = spec(name) {
        return Some(spec);
    }
    if let Some(spec) = spec_by_short(name) {
        return Some(spec);
    }
    let matches: Vec<&OptionSpec> = specs()
        .filter(|spec| spec.name.starts_with(name) || spec.short.starts_with(name))
        .collect();
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct OptionStore {
    global: HashMap<String, OptionValue>,
    buffer: HashMap<(u64, String), OptionValue>,
    window: HashMap<(u64, String), OptionValue>,
    termcap: HashMap<String, String>,
}

impl Default for OptionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl OptionStore {
    pub fn new() -> Self {
        let mut global = HashMap::new();
        for spec in specs() {
            // Every available option has a default value in the global table.
            // Buffer, window and tab scopes add overrides on top of it.
            global.insert(spec.name.to_string(), default_value(spec.default));
        }
        let mut termcap = HashMap::new();
        termcap.insert(
            "t_Co".to_string(),
            rvim_platform::term::capabilities().colors.to_string(),
        );
        for (name, value) in [
            ("t_cl", "\x1b[2J"),
            ("t_ce", "\x1b[K"),
            ("t_me", "\x1b[0m"),
            ("t_vi", "\x1b[?25l"),
            ("t_ve", "\x1b[?25h"),
            ("t_ti", "\x1b[?1049h"),
            ("t_te", "\x1b[?1049l"),
            ("t_AF", "\x1b[3%dm"),
            ("t_AB", "\x1b[4%dm"),
        ] {
            termcap.insert(name.to_string(), value.to_string());
        }
        if let Some(strings) =
            rvim_platform::terminfo::lookup_strings(&rvim_platform::term::term_name())
        {
            for (name, value) in [
                ("t_cl", strings.clear),
                ("t_ce", strings.el),
                ("t_me", strings.sgr0),
                ("t_vi", strings.civis),
                ("t_ve", strings.cnorm),
                ("t_ti", strings.smcup),
                ("t_te", strings.rmcup),
                ("t_AF", strings.setaf),
                ("t_AB", strings.setab),
            ] {
                if !value.is_empty() {
                    termcap.insert(name.to_string(), value);
                }
            }
        }
        Self {
            global,
            buffer: HashMap::new(),
            window: HashMap::new(),
            termcap,
        }
    }

    pub fn get(&self, name: &str) -> Option<&OptionValue> {
        self.global.get(name)
    }

    pub fn get_bool(&self, name: &str) -> bool {
        self.get(name).map(OptionValue::as_bool).unwrap_or(false)
    }

    pub fn get_number(&self, name: &str) -> i64 {
        self.get(name).map(OptionValue::as_number).unwrap_or(0)
    }

    pub fn get_string(&self, name: &str) -> String {
        self.get(name)
            .map(OptionValue::as_string)
            .unwrap_or_default()
    }

    pub fn buffer_value(&self, buffer: u64, name: &str) -> Option<&OptionValue> {
        self.buffer
            .get(&(buffer, name.to_string()))
            .or_else(|| self.global.get(name))
    }

    pub fn buffer_bool(&self, buffer: u64, name: &str) -> bool {
        self.buffer_value(buffer, name)
            .map(OptionValue::as_bool)
            .unwrap_or(false)
    }

    pub fn buffer_number(&self, buffer: u64, name: &str) -> i64 {
        self.buffer_value(buffer, name)
            .map(OptionValue::as_number)
            .unwrap_or(0)
    }

    pub fn buffer_string(&self, buffer: u64, name: &str) -> String {
        self.buffer_value(buffer, name)
            .map(OptionValue::as_string)
            .unwrap_or_default()
    }

    pub fn window_value(&self, window: u64, name: &str) -> Option<&OptionValue> {
        self.window
            .get(&(window, name.to_string()))
            .or_else(|| self.global.get(name))
    }

    pub fn window_bool(&self, window: u64, name: &str) -> bool {
        self.window_value(window, name)
            .map(OptionValue::as_bool)
            .unwrap_or(false)
    }

    pub fn window_number(&self, window: u64, name: &str) -> i64 {
        self.window_value(window, name)
            .map(OptionValue::as_number)
            .unwrap_or(0)
    }

    pub fn window_string(&self, window: u64, name: &str) -> String {
        self.window_value(window, name)
            .map(OptionValue::as_string)
            .unwrap_or_default()
    }

    pub fn set_global(&mut self, name: &str, value: OptionValue) -> Result<(), String> {
        let spec = spec(name).ok_or_else(|| format!("E518: Unknown option: {name}"))?;
        validate_value(spec, &value)?;
        self.global.insert(name.to_string(), value);
        Ok(())
    }

    pub fn set_buffer(
        &mut self,
        buffer: u64,
        name: &str,
        value: OptionValue,
    ) -> Result<(), String> {
        let spec = spec(name).ok_or_else(|| format!("E518: Unknown option: {name}"))?;
        validate_value(spec, &value)?;
        self.buffer.insert((buffer, name.to_string()), value);
        Ok(())
    }

    pub fn set_window(
        &mut self,
        window: u64,
        name: &str,
        value: OptionValue,
    ) -> Result<(), String> {
        let spec = spec(name).ok_or_else(|| format!("E518: Unknown option: {name}"))?;
        validate_value(spec, &value)?;
        self.window.insert((window, name.to_string()), value);
        Ok(())
    }

    pub fn reset_global(&mut self, name: &str) -> Result<(), String> {
        let spec = spec(name).ok_or_else(|| format!("E518: Unknown option: {name}"))?;
        self.global
            .insert(name.to_string(), default_value(spec.default));
        Ok(())
    }

    pub fn set_from_string(
        &mut self,
        name: &str,
        value: &str,
        local: bool,
        buffer: u64,
        window: u64,
        append: bool,
    ) -> Result<(), String> {
        let spec = spec(name).ok_or_else(|| format!("E518: Unknown option: {name}"))?;
        let previous = if local {
            self.buffer_value(buffer, name).cloned()
        } else {
            self.get(name).cloned()
        };
        let parsed = match spec.kind {
            OptionType::Bool => match value {
                "" | "1" | "true" | "on" | "yes" => OptionValue::Bool(true),
                "0" | "false" | "off" | "no" => OptionValue::Bool(false),
                _ => return Err(format!("E474: Invalid argument: {name}")),
            },
            OptionType::Number => {
                let number = value
                    .parse::<i64>()
                    .map_err(|_| format!("E521: Number required after =: {name}"))?;
                if append {
                    OptionValue::Number(previous.map_or(0, |v| v.as_number()) + number)
                } else {
                    OptionValue::Number(number)
                }
            }
            OptionType::String => {
                let text = if append {
                    format!(
                        "{}{}",
                        previous.map(|v| v.as_string()).unwrap_or_default(),
                        value
                    )
                } else {
                    value.to_string()
                };
                OptionValue::String(VimString::from(text))
            }
        };
        if local {
            match spec.scope {
                OptionScope::Window | OptionScope::GlobalLocalWindow => {
                    self.set_window(window, name, parsed)
                }
                _ => self.set_buffer(buffer, name, parsed),
            }
        } else {
            self.set_global(name, parsed)
        }
    }

    pub fn display(&self, name: &str, local: bool, buffer: u64, window: u64) -> Option<String> {
        let value = if local {
            if let Some(spec) = spec(name) {
                match spec.scope {
                    OptionScope::Window | OptionScope::GlobalLocalWindow => {
                        self.window_value(window, name)
                    }
                    _ => self.buffer_value(buffer, name),
                }
            } else {
                self.get(name)
            }
        } else {
            self.get(name)
        }?;
        Some(match value {
            OptionValue::Bool(value) => {
                if *value {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
            _ => value.as_string(),
        })
    }
}

fn default_value(value: DefaultValue) -> OptionValue {
    match value {
        DefaultValue::Bool(value) => OptionValue::Bool(value),
        DefaultValue::Number(value) => OptionValue::Number(value),
        DefaultValue::Str(value) => OptionValue::String(VimString::from(value)),
    }
}

fn value_matches(kind: OptionType, value: &OptionValue) -> bool {
    matches!(
        (kind, value),
        (OptionType::Bool, OptionValue::Bool(_))
            | (OptionType::Number, OptionValue::Number(_))
            | (OptionType::String, OptionValue::String(_))
    )
}

fn validate_value(spec: &OptionSpec, value: &OptionValue) -> Result<(), String> {
    if !value_matches(spec.kind, value) {
        return Err(format!("E474: Invalid argument: {}", spec.name));
    }
    let invalid = || format!("E474: Invalid argument: {}", spec.name);
    match spec.kind {
        OptionType::Number => {
            let number = value.as_number();
            match spec.name {
                "columns" => number >= 1,
                "lines" => number >= 2,
                "history" | "modelines" | "scroll" | "scrolloff" | "sidescrolloff"
                | "timeoutlen" | "updatetime" | "textwidth" | "report" | "verbose" | "maxmem"
                | "maxmemtot" | "pumheight" => number >= 0,
                "undolevels" => number >= -1,
                "tabstop" | "shiftwidth" | "softtabstop" => (-1..=9999).contains(&number),
                "numberwidth" => (1..=20).contains(&number),
                "cmdheight" | "winwidth" | "winheight" | "pumwidth" | "tabpagemax" => number >= 1,
                "laststatus" => (0..=3).contains(&number),
                "showtabline" => (0..=2).contains(&number),
                "regexpengine" => (0..=2).contains(&number),
                _ => true,
            }
        }
        OptionType::String => {
            let text = value.as_string();
            match spec.name {
                "background" => matches!(text.as_str(), "light" | "dark"),
                "fileformat" => matches!(text.as_str(), "unix" | "dos" | "mac"),
                "buftype" => matches!(
                    text.as_str(),
                    "" | "nofile" | "nowrite" | "acwrite" | "quickfix" | "help" | "prompt"
                ),
                "bufhidden" => matches!(text.as_str(), "" | "hide" | "unload" | "delete" | "wipe"),
                "selection" => matches!(text.as_str(), "inclusive" | "exclusive" | "old"),
                "encoding" => matches!(text.to_ascii_lowercase().as_str(), "utf-8" | "utf8"),
                "fileencoding" => {
                    text.is_empty()
                        || text
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
                }
                "mouse" => text
                    .chars()
                    .all(|ch| matches!(ch, 'a' | 'n' | 'v' | 'i' | 'c' | 'h')),
                "clipboard" => text.split(',').all(|part| {
                    matches!(
                        part,
                        "unnamed" | "unnamedplus" | "autoselect" | "autoselectplus"
                    ) || part.starts_with("exclude:")
                }),
                "cedit" => {
                    if text.is_empty() {
                        true
                    } else {
                        crate::keymap::parse_keys(&text).first().is_some_and(|key| {
                            match (&key.key, key.mods) {
                                (rvim_ui_api::Key::Char(ch), mods) => {
                                    ch.is_control()
                                        || mods.contains(rvim_ui_api::Mods::CTRL)
                                        || mods.contains(rvim_ui_api::Mods::ALT)
                                }
                                _ => true,
                            }
                        })
                    }
                }
                _ => true,
            }
        }
        OptionType::Bool => true,
    }
    .then_some(())
    .ok_or_else(invalid)
}

#[derive(Clone, Debug, Default)]
pub struct SetResult {
    pub messages: Vec<String>,
}

/// Parse one :set argument (Vim's :set grammar) and update the store.
pub fn execute_set(
    store: &mut OptionStore,
    args: &str,
    local: bool,
    buffer: u64,
    window: u64,
    global_only: bool,
) -> Result<SetResult, String> {
    let mut result = SetResult::default();
    for token in split_set_args(args) {
        if token.is_empty() {
            continue;
        }
        if token == "all" || token == "all&" {
            for spec in specs() {
                if token == "all&" {
                    store.reset_global(spec.name)?;
                } else {
                    result.messages.push(format!("-- {} --", spec.name));
                }
            }
            continue;
        }
        if token == "termcap" || token.starts_with("termcap ") {
            let size = rvim_platform::term::terminal_size();
            result.messages.push(format!(
                "-- Terminal capabilities --
terminal: {}
size: {}x{}
truecolor: {}
",
                rvim_platform::term::term_name(),
                size.cols,
                size.rows,
                if rvim_platform::term::colorterm_truecolor() {
                    "yes"
                } else {
                    "no"
                }
            ));
            continue;
        }
        if token.starts_with("t_") {
            let mut name = token.to_string();
            let mut value = None;
            if let Some((left, right)) = token.split_once('=') {
                name = left.to_string();
                value = Some(right.to_string());
            }
            let query = name.ends_with('?');
            let reset = name.ends_with('&');
            let name = name.trim_end_matches(['?', '&']).to_string();
            if query {
                let value = store.termcap.get(&name).cloned().unwrap_or_default();
                result.messages.push(format!("{name}={value}"));
            } else if reset {
                if name == "t_Co" {
                    store.termcap.insert(
                        name.clone(),
                        rvim_platform::term::capabilities().colors.to_string(),
                    );
                } else {
                    store.termcap.remove(&name);
                }
            } else if let Some(value) = value {
                store.termcap.insert(name, value);
            }
            continue;
        }
        let (mut name, mut bang, mut query) = (token.as_str(), false, false);
        if let Some(stripped) = name.strip_suffix('?') {
            name = stripped;
            query = true;
        }
        if let Some(stripped) = name.strip_suffix('!') {
            name = stripped;
            bang = true;
        }
        let (base, op) = split_set_operator(name);
        let mut invert = false;
        let mut reset = false;
        let mut no = false;
        let mut resolved = base;
        if let Some(stripped) = resolved.strip_prefix("no") {
            if resolve(stripped).is_some() {
                no = true;
                resolved = stripped;
            }
        }
        if let Some(stripped) = resolved.strip_prefix("inv") {
            if resolve(stripped).is_some() {
                invert = true;
                resolved = stripped;
            }
        }
        if let Some(stripped) = resolved
            .strip_suffix("&vim")
            .or_else(|| resolved.strip_suffix("&vi"))
        {
            resolved = stripped;
            reset = true;
        } else if let Some(stripped) = resolved.strip_suffix('&') {
            resolved = stripped;
            reset = true;
        }
        let Some(spec) = resolve(resolved) else {
            return Err(format!("E518: Unknown option: {resolved}"));
        };
        // :set stores scoped options in their natural scope. :setglobal keeps
        // using the same path for now; the explicit global-only split lands with
        // the option-scope milestone.
        let explicit_local = local;
        let scoped = matches!(
            spec.scope,
            OptionScope::Buffer
                | OptionScope::Window
                | OptionScope::Tab
                | OptionScope::GlobalLocalBuffer
                | OptionScope::GlobalLocalWindow
        );
        let local = !global_only && (explicit_local || scoped);
        let both = !global_only
            && !explicit_local
            && matches!(
                spec.scope,
                OptionScope::GlobalLocalBuffer | OptionScope::GlobalLocalWindow
            );
        if query || (bang && spec.kind == OptionType::Bool) {
            let value = store
                .display(spec.name, local, buffer, window)
                .unwrap_or_default();
            result.messages.push(format!("  {} {}", spec.name, value));
            continue;
        }
        if reset {
            if local {
                let value = default_value(spec.default);
                if matches!(
                    spec.scope,
                    OptionScope::Window | OptionScope::GlobalLocalWindow
                ) {
                    store.set_window(window, spec.name, value)?;
                } else {
                    store.set_buffer(buffer, spec.name, value)?;
                }
            } else {
                store.reset_global(spec.name)?;
            }
            if both {
                store.reset_global(spec.name)?;
            }
            continue;
        }
        if spec.kind == OptionType::Bool {
            let state = if let Some((_, value)) = op {
                match value {
                    "" | "1" | "true" | "on" | "yes" => true,
                    "0" | "false" | "off" | "no" => false,
                    _ => return Err(format!("E474: Invalid argument: {}", spec.name)),
                }
            } else if invert || bang {
                !if local {
                    match spec.scope {
                        OptionScope::Window | OptionScope::GlobalLocalWindow => {
                            store.window_bool(window, spec.name)
                        }
                        _ => store.buffer_bool(buffer, spec.name),
                    }
                } else {
                    store.get_bool(spec.name)
                }
            } else {
                !no
            };
            if local {
                match spec.scope {
                    OptionScope::Window | OptionScope::GlobalLocalWindow => {
                        store.set_window(window, spec.name, OptionValue::Bool(state))?
                    }
                    _ => store.set_buffer(buffer, spec.name, OptionValue::Bool(state))?,
                }
            } else {
                store.set_global(spec.name, OptionValue::Bool(state))?;
            }
            if both {
                store.set_global(spec.name, OptionValue::Bool(state))?;
            }
            continue;
        }
        let Some((_, value)) = op else {
            // A non-boolean option without = keeps its value but reports it.
            if let Some(display) = store.display(spec.name, local, buffer, window) {
                result.messages.push(format!("  {}={}", spec.name, display));
            }
            continue;
        };
        let append = token.contains("+=");
        store.set_from_string(spec.name, value, local, buffer, window, append)?;
        if both {
            store.set_from_string(spec.name, value, false, buffer, window, append)?;
        }
    }
    Ok(result)
}

fn split_set_args(args: &str) -> Vec<String> {
    // :set uses whitespace separation and backslash escaping.
    let mut out = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in args.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn split_set_operator(token: &str) -> (&str, Option<(char, &str)>) {
    let mut found = None;
    for (index, ch) in token.char_indices() {
        if ch == '=' {
            if index > 0 {
                let previous = token[..index].chars().last().unwrap_or(' ');
                if matches!(previous, '+' | '-' | '^') {
                    found = Some((index - 1, previous, index + 1));
                } else {
                    found = Some((index, '=', index + 1));
                }
            } else {
                found = Some((0, '=', 1));
            }
            break;
        }
    }
    if let Some((at, op, value_start)) = found {
        (&token[..at], Some((op, &token[value_start..])))
    } else {
        (token, None)
    }
}
