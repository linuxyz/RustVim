use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::time::{Duration, Instant};

use rvim_edit::{move_cursor, Motion, RegisterContent, RegisterKind, RegisterName, Registers};
use rvim_platform::fs as pfs;
use rvim_runtime::RuntimePaths;
use rvim_syntax::SyntaxEngine;
use rvim_types::{Arena, BufPos, BufferId, TabId, VimError, VimString, WindowId};
use rvim_ui_api::{
    ActionKind, ClickArea, ClickRegion, Key, KeyEvent, KeyKind, Mods, MouseButton, MouseEvent,
    MouseKind, Size, UiEvent, UiMode,
};
use rvim_view::{SplitDir, ViewState};
use rvim_viml::VimRuntime;

use crate::autocmd::{AutocmdState, Event};
use crate::buffer::BufferEntry;
use crate::highlights::HighlightTable;
use crate::keymap::{MapMode, MappingState};
use crate::options::OptionStore;
use crate::qf::{parse_error_lines, QfEntry, QuickfixList};
use crate::swap::{read_swap, swap_path, write_swap, SwapState};
use crate::tags::TagIndex;
use crate::usercmd::UserCommand;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
    UpperCase,
    LowerCase,
    Rot13,
    Format,
    Indent,
    Unindent,
    Filter,
    /// zf: create a fold over the motion.
    Fold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Ex,
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListDoCommand {
    Buffers,
    Windows,
    Tabs,
    Arguments,
    Quickfix,
    QuickfixFiles,
    Loclist,
    LoclistFiles,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsertState {
    pub start: BufPos,
    pub append: bool,
    pub replace: bool,
    pub undo_started: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct InsertCompletion {
    start: BufPos,
    candidates: Vec<String>,
    index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert(InsertState),
    Command(CommandKind),
    Visual {
        linewise: bool,
        block: bool,
        start: BufPos,
    },
}

impl Mode {
    pub fn is_insert(&self) -> bool {
        matches!(self, Self::Insert(_))
    }

    pub fn is_command(&self) -> bool {
        matches!(self, Self::Command(_))
    }
}

#[derive(Clone, Debug, Default)]
pub struct CommandLine {
    pub text: String,
    pub cursor: usize,
    pub prompt: char,
    pub kind: Option<CommandKind>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageKind {
    #[default]
    None,
    Info,
    Warning,
    Error,
    Echo,
    EchoMsg,
    EchoErr,
    EchoWindow,
}

#[derive(Clone, Debug, Default)]
pub struct MessageState {
    pub last: Option<String>,
    pub error: bool,
    pub kind: MessageKind,
    pub history: VecDeque<String>,
    pub statusmsg: String,
    pub errmsg: String,
    pub echo_window: bool,
    pub silent: bool,
    pub capture: Option<String>,
    pub filter: Option<String>,
}

impl MessageState {
    pub fn info(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(filter) = &self.filter {
            if !message.contains(filter) {
                return;
            }
        }
        if let Some(capture) = &mut self.capture {
            capture.push_str(&message);
            capture.push('\n');
        }
        self.last = Some(message.clone());
        self.statusmsg = message.clone();
        self.error = false;
        self.kind = MessageKind::Info;
        self.echo_window = false;
        if self.silent {
            self.history.push_back(message);
            return;
        }
        self.history.push_back(message);
        while self.history.len() > 200 {
            self.history.pop_front();
        }
    }

    pub fn warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(filter) = &self.filter {
            if !message.contains(filter) {
                return;
            }
        }
        if let Some(capture) = &mut self.capture {
            capture.push_str(&message);
            capture.push('\n');
        }
        if self.silent {
            self.history.push_back(message);
            return;
        }
        self.last = Some(message.clone());
        self.statusmsg = message.clone();
        self.error = false;
        self.kind = MessageKind::Warning;
        self.echo_window = false;
        self.history.push_back(message);
    }

    pub fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some(filter) = &self.filter {
            if !message.contains(filter) {
                return;
            }
        }
        if let Some(capture) = &mut self.capture {
            capture.push_str(&message);
            capture.push('\n');
        }
        if self.silent {
            self.errmsg = message.clone();
            self.history.push_back(message);
            return;
        }
        self.last = Some(message.clone());
        self.errmsg = message.clone();
        self.error = true;
        self.kind = MessageKind::Error;
        self.echo_window = false;
        self.history.push_back(message);
    }

    pub fn echo(&mut self, message: impl Into<String>, append: bool) {
        let message = message.into();
        if let Some(filter) = &self.filter {
            if !message.contains(filter) {
                return;
            }
        }
        if let Some(capture) = &mut self.capture {
            capture.push_str(&message);
            if !append {
                capture.push('\n');
            }
        }
        if self.silent {
            return;
        }
        if append {
            match &mut self.last {
                Some(existing) => existing.push_str(&message),
                None => self.last = Some(message),
            }
        } else {
            self.last = Some(message);
        }
        self.error = false;
        self.kind = MessageKind::Echo;
        self.echo_window = false;
    }

    pub fn echomsg(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.last = Some(message.clone());
        self.statusmsg = message.clone();
        self.error = false;
        self.kind = MessageKind::EchoMsg;
        self.history.push_back(message);
    }

    pub fn echoerr(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.last = Some(message.clone());
        self.errmsg = message.clone();
        self.error = true;
        self.kind = MessageKind::EchoErr;
        self.history.push_back(message);
    }

    pub fn echowindow(&mut self, message: impl Into<String>) {
        self.info(message);
        self.kind = MessageKind::EchoWindow;
        self.echo_window = true;
    }

    pub fn highlight(&self) -> &'static str {
        match self.kind {
            MessageKind::Error | MessageKind::EchoErr => "ErrorMsg",
            MessageKind::Warning => "WarningMsg",
            _ => "None",
        }
    }

    pub fn clear(&mut self) {
        self.last = None;
        self.error = false;
        self.kind = MessageKind::None;
        self.echo_window = false;
    }
}

#[derive(Clone, Debug)]
pub struct CmdwinState {
    pub buffer: BufferId,
    pub return_buffer: BufferId,
    pub kind: CommandKind,
    pub backward: bool,
}

type LoclistSaved = (
    WindowId,
    (QuickfixList, Vec<QuickfixList>, usize, Option<WindowId>),
);

#[derive(Clone, Debug, Default)]
pub struct LocationListState {
    pub current: QuickfixList,
    pub stack: Vec<QuickfixList>,
    pub stack_index: usize,
    pub window: Option<WindowId>,
}
#[derive(Clone, Debug)]
pub struct LastChange {
    pub insert: bool,
    pub text: String,
}

pub(crate) enum RedirTarget {
    File(std::path::PathBuf, bool),
    Register(char),
    Variable(String),
}

pub struct Editor {
    pub buffers: Arena<BufferEntry>,
    pub current_buffer: BufferId,
    pub view: ViewState,
    pub options: OptionStore,
    pub highlights: HighlightTable,
    pub registers: Registers,
    pub mode: Mode,
    pub cmdline: CommandLine,
    pub messages: MessageState,
    pub pending_count: Option<u32>,
    pub operator_count: Option<u32>,
    pub pending_operator: Option<Operator>,
    pub pending_register: Option<char>,
    pub pending_g: bool,
    pub pending_ctrl_w: bool,
    pub pending_z: bool,
    pub pending_bracket: Option<char>,
    pub pending_char: Option<CharPending>,
    pub should_quit: bool,
    pub should_redraw: bool,
    pub hit_enter: bool,
    pub size: Size,
    pub last_change: Option<LastChange>,
    pub last_search: Option<(String, i8)>,
    pub last_substitute_pattern: Option<String>,
    pub last_substitute_replacement: Option<String>,
    pub last_substitute_flags: String,
    loaded_packages: std::collections::HashSet<String>,
    insert_completion: Option<InsertCompletion>,
    insert_ctrl_x: bool,
    border_drag: Option<BorderDrag>,
    pub(crate) redir_target: Option<RedirTarget>,
    pub(crate) noautocmd_depth: u32,
    pub(crate) keepjumps_depth: u32,
    /// Set while sourcing a local .vimrc/.exrc with the 'secure' option on;
    /// commands that are unsafe in that context fail with E12.
    pub(crate) secure_sourcing: bool,
    /// Non-zero while a :sandbox command is executing; unsafe commands fail
    /// with E48.
    pub(crate) sandbox_depth: u32,
    pub(crate) vertical_requested: bool,
    last_change_keys: Vec<KeyEvent>,
    change_keys: Vec<KeyEvent>,
    change_start_tick: u64,
    replaying: bool,
    last_click: Option<(Instant, u16, u16, MouseButton, u8)>,
    pub last_bang: String,
    pub message_kind: MessageKind,
    pub message_append: bool,
    pub force_write: bool,
    pub scheduler: crate::scheduler::Scheduler,
    pub suspend_requested: bool,
    pub fed_keys: std::collections::VecDeque<rvim_viml::Value>,
    pub input_stack: Vec<std::collections::VecDeque<rvim_viml::Value>>,
    pub last_mouse: Option<(u16, u16)>,
    pub sourced_scripts: Vec<(u64, String)>,
    /// Path of the script currently being sourced, used to expand `<sfile>`.
    pub(crate) script_file: Option<String>,
    /// Script id of the Ex command currently executing.  User commands record
    /// it so their replacement text runs in the defining script's context.
    pub(crate) viml_script_context: Option<rvim_types::ScriptId>,
    /// When set, the next `source_viml` call executes as that script.
    pub(crate) viml_script_override: Option<rvim_types::ScriptId>,
    /// Optional category-based diagnostics log (see [`crate::trace`]).
    pub trace: crate::trace::Trace,
    next_script_id: u64,
    pub cmdwin: Option<CmdwinState>,
    pub statusline_cache: HashMap<(WindowId, String, u64, u32, u32, String), String>,
    /// Guard against re-entering fold computation from evaluated expressions.
    pub folds_syncing: bool,
    /// Sign type definitions (`:sign define`).
    pub sign_defs: HashMap<String, crate::signs::SignDef>,
    /// Bumped whenever signs or sign types change.
    pub sign_version: u64,
    /// Global text property types (prop_type_add without "bufnr").
    pub prop_types: HashMap<String, crate::textprop::PropType>,
    pub search_pattern: Option<String>,
    pub search_highlight_active: bool,
    pub search_origin: Option<BufPos>,
    pub command_history: VecDeque<String>,
    pub search_history: VecDeque<String>,
    pub oldfiles: Vec<String>,
    pub command_history_index: usize,
    pub cmdline_completion: Option<CmdCompletion>,
    pub last_visual: Option<(BufPos, BufPos, bool, bool)>,
    pub mappings: MappingState,
    pub autocmds: AutocmdState,
    pub alternate_buffer: Option<BufferId>,
    pub jumplist: Vec<(BufferId, BufPos)>,
    pub tab_vars: HashMap<TabId, HashMap<String, rvim_viml::Value>>,
    pub runtime: RuntimePaths,
    pub tag_index: TagIndex,
    pub quickfix: QuickfixList,
    pub quickfix_stack: Vec<QuickfixList>,
    pub quickfix_stack_index: usize,
    pub quickfix_window: Option<WindowId>,
    /// Preview window opened by :ptag/:pedit (TAG-07).
    pub preview_window: Option<WindowId>,
    /// Prompt-buffer state (BUF-09): displayed prompt text and callbacks.
    pub prompt_texts: HashMap<BufferId, String>,
    pub prompt_callbacks: HashMap<BufferId, rvim_viml::Value>,
    pub prompt_interrupts: HashMap<BufferId, rvim_viml::Value>,
    /// Per-window location lists (:l* commands and getloclist()).
    pub location_lists: HashMap<WindowId, LocationListState>,
    pub(crate) loclist_saved: Option<LoclistSaved>,
    pub syntax: SyntaxEngine,
    pub user_commands: HashMap<String, UserCommand>,
    pub viml: VimRuntime,
    pub viml_vars: HashMap<(rvim_viml::VarScope, String), rvim_viml::Value>,
    pub filetype_plugin: bool,
    pub filetype_indent: bool,
    pub autoload_depth: usize,
    pub swaps: SwapState,
    pub swap_enabled: bool,
    pub tag_stack: Vec<(BufferId, BufPos)>,
    pub tag_matches: Vec<crate::tags::TagEntry>,
    pub tag_match_index: usize,
    pub arglist: Vec<std::path::PathBuf>,
    pub arg_index: usize,
    pub previous_cwd: Option<std::path::PathBuf>,
    pub jump_index: usize,
    pub changelist: Vec<(BufferId, BufPos)>,
    pub change_index: usize,
    pub recording: Option<(char, Vec<KeyEvent>)>,
    pub macros: HashMap<char, Vec<KeyEvent>>,
    pub abbreviations: HashMap<String, String>,
    pub digraph_pending: Option<char>,
    pub insert_register_pending: bool,
    pub insert_literal_pending: bool,
    pub last_macro: Option<char>,
}

#[derive(Clone, Copy, Debug)]
struct BorderDrag {
    window: WindowId,
    vertical: bool,
    last_row: u16,
    last_col: u16,
}

#[derive(Clone, Debug)]
pub(crate) struct TablineClick {
    pub start: u16,
    pub end: u16,
    pub function: String,
    pub minwid: u32,
}

#[derive(Clone, Debug)]
pub struct CmdCompletion {
    pub start: usize,
    pub candidates: Vec<String>,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharPending {
    Find { forward: bool, till: bool },
    Replace,
    Mark,
    Z,
    TextObject { around: bool },
    Jump { exact: bool },
    MacroRecord,
    MacroPlay,
    Shift { right: bool },
}

impl Editor {
    pub fn new(size: Size) -> Self {
        let size = Size::new(size.rows.max(3), size.cols.max(20));
        let mut buffers = Arena::new();
        let placeholder = BufferId::default();
        let id = buffers.insert(BufferEntry::new(placeholder));
        if let Some(buffer) = buffers.get_mut(id) {
            buffer.id = id;
        }
        let view = ViewState::new(size, id);
        let mut options = OptionStore::new();
        let _ = options.set_global(
            "background",
            crate::options::OptionValue::String(VimString::from("dark")),
        );
        let window_height = size.rows.saturating_sub(1) as i64;
        let _ = options.set_global("window", crate::options::OptionValue::Number(window_height));
        let _ = options.set_global(
            "scroll",
            crate::options::OptionValue::Number((window_height / 2).max(1)),
        );
        let _ = options.set_global("maxmem", crate::options::OptionValue::Number(5120));
        let _ = options.set_global("maxmemtot", crate::options::OptionValue::Number(10240));
        let runtime = RuntimePaths::discover(
            rvim_platform::env::vimruntime(),
            std::env::var("VIMRUNTIME").ok(),
        );
        let tag_index = runtime
            .find("doc/tags")
            .map(|path| TagIndex::load(&path))
            .unwrap_or_default();
        let runtimepath = runtime
            .runtimepath()
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(",");
        let _ = options.set_global(
            "runtimepath",
            crate::options::OptionValue::String(VimString::from(runtimepath.clone())),
        );
        let _ = options.set_global(
            "packpath",
            crate::options::OptionValue::String(VimString::from(runtimepath)),
        );
        let mut editor = Self {
            buffers,
            current_buffer: id,
            view,
            options,
            highlights: HighlightTable::new(),
            registers: Registers::new(),
            mode: Mode::Normal,
            cmdline: CommandLine::default(),
            messages: MessageState::default(),
            pending_count: None,
            operator_count: None,
            pending_operator: None,
            pending_register: None,
            pending_g: false,
            pending_ctrl_w: false,
            pending_z: false,
            pending_bracket: None,
            pending_char: None,
            should_quit: false,
            should_redraw: true,
            hit_enter: false,
            size,
            last_change: None,
            last_search: None,
            last_substitute_pattern: None,
            last_substitute_replacement: None,
            last_substitute_flags: String::new(),
            loaded_packages: std::collections::HashSet::new(),
            insert_completion: None,
            insert_ctrl_x: false,
            border_drag: None,
            redir_target: None,
            noautocmd_depth: 0,
            keepjumps_depth: 0,
            secure_sourcing: false,
            sandbox_depth: 0,
            vertical_requested: false,
            last_change_keys: Vec::new(),
            change_keys: Vec::new(),
            change_start_tick: 0,
            replaying: false,
            last_click: None,
            last_bang: String::new(),
            message_kind: MessageKind::Info,
            message_append: false,
            force_write: false,
            scheduler: crate::scheduler::Scheduler::new(),
            suspend_requested: false,
            fed_keys: std::collections::VecDeque::new(),
            input_stack: Vec::new(),
            last_mouse: None,
            sourced_scripts: Vec::new(),
            script_file: None,
            viml_script_context: None,
            viml_script_override: None,
            trace: crate::trace::Trace::default(),
            next_script_id: 0,
            cmdwin: None,
            statusline_cache: HashMap::new(),
            folds_syncing: false,
            sign_defs: HashMap::new(),
            sign_version: 0,
            prop_types: HashMap::new(),
            search_pattern: None,
            search_highlight_active: false,
            search_origin: None,
            command_history: VecDeque::new(),
            search_history: VecDeque::new(),
            oldfiles: Vec::new(),
            command_history_index: 0,
            cmdline_completion: None,
            last_visual: None,
            mappings: MappingState::new(),
            autocmds: AutocmdState::new(),
            alternate_buffer: None,
            jumplist: Vec::new(),
            tab_vars: HashMap::new(),
            runtime,
            tag_index,
            quickfix: QuickfixList::default(),
            quickfix_stack: Vec::new(),
            quickfix_stack_index: 0,
            quickfix_window: None,
            preview_window: None,
            prompt_texts: HashMap::new(),
            prompt_callbacks: HashMap::new(),
            prompt_interrupts: HashMap::new(),
            location_lists: HashMap::new(),
            loclist_saved: None,
            syntax: SyntaxEngine::new(),
            user_commands: HashMap::new(),
            viml: VimRuntime::new(),
            viml_vars: HashMap::new(),
            filetype_plugin: false,
            filetype_indent: false,
            autoload_depth: 0,
            swaps: SwapState::new(),
            swap_enabled: true,
            tag_stack: Vec::new(),
            tag_matches: Vec::new(),
            tag_match_index: 0,
            arglist: Vec::new(),
            arg_index: 0,
            previous_cwd: None,
            jump_index: 0,
            changelist: Vec::new(),
            change_index: 0,
            recording: None,
            macros: HashMap::new(),
            abbreviations: HashMap::new(),
            digraph_pending: None,
            insert_register_pending: false,
            insert_literal_pending: false,
            last_macro: None,
        };
        editor
            .mappings
            .set_timeout(editor.options.get_number("timeoutlen").max(1) as u64);
        editor
            .mappings
            .set_timeout_enabled(editor.options.get_bool("timeout"));
        editor.sync_window_options();
        editor.update_window_buffer();
        editor
    }

    pub fn with_file(size: Size, path: &Path) -> Self {
        let mut editor = Self::new(size);
        if let Err(error) = editor.open_path(path) {
            editor.messages.error(error.to_string());
        }
        editor
    }

    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    pub fn current_buffer(&self) -> Option<&BufferEntry> {
        self.buffers.get(self.current_buffer)
    }

    pub fn current_buffer_mut(&mut self) -> Option<&mut BufferEntry> {
        self.buffers.get_mut(self.current_buffer)
    }

    pub fn cursor(&self) -> BufPos {
        self.view
            .current_window()
            .map(|window| window.cursor)
            .unwrap_or(BufPos::start())
    }

    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            Mode::Normal => "NORMAL",
            Mode::Insert(_) => "INSERT",
            Mode::Command(CommandKind::Ex) => "COMMAND",
            Mode::Command(CommandKind::Search) => "SEARCH",
            Mode::Visual { .. } => "VISUAL",
        }
    }

    pub fn ui_mode(&self) -> UiMode {
        match self.mode {
            Mode::Normal => UiMode::Normal,
            Mode::Insert(_) => UiMode::Insert,
            Mode::Command(_) => UiMode::Command,
            Mode::Visual { .. } => UiMode::Visual,
        }
    }

    pub fn line_count(&self) -> u32 {
        self.current_buffer().map_or(0, BufferEntry::line_count)
    }

    pub fn line(&self, lnum: u32) -> Option<&[u8]> {
        self.current_buffer()
            .and_then(|buffer| buffer.text.line(lnum))
    }

    pub fn set_cursor(&mut self, pos: BufPos) {
        let pos = self.clamp_cursor(pos);
        // Vim never leaves the cursor on a line hidden by a closed fold.
        let pos = self.fold_normalized(pos);
        let pos = self.clamp_cursor(pos);
        if let Some(window) = self.view.current_window_mut() {
            window.set_cursor(pos);
            window.viewport.skipcol = 0;
        }
    }

    pub fn ruler_text(&self) -> Option<String> {
        if !self.options.get_bool("ruler") || self.mode.is_command() {
            return None;
        }
        if self
            .view
            .current_window()
            .is_some_and(|window| window.status_height() > 0)
        {
            return None;
        }
        let pos = self.cursor();
        let format = self.options.get_string("rulerformat");
        if format.is_empty() {
            return Some(format!("{},{}", pos.lnum, pos.col + 1));
        }
        let width = self
            .view
            .current_window()
            .map_or(80, |window| window.text_width());
        Some(rvim_view::expand_statusline(
            &format,
            "",
            "",
            false,
            false,
            "",
            pos.lnum,
            pos.col + 1,
            self.line_count().max(1),
            self.mode_name(),
            width,
        ))
    }

    pub fn showmode_text(&self) -> Option<String> {
        if !self.options.get_bool("showmode") {
            return None;
        }
        let text = match &self.mode {
            Mode::Insert(state) => {
                if state.replace {
                    "-- REPLACE --"
                } else {
                    "-- INSERT --"
                }
            }
            Mode::Visual {
                linewise, block, ..
            } => {
                if *block {
                    "-- VISUAL BLOCK --"
                } else if *linewise {
                    "-- VISUAL LINE --"
                } else {
                    "-- VISUAL --"
                }
            }
            _ => return None,
        };
        Some(text.to_string())
    }

    pub fn showcmd_text(&self) -> Option<String> {
        if !self.options.get_bool("showcmd") {
            return None;
        }
        let mut out = String::new();
        if let Some(count) = self.pending_count.or(self.operator_count) {
            out.push_str(&count.to_string());
        }
        if let Some(register) = self.pending_register {
            out.push('"');
            out.push(register);
        }
        if let Some(operator) = self.pending_operator {
            out.push(match operator {
                Operator::Delete => 'd',
                Operator::Change => 'c',
                Operator::Yank => 'y',
                Operator::UpperCase => 'U',
                Operator::LowerCase => 'u',
                Operator::Rot13 => 'g',
                Operator::Format => 'g',
                Operator::Indent => '>',
                Operator::Unindent => '<',
                Operator::Filter => '!',
                Operator::Fold => 'f',
            });
        }
        if self.pending_g {
            out.push('g');
        }
        if self.pending_ctrl_w {
            out.push_str("^W");
        }
        if self.pending_z {
            out.push('z');
        }
        if let Some(bracket) = self.pending_bracket {
            out.push(bracket);
        }
        if let Some(pending) = &self.pending_char {
            out.push(match pending {
                CharPending::Find { forward, till } => match (forward, till) {
                    (true, true) => 't',
                    (true, false) => 'f',
                    (false, true) => 'T',
                    (false, false) => 'F',
                },
                CharPending::Replace => 'r',
                CharPending::Mark => 'm',
                CharPending::Z => 'z',
                CharPending::TextObject { around } => {
                    if *around {
                        'a'
                    } else {
                        'i'
                    }
                }
                CharPending::Jump { exact } => {
                    if *exact {
                        char::from(96)
                    } else {
                        char::from(39)
                    }
                }
                CharPending::MacroRecord => 'q',
                CharPending::MacroPlay => '@',
                CharPending::Shift { right } => {
                    if *right {
                        '>'
                    } else {
                        '<'
                    }
                }
            });
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    pub fn clamp_cursor(&self, pos: BufPos) -> BufPos {
        let Some(buffer) = self.current_buffer() else {
            return BufPos::start();
        };
        let lnum = pos.lnum.max(1).min(buffer.text.line_count().max(1));
        let len = buffer.text.line_len(lnum);
        let insert = self.mode.is_insert();
        let max_col = if insert { len } else { len.saturating_sub(1) };
        BufPos::new(lnum, pos.col.min(max_col))
    }

    pub fn read_default_viminfo(&mut self) {
        if self.options.get_string("viminfo").is_empty() {
            return;
        }
        if let Some(path) = crate::viminfo::default_path(self) {
            let _ = crate::viminfo::read(self, &path);
        }
    }

    pub fn write_default_viminfo(&self) {
        if self.options.get_string("viminfo").is_empty() {
            return;
        }
        if let Some(path) = crate::viminfo::default_path(self) {
            let _ = crate::viminfo::write(self, &path);
        }
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.messages.info(message);
    }

    pub fn set_error(&mut self, error: impl std::fmt::Display) {
        let error = error.to_string();
        self.messages.error(error.clone());
        self.viml
            .set_vvar("errmsg", rvim_viml::Value::String(VimString::from(error)));
    }

    pub fn request_redraw(&mut self) {
        self.should_redraw = true;
    }

    pub fn resize(&mut self, size: Size) {
        self.size = size;
        self.view.resize(size);
        self.should_redraw = true;
    }

    pub fn handle_event(&mut self, event: UiEvent) {
        if self.hit_enter {
            if let UiEvent::Key(_) = event {
                self.hit_enter = false;
                self.request_redraw();
                return;
            }
        }
        match event {
            UiEvent::Key(key) => self.handle_key(key),
            UiEvent::Text(text) => self.insert_text(text.as_bytes()),
            UiEvent::Paste(text) => self.insert_paste(&text),
            UiEvent::Resize(size) => self.resize(size),
            UiEvent::Focus(_)
            | UiEvent::Wake
            | UiEvent::Clipboard(_)
            | UiEvent::TermResponse(_)
            | UiEvent::Ime(_) => {}
            UiEvent::Mouse(mouse) => self.handle_mouse(mouse),
            UiEvent::Quit(_) => self.should_quit = true,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyKind::Release {
            return;
        }
        self.trace.log("input", &format!("{:?}", key));
        if let Some((register, keys)) = &mut self.recording {
            if key.key == Key::Char('q') && key.mods.is_empty() {
                let register = *register;
                let keys = std::mem::take(keys);
                self.recording = None;
                self.macros.insert(register, keys);
                self.last_macro = Some(register);
                self.messages.info(format!("recording stopped @{register}"));
                return;
            }
            keys.push(key.clone());
        }
        let mode = match self.mode {
            Mode::Normal if self.pending_operator.is_some() => MapMode::Operator,
            Mode::Normal => MapMode::Normal,
            Mode::Insert(_) => MapMode::Insert,
            Mode::Visual { .. } => MapMode::Visual,
            Mode::Command(_) => MapMode::Command,
        };
        let feed = self.mappings.feed(mode, key, self.current_buffer);
        let previous_silent = self.messages.silent;
        if feed.silent {
            self.messages.silent = true;
        }
        if let Some(expression) = feed.expr {
            match self.eval_viml(&expression) {
                Ok(value) => {
                    for key in crate::keymap::parse_keys(&value.echo_string()) {
                        self.dispatch_key(key);
                    }
                }
                Err(error) => self.set_error(error),
            }
        } else {
            for key in feed.keys {
                self.dispatch_key(key);
            }
        }
        self.messages.silent = previous_silent;
        if feed.pending {
            self.should_redraw = true;
        }
    }

    pub fn flush_mappings(&mut self) {
        let keys = self.mappings.flush();
        for key in keys {
            self.dispatch_key(key);
        }
    }

    pub fn next_deadline(&self) -> Option<std::time::Instant> {
        match (self.mappings.deadline(), self.scheduler.next_deadline()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    pub fn post_event(&mut self, event: crate::scheduler::EngineEvent) {
        self.scheduler.post(event);
    }

    pub fn start_timer(&mut self, millis: u64, repeat: bool, command: String) -> i64 {
        self.scheduler.start(millis, repeat, command)
    }

    pub fn stop_timer(&mut self, id: i64) -> bool {
        self.scheduler.stop(id)
    }

    pub fn on_tick(&mut self, now: std::time::Instant) {
        let mode = match self.mode {
            Mode::Normal if self.pending_operator.is_some() => MapMode::Operator,
            Mode::Normal => MapMode::Normal,
            Mode::Insert(_) => MapMode::Insert,
            Mode::Visual { .. } => MapMode::Visual,
            Mode::Command(_) => MapMode::Command,
        };
        if let Some(feed) = self.mappings.flush_timeout(mode, now) {
            let previous_silent = self.messages.silent;
            if feed.silent {
                self.messages.silent = true;
            }
            if let Some(expression) = feed.expr {
                match self.eval_viml(&expression) {
                    Ok(value) => {
                        for key in crate::keymap::parse_keys(&value.echo_string()) {
                            self.dispatch_key(key);
                        }
                    }
                    Err(error) => self.set_error(error),
                }
            } else {
                for key in feed.keys {
                    self.dispatch_key(key);
                }
            }
            self.messages.silent = previous_silent;
            self.request_redraw();
        }
        for event in self.scheduler.drain_events() {
            match event {
                crate::scheduler::EngineEvent::Redraw => self.request_redraw(),
                crate::scheduler::EngineEvent::Resize(size) => self.resize(size),
                crate::scheduler::EngineEvent::Job(_) => {
                    // Job execution lands with the P3 job subsystem; the
                    // reactor accepts the event now so hosts can post it.
                }
                crate::scheduler::EngineEvent::User(command) => self.execute_ex(&command),
            }
        }
        for command in self.scheduler.due(now) {
            self.execute_ex(&command);
        }
    }

    fn dispatch_key(&mut self, key: KeyEvent) {
        if !self.replaying && !self.mode.is_command() {
            let at_command_start = matches!(self.mode, Mode::Normal)
                && self.pending_operator.is_none()
                && self.pending_char.is_none()
                && !self.pending_g
                && !self.pending_ctrl_w
                && !self.pending_z
                && self.pending_register.is_none()
                && self.pending_count.is_none();
            if at_command_start {
                self.change_keys.clear();
                self.change_start_tick = self.buffer_tick();
            }
            self.change_keys.push(key.clone());
        }
        match self.mode.clone() {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Insert(state) => self.handle_insert_key(key, state),
            Mode::Command(kind) => self.handle_command_key(key, kind),
            Mode::Visual { .. } => self.handle_visual_key(key),
        }
        self.maybe_finish_change();
    }

    pub fn switch_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.request_redraw();
    }

    pub fn execute_ex(&mut self, line: &str) {
        self.trace.log("ex", line);
        self.statusline_cache.clear();
        if let Err(error) = self.execute_ex_line(line) {
            self.set_error(error);
        }
        self.should_redraw = true;
    }

    pub(crate) fn execute_ex_line(&mut self, line: &str) -> Result<(), VimError> {
        crate::ex::execute(self, line)
    }

    pub(crate) fn set_shell_error(&mut self, status: i32) {
        self.viml
            .set_vvar("shell_error", rvim_viml::Value::Number(status as i64));
    }

    pub(crate) fn run_shell(
        &mut self,
        command: &str,
        input: Option<&[u8]>,
    ) -> Result<rvim_platform::CommandOutput, rvim_platform::ProcessError> {
        let shell = self.options.get_string("shell");
        let flag = self.options.get_string("shellcmdflag");
        let shellquote = self.options.get_string("shellquote");
        let xquote = self.options.get_string("shellxquote");
        let xescape = self.options.get_string("shellxescape");
        let quoted = quote_shell_command(command, &shellquote, &xquote, &xescape);
        let shelltemp = self.options.get_bool("shelltemp");
        let result = if shelltemp && input.is_some() {
            self.run_shell_with_temp_file(&shell, &flag, &quoted, input.unwrap_or(&[]))
                .or_else(|_| rvim_platform::run_shell_command(&shell, &flag, &quoted, input))
        } else {
            rvim_platform::run_shell_command(&shell, &flag, &quoted, input)
        };
        if let Ok(output) = &result {
            self.set_shell_error(output.status);
        }
        result
    }

    /// Runs a filtering shell command with 'shelltemp' on: input and output
    /// go through private temporary files and the command is connected with
    /// 'shellredir'.
    fn run_shell_with_temp_file(
        &mut self,
        shell: &str,
        flag: &str,
        quoted: &str,
        input: &[u8],
    ) -> Result<rvim_platform::CommandOutput, rvim_platform::ProcessError> {
        let directory = std::env::temp_dir();
        let unique = format!(
            "rvim-shell-{}-{}",
            std::process::id(),
            SHELL_TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let input_path = directory.join(format!("{unique}.in"));
        let output_path = directory.join(format!("{unique}.out"));
        write_private(&input_path, input)?;
        write_private(&output_path, b"")?;
        let redirect = self.options.get_string("shellredir");
        let redirect = if redirect.is_empty() {
            ">".to_string()
        } else {
            redirect
        };
        let output_quoted = crate::ex::shell_quote(&output_path.display().to_string());
        let redirect = if redirect.contains("%s") {
            redirect.replace("%s", &output_quoted)
        } else {
            format!("{redirect}{output_quoted}")
        };
        let redirect = if redirect.contains("2>") {
            redirect
        } else {
            format!("{redirect} 2>&1")
        };
        let input_quoted = crate::ex::shell_quote(&input_path.display().to_string());
        let command = format!("{quoted} < {input_quoted} {redirect}");
        let result = rvim_platform::run_shell_command(shell, flag, &command, None);
        let output = result.map(|mut output| {
            output.stdout = std::fs::read(&output_path).unwrap_or_default();
            output.stderr = Vec::new();
            output
        });
        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(&output_path);
        output
    }

    fn accumulate_count(&mut self, ch: char) -> bool {
        if ch.is_ascii_digit() {
            if ch == '0' && self.pending_count.is_none() {
                return false;
            }
            let value = self.pending_count.unwrap_or(0);
            self.pending_count = Some(
                value
                    .saturating_mul(10)
                    .saturating_add(ch as u32 - '0' as u32),
            );
            self.messages.clear();
            return true;
        }
        false
    }

    fn take_count(&mut self) -> u32 {
        self.pending_count.take().unwrap_or(1).max(1)
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        if self.cmdwin.is_some() && key.key == Key::Enter && key.mods.is_empty() {
            self.execute_cmdwin();
            return;
        }
        if self.cmdwin.is_some() && key.mods.contains(Mods::CTRL) && key.key == Key::Char('c') {
            self.close_cmdwin();
            return;
        }
        if self.pending_register == Some('"') {
            if let Key::Char(ch) = key.key {
                if RegisterName::parse(ch).is_some() {
                    self.pending_register = Some(ch);
                } else {
                    self.pending_register = None;
                }
            }
            return;
        }
        if let Some(pending) = self.pending_char {
            self.handle_pending_char(pending, key);
            return;
        }
        if self.pending_ctrl_w {
            self.handle_ctrl_w_key(key);
            return;
        }
        if self.pending_z {
            self.handle_z_key(key);
            return;
        }
        if let Some(bracket) = self.pending_bracket.take() {
            match key.key {
                Key::Char('z') => self.fold_jump(bracket == ']'),
                Key::Char('c') => {
                    self.diff_jump(bracket == ']');
                }
                _ => {}
            }
            return;
        }
        if self.pending_g {
            self.handle_g_key(key);
            return;
        }
        if let Some(op) = self.pending_operator {
            self.handle_operator_pending(op, key);
            return;
        }
        if let Key::Char(ch) = key.key {
            if key.mods.contains(Mods::CTRL) {
                self.handle_normal_ctrl(ch);
                return;
            }
            if key.mods.contains(Mods::ALT) {
                return;
            }
            if self.accumulate_count(ch) {
                return;
            }
            match ch {
                'h' => self.move_count(Motion::Left),
                'j' => self.move_count(Motion::Down),
                'k' => self.move_count(Motion::Up),
                'l' => self.move_count(Motion::Right),
                'w' => self.move_count(Motion::WordForward),
                'b' => self.move_count(Motion::WordBackward),
                'e' => self.move_count(Motion::WordEnd),
                'W' => self.move_count(Motion::BigWordForward),
                'B' => self.move_count(Motion::BigWordBackward),
                'E' => self.move_count(Motion::BigWordEnd),
                '0' => self.move_count(Motion::LineStart),
                '^' => self.move_count(Motion::FirstNonBlank),
                '$' => self.move_count(Motion::LineEnd),
                '{' => self.move_count(Motion::ParagraphBackward),
                '}' => self.move_count(Motion::ParagraphForward),
                '(' => self.move_count(Motion::SentenceBackward),
                ')' => self.move_count(Motion::SentenceForward),
                '%' => self.move_count(Motion::MatchBracket),
                '+' => self.move_count(Motion::Down),
                '-' => self.move_count(Motion::Up),
                '_' => self.move_count(Motion::FirstNonBlank),
                '|' => {
                    let count = self.take_count();
                    self.goto_motion(Motion::Column(count));
                }
                'g' => {
                    self.pending_g = true;
                }
                '>' => self.pending_char = Some(CharPending::Shift { right: true }),
                '<' => self.pending_char = Some(CharPending::Shift { right: false }),
                'z' => self.pending_z = true,
                '[' => self.pending_bracket = Some('['),
                ']' => self.pending_bracket = Some(']'),
                'H' => self.viewport_goto(0),
                'M' => self.viewport_goto(1),
                'L' => self.viewport_goto(2),
                'G' => {
                    let count = self.pending_count.take();
                    self.goto_line(count.unwrap_or(self.line_count().max(1)));
                }
                'i' => self.enter_insert(InsertKind::Before),
                'a' => self.enter_insert(InsertKind::After),
                'I' => self.enter_insert(InsertKind::FirstNonBlank),
                'A' => self.enter_insert(InsertKind::EndOfLine),
                'o' => self.enter_insert(InsertKind::OpenBelow),
                'O' => self.enter_insert(InsertKind::OpenAbove),
                'R' => self.enter_insert(InsertKind::Replace),
                'x' => self.delete_chars(false),
                'X' => self.delete_chars(true),
                'd' => self.begin_operator(Operator::Delete),
                'c' => self.begin_operator(Operator::Change),
                'y' => self.begin_operator(Operator::Yank),
                'D' => self.delete_to_end(),
                'C' => self.change_to_end(),
                'Y' => self.yank_line(),
                's' => self.substitute_chars(),
                'S' => self.change_line(),
                'p' => self.put(false),
                'P' => self.put(true),
                'u' => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.undo();
                    }
                }
                'J' => {
                    let count = self.take_count();
                    self.join_lines(count, true);
                }
                '*' => {
                    if let Some(word) = self.keyword_under_cursor() {
                        self.search_pattern = Some(word.clone());
                        self.last_search = Some((word.clone(), 1));
                        if let Some(pos) = crate::ex::simple_search(self, &word, true) {
                            self.set_cursor(pos);
                        } else {
                            self.set_error(format!("E486: Pattern not found: {word}"));
                        }
                    }
                }
                '#' => {
                    if let Some(word) = self.keyword_under_cursor() {
                        self.search_pattern = Some(word.clone());
                        self.last_search = Some((word.clone(), -1));
                        if let Some(pos) = crate::ex::simple_search(self, &word, false) {
                            self.set_cursor(pos);
                        } else {
                            self.set_error(format!("E486: Pattern not found: {word}"));
                        }
                    }
                }
                'K' => {
                    self.pending_count = None;
                    if let Err(error) = self.keyword_lookup() {
                        self.set_error(error);
                    }
                }
                '~' => self.toggle_case(),
                'r' => self.pending_char = Some(CharPending::Replace),
                'f' => {
                    self.pending_char = Some(CharPending::Find {
                        forward: true,
                        till: false,
                    })
                }
                'F' => {
                    self.pending_char = Some(CharPending::Find {
                        forward: false,
                        till: false,
                    })
                }
                't' => {
                    self.pending_char = Some(CharPending::Find {
                        forward: true,
                        till: true,
                    })
                }
                'T' => {
                    self.pending_char = Some(CharPending::Find {
                        forward: false,
                        till: true,
                    })
                }
                'm' => self.pending_char = Some(CharPending::Mark),
                'q' => self.pending_char = Some(CharPending::MacroRecord),
                '@' => self.pending_char = Some(CharPending::MacroPlay),
                '\'' => self.pending_char = Some(CharPending::Jump { exact: false }),
                '`' => self.pending_char = Some(CharPending::Jump { exact: true }),
                ':' => self.begin_command(CommandKind::Ex),
                '/' => self.begin_command(CommandKind::Search),
                'n' => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.search_next(false);
                    }
                }
                'N' => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.search_next(true);
                    }
                }
                'Z' => self.pending_char = Some(CharPending::Z),
                '"' => {
                    self.pending_register = Some('"');
                }
                '.' => {
                    let count = self.take_count();
                    self.repeat_change(count);
                }
                'v' => {
                    self.mode = Mode::Visual {
                        linewise: false,
                        block: false,
                        start: self.cursor(),
                    }
                }
                'V' => {
                    self.mode = Mode::Visual {
                        linewise: true,
                        block: false,
                        start: self.cursor(),
                    }
                }
                _ => {}
            }
            self.after_normal_key_ch(ch);
            return;
        }
        match key.key {
            Key::Left => self.move_count(Motion::Left),
            Key::Right => self.move_count(Motion::Right),
            Key::Up => self.move_count(Motion::Up),
            Key::Down => self.move_count(Motion::Down),
            Key::Home => self.move_count(Motion::LineStart),
            Key::End => self.move_count(Motion::LineEnd),
            Key::PageDown => self.page_move(true),
            Key::PageUp => self.page_move(false),
            Key::Delete => self.delete_chars(false),
            Key::Insert => self.enter_insert(InsertKind::Before),
            Key::Esc => {
                self.pending_count = None;
                self.pending_operator = None;
                self.pending_g = false;
                self.pending_register = None;
                self.pending_char = None;
            }
            Key::Enter => self.move_count(Motion::Down),
            Key::Backspace => self.move_count(Motion::Left),
            _ => {}
        }
        self.pending_count = None;
    }

    fn after_normal_key_ch(&mut self, ch: char) {
        if ch != 'g' && !matches!(ch, 'Z' | '"') {
            self.pending_count = None;
            self.pending_g = false;
            self.pending_register = None;
        }
        self.request_redraw();
    }

    pub(crate) fn refresh_filetype(&mut self) {
        let filetype = self
            .options
            .buffer_string(self.current_buffer_id(), "filetype");
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.filetype = filetype.clone();
        }
        self.syntax.set_filetype(&filetype);
        self.fire_event(Event::FileType);
        self.fire_event(Event::Syntax);
        self.load_runtime_syntax();
        self.load_filetype_plugins();
    }

    pub(crate) fn load_runtime_syntax(&mut self) {
        if !self.syntax.enabled() {
            return;
        }
        let filetype = self
            .syntax
            .filetype()
            .map(str::to_string)
            .or_else(|| self.current_buffer().map(|buffer| buffer.filetype.clone()))
            .unwrap_or_default();
        if filetype.is_empty() {
            return;
        }
        let Some(path) = self.runtime.find(&format!("syntax/{filetype}.vim")) else {
            return;
        };
        let Ok(text) = std::fs::read(&path) else {
            return;
        };
        if let Err(error) = self.source_viml(&text, &path.display().to_string()) {
            self.messages
                .error(format!("syntax {}: {error}", path.display()));
        }
    }

    pub(crate) fn load_filetype_plugins(&mut self) {
        if !self.filetype_plugin && !self.filetype_indent {
            return;
        }
        let Some(filetype) = self.current_buffer().map(|buffer| buffer.filetype.clone()) else {
            return;
        };
        if filetype.is_empty() {
            return;
        }
        if self.filetype_plugin {
            if let Some(path) = self.runtime.find(&format!("ftplugin/{filetype}.vim")) {
                if let Ok(text) = std::fs::read(&path) {
                    if let Err(error) = self.source_viml(&text, &path.display().to_string()) {
                        self.messages
                            .error(format!("ftplugin {}: {error}", path.display()));
                    }
                }
            }
        }
        if self.filetype_indent {
            if let Some(path) = self.runtime.find(&format!("indent/{filetype}.vim")) {
                if let Ok(text) = std::fs::read(&path) {
                    if let Err(error) = self.source_viml(&text, &path.display().to_string()) {
                        self.messages
                            .error(format!("indent {}: {error}", path.display()));
                    }
                }
            }
        }
    }

    pub(crate) fn apply_undolevels(&mut self) {
        let limit = self.options.get_number("undolevels").max(1) as usize;
        for (_, buffer) in self.buffers.iter_mut() {
            buffer.undo.set_limit(limit);
        }
    }

    pub(crate) fn create_swap_for_current(&mut self) {
        let id = self.current_buffer;
        let Some(path) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone())
        else {
            return;
        };
        if !self
            .options
            .buffer_bool(rvim_types::Id::raw(id), "swapfile")
        {
            return;
        }
        let directory = self
            .options
            .buffer_string(rvim_types::Id::raw(id), "directory");
        let swap = swap_path(&path, Some(&directory));
        if swap.exists() {
            if let Ok((_, _, _)) = read_swap(&swap) {
                self.messages
                    .info(format!("Swap file {} already exists!", swap.display()));
            }
            return;
        }
        let tick = self
            .current_buffer()
            .map_or(0, |buffer| buffer.text.meta.changedtick);
        let text = self
            .current_buffer()
            .map(|buffer| buffer.text.to_file_bytes())
            .unwrap_or_default();
        if let Err(error) = write_swap(&swap, &path, &text, tick) {
            self.messages.error(format!(
                "E303: Unable to create swap file {}: {error}",
                swap.display()
            ));
            return;
        }
        self.swaps.set(id, swap);
    }

    pub(crate) fn update_swap_for(&mut self, id: BufferId) {
        if !self.swap_enabled {
            return;
        }
        let Some(swap) = self.swaps.get(id).cloned() else {
            return;
        };
        let Some(buffer) = self.buffers.get(id) else {
            return;
        };
        let tick = buffer.text.meta.changedtick;
        let text = buffer.text.to_file_bytes();
        if let Some(path) = buffer.text.meta.name.clone() {
            let _ = write_swap(&swap, &path, &text, tick);
        }
    }

    pub(crate) fn preserve_swaps(&mut self) {
        let ids = self.buffer_ids();
        for id in ids {
            self.update_swap_for(id);
        }
        self.messages.info("swap files updated");
    }

    pub fn load_undo_file(&mut self, id: BufferId) {
        if !self.options.get_bool("undofile") {
            return;
        }
        let Some(buffer) = self.buffers.get(id) else {
            return;
        };
        let name = buffer.name_string();
        let Some(path) = crate::undofile::path_for(self, &name) else {
            return;
        };
        let Ok((nodes, current)) = crate::undofile::read(&path) else {
            return;
        };
        if nodes.len() <= 1 {
            return;
        }
        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.undo.replace_nodes(nodes, current);
        }
    }

    pub fn save_undo_file(&mut self, id: BufferId) {
        if !self.options.get_bool("undofile") {
            return;
        }
        let Some(path) = self
            .buffers
            .get(id)
            .and_then(|buffer| crate::undofile::path_for(self, &buffer.name_string()))
        else {
            return;
        };
        let Some(buffer) = self.buffers.get(id) else {
            return;
        };
        let nodes = buffer.undo.nodes();
        let current = buffer.undo.current();
        if nodes.len() <= 1 {
            let _ = std::fs::remove_file(path);
            return;
        }
        let _ = crate::undofile::write(&path, nodes, current);
    }

    pub fn shutdown_swaps(&mut self) {
        let ids: Vec<BufferId> = self
            .buffers
            .iter()
            .map(|(id, _)| BufferId::from_raw(id))
            .collect();
        for id in ids {
            self.save_undo_file(id);
        }
        self.swaps.clear();
    }

    pub(crate) fn swap_name(&self) -> String {
        self.swaps
            .get(self.current_buffer)
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    }

    pub fn recover_current(&mut self, path: Option<&Path>) -> Result<(), VimError> {
        let mut swap = match path {
            Some(path) => {
                let preferred = swap_path(
                    path,
                    Some(
                        &self
                            .options
                            .buffer_string(self.current_buffer_id(), "directory"),
                    ),
                );
                if preferred.exists() {
                    preferred
                } else {
                    let local = swap_path(path, None);
                    if local.exists() {
                        local
                    } else {
                        preferred
                    }
                }
            }
            None => self
                .swaps
                .get(self.current_buffer)
                .cloned()
                .or_else(|| {
                    self.current_buffer()
                        .and_then(|buffer| buffer.text.meta.name.clone())
                        .map(|path| {
                            swap_path(
                                &path,
                                Some(
                                    &self
                                        .options
                                        .buffer_string(self.current_buffer_id(), "directory"),
                                ),
                            )
                        })
                })
                .ok_or_else(|| VimError::new(32, "No file name"))?,
        };
        let _ = &mut swap;
        let (original, text, _tick) = read_swap(&swap)
            .map_err(|error| VimError::new(13, format!("{}: {error}", swap.display())))?;
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.set_name(original);
            buffer.text.load_from_bytes(&text);
            buffer.text.meta.modified = true;
        }
        self.create_swap_for_current();
        self.messages.info(format!("Recovered {}", swap.display()));
        Ok(())
    }

    pub(crate) fn any_modified(&self) -> bool {
        self.buffers.iter().any(|(_, buffer)| buffer.modified())
    }

    pub(crate) fn write_all_buffers(&mut self) -> Result<usize, VimError> {
        let ids = self.buffer_ids();
        let mut written = 0;
        for id in ids {
            let Some(buffer) = self.buffers.get(id) else {
                continue;
            };
            if !buffer.modified() {
                continue;
            }
            let Some(path) = buffer.text.meta.name.clone() else {
                continue;
            };
            let mut bytes = buffer.text.to_file_bytes();
            if buffer.text.meta.bomb && bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
                bytes.drain(..3);
            }
            let fileencoding = buffer.text.meta.fileencoding.clone();
            let bomb = buffer.text.meta.bomb;
            let bytes = self.encode_with(&bytes, &fileencoding, bomb)?;
            rvim_platform::fs::write_file_atomic(&path, &bytes)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            if let Some(buffer) = self.buffers.get_mut(id) {
                buffer.set_modified(false);
            }
            written += 1;
        }
        Ok(written)
    }

    pub(crate) fn set_quickfix(&mut self, title: String, entries: Vec<QfEntry>) {
        let count = entries.len();
        self.quickfix.set(title, entries);
        if self.quickfix_stack_index + 1 < self.quickfix_stack.len() {
            self.quickfix_stack.truncate(self.quickfix_stack_index + 1);
        }
        self.quickfix_stack.push(self.quickfix.clone());
        if self.quickfix_stack.len() > 10 {
            self.quickfix_stack.remove(0);
        }
        self.quickfix_stack_index = self.quickfix_stack.len().saturating_sub(1);
        self.messages.info(format!("{count} quickfix entries"));
    }

    /// Window that owns the location list used by the current window.  When
    /// the current window is a location-list window, its owner is used.
    fn loclist_owner(&self) -> Option<WindowId> {
        let current = self.view.current_window()?.id;
        if let Some((owner, _)) = self
            .location_lists
            .iter()
            .find(|(_, state)| state.window == Some(current))
        {
            return Some(*owner);
        }
        Some(current)
    }

    /// Swaps the global quickfix state out so `:l*` commands operate on the
    /// current window's location list.  Returns the owner window.
    pub(crate) fn begin_loclist_command(&mut self) -> Option<WindowId> {
        if self.loclist_saved.is_some() {
            return None;
        }
        let owner = self.loclist_owner()?;
        if self.view.current_window().map(|window| window.id) != Some(owner) {
            self.view.set_current_window(owner);
            self.sync_current_buffer_to_window();
        }
        let state = self.location_lists.entry(owner).or_default();
        let saved = (
            std::mem::replace(&mut self.quickfix, state.current.clone()),
            std::mem::replace(&mut self.quickfix_stack, state.stack.clone()),
            std::mem::replace(&mut self.quickfix_stack_index, state.stack_index),
            std::mem::replace(&mut self.quickfix_window, state.window),
        );
        self.loclist_saved = Some((owner, saved));
        Some(owner)
    }

    /// Restores the global quickfix state and stores the updated location
    /// list back into the owner window.
    pub(crate) fn end_loclist_command(&mut self) {
        let Some((owner, (quickfix, stack, index, window))) = self.loclist_saved.take() else {
            return;
        };
        let state = self.location_lists.entry(owner).or_default();
        state.current = std::mem::replace(&mut self.quickfix, quickfix);
        state.stack = std::mem::replace(&mut self.quickfix_stack, stack);
        state.stack_index = std::mem::replace(&mut self.quickfix_stack_index, index);
        state.window = std::mem::replace(&mut self.quickfix_window, window);
    }

    fn resolve_loclist_window(&self, nr: i64) -> Option<WindowId> {
        if nr == 0 {
            return self.loclist_owner();
        }
        let ids = self.view.window_ids_in_current_tab();
        // Window numbers are the primary interpretation; window IDs are only
        // used when the number is out of range.
        if let Some(id) = ids.get(nr.saturating_sub(1) as usize) {
            return Some(*id);
        }
        ids.into_iter()
            .find(|id| rvim_types::Id::raw(*id) as i64 == nr)
    }

    /// Entries of a location list: 0 means the current window, otherwise a
    /// window number or window ID.
    pub(crate) fn loclist_entries_for(&self, nr: i64) -> Vec<QfEntry> {
        self.resolve_loclist_window(nr)
            .and_then(|owner| self.location_lists.get(&owner))
            .map(|state| state.current.entries.clone())
            .unwrap_or_default()
    }

    /// Sets a location list for a window number/ID (0 = current window).
    pub(crate) fn set_loclist_for(&mut self, nr: i64, title: String, entries: Vec<QfEntry>) {
        let Some(owner) = self.resolve_loclist_window(nr) else {
            return;
        };
        let count = entries.len();
        let state = self.location_lists.entry(owner).or_default();
        state.current.set(title, entries);
        if state.stack_index + 1 < state.stack.len() {
            state.stack.truncate(state.stack_index + 1);
        }
        state.stack.push(state.current.clone());
        if state.stack.len() > 10 {
            state.stack.remove(0);
        }
        state.stack_index = state.stack.len().saturating_sub(1);
        self.messages.info(format!("{count} location list entries"));
    }

    pub(crate) fn loclist_entries(&self) -> Vec<QfEntry> {
        self.loclist_entries_for(0)
    }

    #[allow(dead_code)]
    pub(crate) fn set_loclist(&mut self, title: String, entries: Vec<QfEntry>) {
        self.set_loclist_for(0, title, entries);
    }
    pub(crate) fn quickfix_older(&mut self) -> Result<(), VimError> {
        if self.quickfix_stack_index == 0 {
            return Err(VimError::new(776, "No older quickfix list"));
        }
        self.quickfix_stack_index -= 1;
        self.quickfix = self
            .quickfix_stack
            .get(self.quickfix_stack_index)
            .cloned()
            .unwrap_or_default();
        self.messages
            .info(format!("quickfix list {}", self.quickfix_stack_index + 1));
        Ok(())
    }

    pub(crate) fn quickfix_newer(&mut self) -> Result<(), VimError> {
        if self.quickfix_stack_index + 1 >= self.quickfix_stack.len() {
            return Err(VimError::new(777, "No newer quickfix list"));
        }
        self.quickfix_stack_index += 1;
        self.quickfix = self
            .quickfix_stack
            .get(self.quickfix_stack_index)
            .cloned()
            .unwrap_or_default();
        self.messages
            .info(format!("quickfix list {}", self.quickfix_stack_index + 1));
        Ok(())
    }

    pub(crate) fn open_quickfix(&mut self) -> Result<(), VimError> {
        if self.quickfix.entries.is_empty() {
            return Err(VimError::new(42, "No errors"));
        }
        if let Some(window) = self.quickfix_window {
            if self.view.windows.contains_key(&window) {
                self.view.set_current_window(window);
                self.sync_current_buffer_to_window();
                self.refresh_quickfix_buffer();
                return Ok(());
            }
        }
        let id = self.create_buffer(None)?;
        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.listed = false;
            buffer.readonly = true;
            buffer.buftype = "quickfix".to_string();
        }
        self.current_buffer = id;
        self.refresh_quickfix_buffer();
        let window = self
            .view
            .split(id, SplitDir::Horizontal)
            .ok_or_else(|| VimError::new(36, "Not enough room"))?;
        self.quickfix_window = Some(window);
        self.sync_window_options();
        self.request_redraw();
        Ok(())
    }

    fn refresh_quickfix_buffer(&mut self) {
        let lines = self.quickfix.lines().join("\n");
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.load_from_bytes(lines.as_bytes());
            buffer.set_modified(false);
        }
    }

    pub(crate) fn close_quickfix(&mut self) -> Result<(), VimError> {
        let Some(window) = self.quickfix_window.take() else {
            return Err(VimError::new(42, "No errors"));
        };
        if !self.view.windows.contains_key(&window) {
            return Ok(());
        }
        self.view.set_current_window(window);
        self.close_window_or_error()?;
        self.sync_current_buffer_to_window();
        Ok(())
    }

    pub(crate) fn quickfix_jump(&mut self, index: Option<usize>) -> Result<(), VimError> {
        if self.quickfix.entries.is_empty() {
            return Err(VimError::new(42, "No errors"));
        }
        if let Some(index) = index {
            self.quickfix.current = index.saturating_sub(1).min(self.quickfix.entries.len() - 1);
        } else {
            self.quickfix.current =
                (self.quickfix.current + 1).min(self.quickfix.entries.len() - 1);
        }
        let entry = self
            .quickfix
            .current()
            .cloned()
            .ok_or_else(|| VimError::new(42, "No errors"))?;
        let path = if entry.file.is_absolute() {
            entry.file.clone()
        } else {
            pfs::cwd().join(&entry.file)
        };
        let existing = self.buffer_ids().into_iter().find(|id| {
            self.buffers
                .get(*id)
                .is_some_and(|buffer| buffer.text.meta.name.as_ref() == Some(&path))
        });
        let switchbuf = self.options.get_string("switchbuf");
        if switchbuf.split(',').any(|item| item == "newtab") {
            self.tab_new(Some(&path))?;
        } else {
            let id = match existing {
                Some(id) => id,
                None => self.create_buffer(Some(&path))?,
            };
            if switchbuf
                .split(',')
                .any(|item| item == "vsplit" || item == "split")
            {
                let direction = if switchbuf.split(',').any(|item| item == "vsplit") {
                    rvim_view::SplitDir::Vertical
                } else {
                    rvim_view::SplitDir::Horizontal
                };
                self.split_window(direction, None)?;
            } else if switchbuf.split(',').any(|item| item == "useopen") {
                let target_window = self
                    .view
                    .windows
                    .iter()
                    .find(|(_, window)| window.buffer == id)
                    .map(|(_, window)| window.id);
                if let Some(window) = target_window {
                    self.view.set_current_window(window);
                    self.sync_current_buffer_to_window();
                }
            }
            self.switch_to_buffer(id)?;
        }
        self.set_cursor(BufPos::new(entry.lnum.max(1), entry.col.saturating_sub(1)));
        self.messages.info(format!(
            "({}/{}) {}",
            self.quickfix.current + 1,
            self.quickfix.entries.len(),
            entry.text
        ));
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn quickfix_previous(&mut self) -> Result<(), VimError> {
        if self.quickfix.entries.is_empty() {
            return Err(VimError::new(42, "No errors"));
        }
        self.quickfix.current = self.quickfix.current.saturating_sub(1);
        let index = self.quickfix.current + 1;
        self.quickfix_jump(Some(index))
    }

    pub(crate) fn parse_quickfix_text(&self, text: &str) -> Vec<QfEntry> {
        let errorformat = self
            .options
            .buffer_string(self.current_buffer_id(), "errorformat");
        if errorformat.is_empty() {
            parse_error_lines(text)
        } else {
            crate::errorformat::parse(text, &errorformat)
        }
    }

    pub(crate) fn run_make(&mut self, arguments: &str) -> Result<(), VimError> {
        let program = self.options.get_string("makeprg");
        let program = if program.is_empty() {
            "make".to_string()
        } else {
            program
        };
        let quoted = if arguments.is_empty() {
            String::new()
        } else {
            crate::ex::shell_quote(arguments)
        };
        let program = if program.contains("$*") {
            program.replace("$*", &quoted)
        } else if quoted.is_empty() {
            program
        } else {
            format!("{program} {quoted}")
        };
        let output = rvim_platform::run_shell(&program, None)
            .map_err(|error| VimError::new(13, error.to_string()))?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        let entries = self.parse_quickfix_text(&text);
        let count = entries.len();
        let status = output.status;
        self.set_quickfix(format!(":make {program}"), entries);
        if count == 0 {
            self.messages.info(format!("make exited {status}"));
        }
        Ok(())
    }

    pub(crate) fn current_tab_id(&self) -> u64 {
        rvim_types::Id::raw(self.view.current_tab)
    }

    fn sync_message_vvars(&mut self) {
        let status = self.messages.statusmsg.clone();
        let error = self.messages.errmsg.clone();
        self.viml.set_vvar(
            "statusmsg",
            rvim_viml::Value::String(VimString::from(status)),
        );
        self.viml
            .set_vvar("errmsg", rvim_viml::Value::String(VimString::from(error)));
    }

    pub fn source_viml(&mut self, source: &[u8], name: &str) -> Result<(), VimError> {
        self.trace
            .log("viml", &format!("source {name} ({} bytes)", source.len()));
        let source = apply_scriptencoding(source);
        let previous_script_file = self.script_file.replace(name.to_string());
        let override_script = self.viml_script_override.take();
        let mut runtime = std::mem::take(&mut self.viml);
        let vars = std::mem::take(&mut self.viml_vars);
        let mut host = crate::viml_host::EditorVimHost::new(self, vars);
        let result = match override_script {
            Some(script_id) => {
                rvim_viml::execute_script_in_context(&source, script_id, &mut runtime, &mut host)
            }
            None => rvim_viml::execute_script(&source, name, &mut runtime, &mut host),
        };
        let vars = std::mem::take(&mut host.vars);
        drop(host);
        self.viml = runtime;
        self.viml_vars = vars;
        self.sync_message_vvars();
        if !name.is_empty() && name != "[command]" {
            self.next_script_id += 1;
            self.sourced_scripts
                .push((self.next_script_id, name.to_string()));
        }
        self.script_file = previous_script_file;
        result.map_err(|error| VimError::plain(error.to_string()))
    }

    /// Opens the diagnostics log (`--log`) with the given category filter
    /// (`--log-categories`).
    pub fn set_trace(&mut self, path: &std::path::Path, categories: &str) -> Result<(), String> {
        self.trace = crate::trace::Trace::open(path, categories)?;
        Ok(())
    }

    /// True when the diagnostics log is active for `category`.
    pub fn trace_enabled(&self, category: &str) -> bool {
        self.trace.enabled_for(category)
    }
    /// Source .vimrc or .exrc from `dir` when 'exrc' is set.  While
    /// 'secure' is on, shell, write and autocommand commands are rejected
    /// with E12, mirroring Vim's secure exrc handling.
    pub fn source_local_exrc(&mut self, dir: &std::path::Path) -> Result<(), VimError> {
        if !self.options.get_bool("exrc") {
            return Ok(());
        }
        let path = [".vimrc", ".exrc"]
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file());
        let Some(path) = path else {
            return Ok(());
        };
        let bytes = std::fs::read(&path).map_err(|error| {
            VimError::new(484, format!("Can't open file {}: {error}", path.display()))
        })?;
        let previous = self.secure_sourcing;
        self.secure_sourcing = self.options.get_bool("secure");
        let result = self.source_viml(&bytes, &path.display().to_string());
        self.secure_sourcing = previous;
        result
    }

    pub fn eval_viml(&mut self, expression: &str) -> Result<rvim_viml::Value, VimError> {
        self.trace.log("viml", &format!("eval {expression}"));
        self.ensure_autoloads(expression.as_bytes());
        let mut runtime = std::mem::take(&mut self.viml);
        let vars = std::mem::take(&mut self.viml_vars);
        let mut host = crate::viml_host::EditorVimHost::new(self, vars);
        let result = rvim_viml::eval_expression(expression.as_bytes(), &mut runtime, &mut host);
        let vars = std::mem::take(&mut host.vars);
        drop(host);
        self.viml = runtime;
        self.viml_vars = vars;
        self.sync_message_vvars();
        result.map_err(|error| VimError::plain(error.to_string()))
    }

    pub fn call_viml_function(
        &mut self,
        name: &str,
        args: Vec<rvim_viml::Value>,
    ) -> Result<rvim_viml::Value, VimError> {
        let mut runtime = std::mem::take(&mut self.viml);
        let vars = std::mem::take(&mut self.viml_vars);
        let mut host = crate::viml_host::EditorVimHost::new(self, vars);
        let result = rvim_viml::call_function(name, args, &mut runtime, &mut host);
        let vars = std::mem::take(&mut host.vars);
        drop(host);
        self.viml = runtime;
        self.viml_vars = vars;
        self.sync_message_vvars();
        result.map_err(|error| VimError::plain(error.to_string()))
    }

    fn ensure_autoloads(&mut self, source: &[u8]) {
        if self.autoload_depth >= 8 {
            return;
        }
        let text = String::from_utf8_lossy(source);
        let mut names = Vec::new();
        let mut token = String::new();
        for ch in text.chars() {
            if ch.is_ascii_alphanumeric() || ch == '#' || ch == '_' {
                token.push(ch);
            } else {
                if token.contains('#') {
                    names.push(std::mem::take(&mut token));
                } else {
                    token.clear();
                }
            }
        }
        if token.contains('#') {
            names.push(token);
        }
        names.sort();
        names.dedup();
        self.autoload_depth += 1;
        for name in names {
            if let Some((path_component, _function)) = name.rsplit_once('#') {
                let relative = format!("autoload/{}.vim", path_component.replace('#', "/"));
                if let Some(path) = self.runtime.find(&relative) {
                    if let Ok(bytes) = std::fs::read(&path) {
                        let _ = self.source_viml(&bytes, &path.display().to_string());
                    }
                }
            }
        }
        self.autoload_depth -= 1;
    }

    pub fn execute_viml(&mut self, source: &str, name: &str) -> Result<(), VimError> {
        self.source_viml(source.as_bytes(), name)
    }

    fn call_viml_value(
        &mut self,
        callee: &rvim_viml::Value,
        args: Vec<rvim_viml::Value>,
    ) -> Result<rvim_viml::Value, VimError> {
        let mut runtime = std::mem::take(&mut self.viml);
        let vars = std::mem::take(&mut self.viml_vars);
        let mut host = crate::viml_host::EditorVimHost::new(self, vars);
        let result = rvim_viml::call_value(&mut runtime, callee, args, &mut host);
        let vars = std::mem::take(&mut host.vars);
        drop(host);
        self.viml = runtime;
        self.viml_vars = vars;
        self.sync_message_vvars();
        result.map_err(|error| VimError::plain(error.to_string()))
    }

    /// Sets the prompt text of a prompt buffer (BUF-09).
    pub(crate) fn set_prompt_text(&mut self, buffer: BufferId, text: String) {
        self.prompt_texts.insert(buffer, text.clone());
        let is_prompt = self
            .buffers
            .get(buffer)
            .is_some_and(|entry| entry.buftype == "prompt" && entry.loaded);
        if is_prompt {
            if let Some(entry) = self.buffers.get_mut(buffer) {
                entry
                    .text
                    .restore_lines(vec![VimString::from(text.clone())]);
                entry.set_modified(false);
            }
            if buffer == self.current_buffer {
                self.set_cursor(BufPos::new(1, text.len() as u32));
            }
        }
    }

    /// Invokes the prompt callback (Enter) or interrupt callback (CTRL-C) and
    /// restores the prompt line.
    fn prompt_submit(&mut self, interrupt: bool) {
        let buffer = self.current_buffer;
        let prompt = self.prompt_texts.get(&buffer).cloned().unwrap_or_default();
        let line = self
            .line(self.cursor().lnum)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default();
        let typed = line
            .strip_prefix(prompt.as_str())
            .map(str::to_string)
            .unwrap_or(line);
        if let Some(entry) = self.buffers.get_mut(buffer) {
            entry
                .text
                .restore_lines(vec![VimString::from(prompt.clone())]);
            entry.set_modified(false);
        }
        self.set_cursor(BufPos::new(1, prompt.len() as u32));
        let callee = if interrupt {
            self.prompt_interrupts.get(&buffer).cloned()
        } else {
            self.prompt_callbacks.get(&buffer).cloned()
        };
        if let Some(callee) = callee {
            let args = if interrupt {
                Vec::new()
            } else {
                vec![rvim_viml::Value::string(typed)]
            };
            if let Err(error) = self.call_viml_value(&callee, args) {
                self.set_error(error);
            }
        }
        self.request_redraw();
    }

    fn ensure_preview_window(&mut self) -> Result<WindowId, VimError> {
        if let Some(id) = self.preview_window {
            if self.view.windows.contains_key(&id) {
                return Ok(id);
            }
            self.preview_window = None;
        }
        let buffer = self.create_buffer(None)?;
        let id = self
            .view
            .split(buffer, SplitDir::Horizontal)
            .ok_or_else(|| VimError::new(36, "Not enough room"))?;
        self.preview_window = Some(id);
        self.view.set_current_window(id);
        self.current_buffer = buffer;
        let target = self
            .options
            .get_number("previewheight")
            .clamp(1, u16::MAX as i64) as u16;
        let _ = self.view.set_window_text_height(id, target);
        self.sync_window_options();
        self.request_redraw();
        Ok(id)
    }

    /// Implements :ptag and CTRL-W }: show the tag in the preview window
    /// while keeping the cursor in the original window.
    pub(crate) fn open_preview_tag(&mut self, name: &str) -> Result<(), VimError> {
        let origin = self.view.current_window().map(|window| window.id);
        let preview = self.ensure_preview_window()?;
        self.view.set_current_window(preview);
        self.sync_current_buffer_to_window();
        let result = self.jump_to_tag(name);
        if let Some(origin) = origin {
            self.view.set_current_window(origin);
            self.sync_current_buffer_to_window();
        }
        self.request_redraw();
        result
    }

    /// Implements :pedit: edit a file in the preview window.
    pub(crate) fn open_preview_file(&mut self, path: &std::path::Path) -> Result<(), VimError> {
        let origin = self.view.current_window().map(|window| window.id);
        let preview = self.ensure_preview_window()?;
        self.view.set_current_window(preview);
        self.sync_current_buffer_to_window();
        let result = self.open_path(path);
        if let Some(origin) = origin {
            self.view.set_current_window(origin);
            self.sync_current_buffer_to_window();
        }
        self.request_redraw();
        result
    }

    /// Implements :pclose / CTRL-W z.
    pub(crate) fn close_preview(&mut self) -> Result<(), VimError> {
        let Some(id) = self.preview_window.take() else {
            return Ok(());
        };
        if !self.view.windows.contains_key(&id) {
            return Ok(());
        }
        self.view.set_current_window(id);
        self.sync_current_buffer_to_window();
        self.close_window_or_error()
    }
    pub(crate) fn jump_to_tag(&mut self, name: &str) -> Result<(), VimError> {
        let entries = self.tag_index.find_all(name).to_vec();
        if entries.is_empty() {
            return Err(VimError::new(433, format!("Tag not found: {name}")));
        }
        self.tag_matches = entries;
        self.tag_match_index = 0;
        let entry = self.tag_matches[0].clone();
        self.jump_to_tag_entry(entry, name)
    }

    pub(crate) fn jump_to_tag_entry(
        &mut self,
        entry: crate::tags::TagEntry,
        name: &str,
    ) -> Result<(), VimError> {
        let old_buffer = self.current_buffer;
        let old_pos = self.cursor();
        self.record_jump();
        self.open_file(&entry.file)?;
        if entry
            .file
            .components()
            .any(|component| component.as_os_str() == "doc")
        {
            self.swaps.remove(self.current_buffer);
            let id = self.current_buffer;
            if let Some(buffer) = self.current_buffer_mut() {
                buffer.readonly = true;
                buffer.filetype = "help".to_string();
                buffer.buftype = "help".to_string();
            }
            let _ = self.options.set_buffer(
                id.raw(),
                "buftype",
                crate::options::OptionValue::String(VimString::from("help")),
            );
            let _ = self.options.set_buffer(
                id.raw(),
                "readonly",
                crate::options::OptionValue::Bool(true),
            );
            let _ = self.options.set_buffer(
                id.raw(),
                "modifiable",
                crate::options::OptionValue::Bool(false),
            );
            self.syntax.set_filetype("help");
            self.load_runtime_syntax();
        }
        let search = tag_search_pattern(&entry.command, name);
        let mut target = None;
        for lnum in 1..=self.line_count() {
            if let Some(line) = self.line(lnum) {
                if find_subslice(line, search.as_bytes()) {
                    target = Some(BufPos::new(lnum, 0));
                    break;
                }
            }
        }
        if target.is_none() {
            if let Ok(number) = entry.command.trim().parse::<u32>() {
                target = Some(BufPos::new(number, 0));
            }
        }
        if let Some(pos) = target {
            self.set_cursor(pos);
        }
        self.tag_stack.push((old_buffer, old_pos));
        self.messages.info(format!("{name}: {}", entry.command));
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn tag_select(&mut self, name: &str) -> Result<(), VimError> {
        let entries = self.tag_index.find_all(name).to_vec();
        if entries.is_empty() {
            return Err(VimError::new(433, format!("Tag not found: {name}")));
        }
        self.tag_matches = entries;
        self.tag_match_index = 0;
        let mut text = String::new();
        for (index, entry) in self.tag_matches.iter().enumerate() {
            text.push_str(&format!(
                "{} {} {}\n",
                index + 1,
                entry.file.display(),
                entry.command
            ));
        }
        self.messages.info(text.trim_end().to_string());
        Ok(())
    }

    pub(crate) fn tag_jump(&mut self, name: &str) -> Result<(), VimError> {
        let entries = self.tag_index.find_all(name).to_vec();
        if entries.is_empty() {
            return Err(VimError::new(433, format!("Tag not found: {name}")));
        }
        if entries.len() == 1 {
            self.tag_matches = entries;
            self.tag_match_index = 0;
            self.jump_to_tag_entry(self.tag_matches[0].clone(), name)
        } else {
            self.tag_select(name)
        }
    }

    pub(crate) fn tag_step(&mut self, delta: isize) -> Result<(), VimError> {
        if self.tag_matches.is_empty() {
            return Err(VimError::new(19, "No tags file"));
        }
        let next = self.tag_match_index as isize + delta;
        if next < 0 || next >= self.tag_matches.len() as isize {
            return Err(VimError::new(425, "tag stack is empty"));
        }
        self.tag_match_index = next as usize;
        let entry = self.tag_matches[self.tag_match_index].clone();
        let name = entry.name.clone();
        self.jump_to_tag_entry(entry, &name)
    }

    pub(crate) fn tag_first(&mut self) -> Result<(), VimError> {
        if self.tag_matches.is_empty() {
            return Err(VimError::new(19, "No tags file"));
        }
        self.tag_match_index = 0;
        let entry = self.tag_matches[0].clone();
        let name = entry.name.clone();
        self.jump_to_tag_entry(entry, &name)
    }

    pub(crate) fn tag_last(&mut self) -> Result<(), VimError> {
        if self.tag_matches.is_empty() {
            return Err(VimError::new(19, "No tags file"));
        }
        self.tag_match_index = self.tag_matches.len() - 1;
        let entry = self.tag_matches[self.tag_match_index].clone();
        let name = entry.name.clone();
        self.jump_to_tag_entry(entry, &name)
    }

    pub(crate) fn tag_back(&mut self) -> Result<(), VimError> {
        let Some((buffer, pos)) = self.tag_stack.pop() else {
            return Err(VimError::new(19, "No tags file"));
        };
        if buffer != self.current_buffer {
            self.switch_to_buffer(buffer)?;
        }
        self.set_cursor(pos);
        self.messages.info("tag stack popped");
        Ok(())
    }

    pub(crate) fn tag_stack_summary(&self) -> String {
        let mut out = String::from("  # TO tag\n");
        for (index, (buffer, pos)) in self.tag_stack.iter().enumerate().rev() {
            let name = self
                .buffers
                .get(*buffer)
                .map(|buffer| buffer.short_name())
                .unwrap_or_default();
            out.push_str(&format!(
                "{:>3}   {} {}:{}\n",
                index + 1,
                name,
                pos.lnum,
                pos.col + 1
            ));
        }
        out.trim_end().to_string()
    }

    /// Implements |K|: run 'keywordprg' for the word under the cursor.  A
    /// leading ':' makes it an Ex command; an empty value or ":he"/":help"
    /// opens the help window instead.
    fn keyword_lookup(&mut self) -> Result<(), VimError> {
        let Some(word) = self.keyword_under_cursor() else {
            return Err(VimError::new(348, "No string under cursor"));
        };
        let keywordprg = self.options.get_string("keywordprg");
        let keywordprg = keywordprg.trim();
        if keywordprg.is_empty() || keywordprg == ":he" || keywordprg == ":help" {
            self.execute_ex(&format!("help {word}"));
            return Ok(());
        }
        if let Some(ex) = keywordprg.strip_prefix(':') {
            self.execute_ex(&format!("{} {word}", ex.trim()));
            return Ok(());
        }
        let command = format!("{} {}", keywordprg, crate::ex::shell_quote(&word));
        match self.run_shell(&command, None) {
            Ok(output) => {
                let mut text = String::from_utf8_lossy(&output.stdout)
                    .trim_end()
                    .to_string();
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .trim_end()
                    .to_string();
                if !stderr.is_empty() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&stderr);
                }
                if !text.is_empty() {
                    self.messages.info(text);
                }
                self.request_redraw();
                Ok(())
            }
            Err(error) => Err(VimError::new(484, error.to_string())),
        }
    }

    pub(crate) fn keyword_under_cursor(&self) -> Option<String> {
        let pos = self.cursor();
        let line = self.current_buffer().and_then(|b| b.text.line(pos.lnum))?;
        let mut start = pos.col as usize;
        let mut end = start;
        while start > 0 && (line[start - 1].is_ascii_alphanumeric() || line[start - 1] == b'_') {
            start -= 1;
        }
        while end < line.len() && (line[end].is_ascii_alphanumeric() || line[end] == b'_') {
            end += 1;
        }
        if start == end {
            None
        } else {
            Some(String::from_utf8_lossy(&line[start..end]).into_owned())
        }
    }

    pub fn set_arglist(&mut self, paths: Vec<std::path::PathBuf>) {
        self.arglist = paths;
        self.arg_index = 0;
    }

    pub(crate) fn arg_next(&mut self, forward: bool, count: u32) -> Result<(), VimError> {
        if self.arglist.is_empty() {
            return Err(VimError::new(471, "Argument required"));
        }
        if self
            .current_buffer()
            .is_some_and(|buffer| buffer.modified())
        {
            return Err(VimError::new(
                37,
                "No write since last change (add ! to override)",
            ));
        }
        let len = self.arglist.len();
        let step = count.max(1) as usize;
        self.arg_index = if forward {
            (self.arg_index + step) % len
        } else {
            (self.arg_index + len - step % len) % len
        };
        let path = self.arglist[self.arg_index].clone();
        self.open_file(&path)?;
        self.messages
            .info(format!("[{}] {}", self.arg_index + 1, path.display()));
        Ok(())
    }

    pub(crate) fn args_summary(&self) -> String {
        let mut out = String::new();
        for (index, path) in self.arglist.iter().enumerate() {
            let marker = if index == self.arg_index { '>' } else { ' ' };
            out.push_str(&format!("{marker} {} {}\n", index + 1, path.display()));
        }
        out.trim_end().to_string()
    }

    pub(crate) fn arg_add(&mut self, path: std::path::PathBuf) {
        self.arglist.push(path);
    }

    pub(crate) fn buffer_ids(&self) -> Vec<BufferId> {
        self.buffers
            .iter()
            .map(|(raw, _)| BufferId::from_raw(raw))
            .collect()
    }

    pub(crate) fn switch_to_buffer(&mut self, id: BufferId) -> Result<(), VimError> {
        if !self.buffers.contains(id) {
            return Err(VimError::new(94, "No matching buffer"));
        }
        if id == self.current_buffer {
            return Ok(());
        }
        let old = self.current_buffer;
        let old_modified = self
            .buffers
            .get(old)
            .is_some_and(|buffer| buffer.modified());
        let bufhidden = self
            .options
            .buffer_string(rvim_types::Id::raw(old), "bufhidden");
        if old_modified && !self.options.get_bool("hidden") && bufhidden != "hide" {
            return Err(VimError::new(
                37,
                "No write since last change (add ! to override)",
            ));
        }
        self.fire_event(Event::BufLeave);
        self.alternate_buffer = Some(self.current_buffer);
        self.current_buffer = id;
        if let Some(window) = self.view.current_window_mut() {
            window.buffer = id;
            window.set_cursor(BufPos::start());
            window.viewport.topline = 1;
            window.viewport.leftcol = 0;
        }
        let old = self.alternate_buffer.unwrap_or(self.current_buffer);
        if old != id && !self.buffer_visible(old) {
            match self
                .options
                .buffer_string(rvim_types::Id::raw(old), "bufhidden")
                .as_str()
            {
                "unload" => {
                    let _ = self.unload_buffer(old, true, false);
                }
                "delete" => {
                    let _ = self.unload_buffer(old, true, true);
                }
                "wipe" => {
                    let _ = self.wipe_buffer(old, true);
                }
                "" if !old_modified && !self.options.get_bool("hidden") => {
                    let _ = self.unload_buffer(old, true, false);
                }
                _ => {}
            }
        }
        self.fire_event(Event::BufEnter);
        self.request_redraw();
        Ok(())
    }

    fn buffer_visible(&self, id: BufferId) -> bool {
        self.view.windows.values().any(|window| window.buffer == id)
    }

    pub(crate) fn execute_list_command(
        &mut self,
        kind: crate::editor::ListDoCommand,
        command: &str,
    ) -> Result<(), VimError> {
        if command.is_empty() {
            return Err(VimError::new(471, "Argument required"));
        }
        match kind {
            crate::editor::ListDoCommand::Buffers => {
                let original = self.current_buffer;
                let ids = self.listed_buffer_ids();
                for id in ids {
                    if !self.buffers.contains(id) {
                        continue;
                    }
                    if self.switch_to_buffer(id).is_err() {
                        continue;
                    }
                    self.execute_ex(command);
                }
                if self.buffers.contains(original) {
                    self.current_buffer = original;
                    self.update_window_buffer();
                    self.sync_window_options();
                }
            }
            crate::editor::ListDoCommand::Windows => {
                let original = self.view.current_window().map(|window| window.id);
                let ids = self.view.window_ids_in_current_tab();
                for id in ids {
                    if !self.view.windows.contains_key(&id) {
                        continue;
                    }
                    self.view.set_current_window(id);
                    self.sync_current_buffer_to_window();
                    self.sync_window_options();
                    self.execute_ex(command);
                }
                if let Some(original) = original {
                    if self.view.windows.contains_key(&original) {
                        self.view.set_current_window(original);
                        self.sync_current_buffer_to_window();
                        self.sync_window_options();
                    }
                }
            }
            crate::editor::ListDoCommand::Tabs => {
                let original = self.view.current_tab;
                let ids = self.view.tab_ids();
                for id in ids {
                    if !self.view.tabs.contains_key(&id) {
                        continue;
                    }
                    self.view.set_tab(id);
                    self.sync_current_buffer_to_window();
                    self.sync_window_options();
                    self.execute_ex(command);
                }
                if self.view.tabs.contains_key(&original) {
                    self.view.set_tab(original);
                    self.sync_current_buffer_to_window();
                    self.sync_window_options();
                }
            }
            crate::editor::ListDoCommand::Arguments => {
                let original = self.current_buffer;
                let args = self.arglist.clone();
                for path in args {
                    self.open_file(&path)?;
                    self.execute_ex(command);
                }
                if self.buffers.contains(original) {
                    self.current_buffer = original;
                    self.update_window_buffer();
                    self.sync_window_options();
                }
            }
            crate::editor::ListDoCommand::Quickfix
            | crate::editor::ListDoCommand::QuickfixFiles
            | crate::editor::ListDoCommand::Loclist
            | crate::editor::ListDoCommand::LoclistFiles => {
                let (entries, files_only) = match kind {
                    crate::editor::ListDoCommand::Quickfix => {
                        (self.quickfix.entries.clone(), false)
                    }
                    crate::editor::ListDoCommand::QuickfixFiles => {
                        (self.quickfix.entries.clone(), true)
                    }
                    crate::editor::ListDoCommand::Loclist => (self.loclist_entries(), false),
                    crate::editor::ListDoCommand::LoclistFiles => (self.loclist_entries(), true),
                    _ => (Vec::new(), false),
                };
                let mut seen: Vec<std::path::PathBuf> = Vec::new();
                for entry in entries {
                    let path = if entry.file.is_absolute() {
                        entry.file.clone()
                    } else {
                        pfs::cwd().join(&entry.file)
                    };
                    if files_only {
                        if seen.contains(&path) {
                            continue;
                        }
                        seen.push(path.clone());
                    }
                    let existing = self.buffer_ids().into_iter().find(|id| {
                        self.buffers.get(*id).is_some_and(|buffer| {
                            buffer.name_string() == path.display().to_string()
                        })
                    });
                    let id = match existing {
                        Some(id) => id,
                        None => self.create_buffer(Some(&path))?,
                    };
                    self.switch_to_buffer(id)?;
                    self.set_cursor(BufPos::new(entry.lnum.max(1), entry.col.saturating_sub(1)));
                    self.execute_ex(command);
                }
            }
        }
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn buffer_first(&mut self) -> Result<(), VimError> {
        let ids = self.listed_buffer_ids();
        if let Some(id) = ids.first().copied() {
            self.switch_to_buffer(id)?;
        }
        Ok(())
    }

    pub(crate) fn buffer_last(&mut self) -> Result<(), VimError> {
        let ids = self.listed_buffer_ids();
        if let Some(id) = ids.last().copied() {
            self.switch_to_buffer(id)?;
        }
        Ok(())
    }

    pub(crate) fn buffer_modified(&mut self) -> Result<(), VimError> {
        let ids = self.listed_buffer_ids();
        if ids.is_empty() {
            return Err(VimError::new(94, "No matching buffer"));
        }
        let start = ids
            .iter()
            .position(|id| *id == self.current_buffer)
            .unwrap_or(0);
        for offset in 1..=ids.len() {
            let id = ids[(start + offset) % ids.len()];
            if self.buffers.get(id).is_some_and(|buffer| buffer.modified()) {
                self.switch_to_buffer(id)?;
                return Ok(());
            }
        }
        Err(VimError::new(94, "No modified buffer"))
    }

    fn listed_buffer_ids(&self) -> Vec<BufferId> {
        self.buffer_ids()
            .into_iter()
            .filter(|id| self.buffers.get(*id).is_some_and(|buffer| buffer.listed))
            .collect()
    }

    pub(crate) fn next_buffer(&mut self, forward: bool) -> Result<(), VimError> {
        let ids = self.buffer_ids();
        if ids.len() <= 1 {
            return Ok(());
        }
        let index = ids
            .iter()
            .position(|id| *id == self.current_buffer)
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % ids.len()
        } else {
            (index + ids.len() - 1) % ids.len()
        };
        self.switch_to_buffer(ids[next])
    }

    pub(crate) fn buffer_by_number(&self, number: u32) -> Option<BufferId> {
        self.buffer_ids()
            .get(number.saturating_sub(1) as usize)
            .copied()
    }

    pub(crate) fn unload_buffer(
        &mut self,
        id: BufferId,
        force: bool,
        unlist: bool,
    ) -> Result<(), VimError> {
        let Some(buffer) = self.buffers.get(id) else {
            return Err(VimError::new(94, "No matching buffer"));
        };
        if buffer.modified() && !force {
            return Err(VimError::new(
                89,
                "No write since last change for buffer (add ! to override)",
            ));
        }
        let previous = self.current_buffer;
        self.current_buffer = id;
        self.fire_event(Event::BufDelete);
        self.fire_event(Event::BufUnload);
        self.current_buffer = previous;
        self.swaps.remove(id);
        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.listed = !unlist;
            buffer.loaded = false;
        }
        self.detach_buffer_from_windows(id);
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn wipe_buffer(&mut self, id: BufferId, force: bool) -> Result<(), VimError> {
        let Some(buffer) = self.buffers.get(id) else {
            return Err(VimError::new(94, "No matching buffer"));
        };
        if buffer.modified() && !force {
            return Err(VimError::new(
                89,
                "No write since last change for buffer (add ! to override)",
            ));
        }
        if self.buffers.len() <= 1 {
            return Err(VimError::new(85, "You can't delete the last buffer"));
        }
        let previous = self.current_buffer;
        self.current_buffer = id;
        self.fire_event(Event::BufDelete);
        self.fire_event(Event::BufUnload);
        self.fire_event(Event::BufWipeout);
        self.current_buffer = previous;
        self.swaps.remove(id);
        self.detach_buffer_from_windows(id);
        self.buffers.remove(id);
        self.request_redraw();
        Ok(())
    }

    fn detach_buffer_from_windows(&mut self, id: BufferId) {
        let next = self
            .buffer_ids()
            .into_iter()
            .find(|candidate| *candidate != id)
            .unwrap_or(self.current_buffer);
        let affected: Vec<_> = self
            .view
            .windows
            .iter()
            .filter(|(_, window)| window.buffer == id)
            .map(|(window, _)| window)
            .collect();
        for window_id in affected {
            if let Some(window) = self.view.window_mut(window_id) {
                window.buffer = next;
                window.set_cursor(BufPos::start());
            }
        }
        if self.current_buffer == id {
            self.current_buffer = next;
        }
    }

    pub(crate) fn load_buffer(&mut self, id: BufferId) -> Result<(), VimError> {
        let Some(buffer) = self.buffers.get(id) else {
            return Err(VimError::new(94, "No matching buffer"));
        };
        if buffer.loaded {
            return Ok(());
        }
        let Some(path) = buffer.text.meta.name.clone() else {
            return Ok(());
        };
        let bytes = rvim_platform::fs::read_file(&path)
            .map_err(|error| VimError::new(13, error.to_string()))?;
        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.load_bytes(&bytes);
            buffer.loaded = true;
            buffer.listed = true;
            buffer.set_modified(false);
        }
        self.load_undo_file(id);
        Ok(())
    }

    pub(crate) fn add_unlisted_buffer(
        &mut self,
        path: Option<&Path>,
    ) -> Result<BufferId, VimError> {
        let id = self.create_buffer(None)?;
        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.listed = false;
            buffer.loaded = false;
            if let Some(path) = path {
                let path = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    pfs::cwd().join(path)
                };
                buffer.set_name(path);
            }
        }
        self.fire_event(Event::BufAdd);
        Ok(id)
    }

    pub(crate) fn buffer_summary(&self, all: bool) -> String {
        let mut out = String::new();
        for (index, id) in self.buffer_ids().iter().enumerate() {
            let Some(buffer) = self.buffers.get(*id) else {
                continue;
            };
            if !all && !buffer.listed {
                continue;
            }
            let marker = if *id == self.current_buffer {
                '%'
            } else if Some(*id) == self.alternate_buffer {
                '#'
            } else {
                ' '
            };
            let modified = if buffer.modified() { '+' } else { ' ' };
            let name = if buffer.name_string().is_empty() {
                "[No Name]".to_string()
            } else {
                buffer.name_string()
            };
            out.push_str(&format!("{index:3} {marker}{modified} {name}\n"));
        }
        out.trim_end().to_string()
    }

    pub fn tabline_text(&self) -> Option<String> {
        let show = self.options.get_number("showtabline");
        if self.view.tab_count() <= 1 && show < 2 {
            return None;
        }
        if show == 0 {
            return None;
        }
        let custom = self.options.get_string("tabline");
        if !custom.is_empty() {
            return Some(self.expand_tabline_template_with_clicks(&custom).0);
        }
        let mut out = String::new();
        for (index, tab) in self.view.tab_ids().iter().enumerate() {
            let number = index + 1;
            let name = self
                .view
                .tabs
                .get(tab)
                .and_then(|tab| self.view.windows.get(&tab.current))
                .and_then(|window| self.buffers.get(window.buffer))
                .map(|buffer| {
                    let name = buffer.short_name();
                    if name.is_empty() {
                        "[No Name]".to_string()
                    } else {
                        name
                    }
                })
                .unwrap_or_else(|| "[No Name]".to_string());
            out.push_str(&format!("[{number}] {name} "));
        }
        Some(out)
    }

    pub fn tabline_regions(&self) -> Vec<(u16, u16, u64)> {
        let Some(text) = self.tabline_text() else {
            return Vec::new();
        };
        let tabs = self.view.tab_ids();
        let mut regions = Vec::new();
        for (index, tab) in tabs.iter().enumerate() {
            let marker = format!("[{}]", index + 1);
            let Some(start_byte) = text.find(&marker) else {
                continue;
            };
            let start = text[..start_byte].chars().count() as u16;
            let next = tabs
                .iter()
                .skip(index + 1)
                .enumerate()
                .find_map(|(offset, _)| text.find(&format!("[{}]", index + 2 + offset)));
            let end_byte = next.unwrap_or(text.len());
            let end = text[..end_byte].chars().count() as u16;
            regions.push((start, end.max(start + 1), rvim_types::Id::raw(*tab)));
        }
        regions
    }

    pub(crate) fn tabline_clicks(&self) -> Vec<TablineClick> {
        let show = self.options.get_number("showtabline");
        if show == 0 || (self.view.tab_count() <= 1 && show < 2) {
            return Vec::new();
        }
        let custom = self.options.get_string("tabline");
        if custom.is_empty() {
            return Vec::new();
        }
        self.expand_tabline_template_with_clicks(&custom).1
    }

    fn expand_tabline_template_with_clicks(&self, template: &str) -> (String, Vec<TablineClick>) {
        let tabs = self.view.tab_ids();
        let current_name = self
            .current_buffer()
            .map(|buffer| buffer.short_name())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "[No Name]".to_string());
        let mut out = String::new();
        let mut col = 0u16;
        let mut clicks = Vec::new();
        let mut pending: Option<(u16, String, u32)> = None;
        let mut chars = template.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                let mut buffer = [0u8; 4];
                let text = ch.encode_utf8(&mut buffer);
                out.push_str(text);
                col = col.saturating_add(rvim_types::unicode::char_width(ch).max(1) as u16);
                continue;
            }
            match chars.next() {
                Some('%') => {
                    out.push('%');
                    col = col.saturating_add(1);
                }
                Some('t') | Some('f') => {
                    out.push_str(&current_name);
                    col = col.saturating_add(
                        current_name
                            .chars()
                            .map(|ch| rvim_types::unicode::char_width(ch).max(1) as u16)
                            .sum::<u16>(),
                    );
                }
                Some('F') => {
                    let name = self
                        .current_buffer()
                        .and_then(|buffer| buffer.text.meta.name.as_ref())
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "[No Name]".to_string());
                    out.push_str(&name);
                    col = col.saturating_add(
                        name.chars()
                            .map(|ch| rvim_types::unicode::char_width(ch).max(1) as u16)
                            .sum::<u16>(),
                    );
                }
                Some('m') => {
                    if self
                        .current_buffer()
                        .is_some_and(|buffer| buffer.modified())
                    {
                        out.push_str("[+]");
                        col = col.saturating_add(3);
                    }
                }
                Some('=') => {}
                Some('#') => {
                    for next in chars.by_ref() {
                        if next == '#' {
                            break;
                        }
                    }
                }
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
                        close_tabline_click(&mut pending, col, &mut clicks);
                        if !function.is_empty() {
                            pending = Some((col, function, 0));
                        }
                    }
                }
                Some(digit) if digit.is_ascii_digit() => {
                    let mut number = digit.to_digit(10).unwrap_or(0);
                    while let Some(next) = chars.peek() {
                        if let Some(value) = next.to_digit(10) {
                            number = number * 10 + value;
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if chars.peek() == Some(&'[') {
                        chars.next();
                        let mut function = String::new();
                        let mut terminated = false;
                        for next in chars.by_ref() {
                            if next == ']' {
                                terminated = true;
                                break;
                            }
                            function.push(next);
                        }
                        if terminated && !function.is_empty() {
                            close_tabline_click(&mut pending, col, &mut clicks);
                            pending = Some((col, function, number));
                        }
                    } else if chars.peek() == Some(&'T') {
                        chars.next();
                        if let Some(tab) = tabs.get(number.saturating_sub(1) as usize) {
                            let name = self
                                .view
                                .tabs
                                .get(tab)
                                .and_then(|page| self.view.windows.get(&page.current))
                                .and_then(|window| self.buffers.get(window.buffer))
                                .map(|buffer| {
                                    let name = buffer.short_name();
                                    if name.is_empty() {
                                        "[No Name]".to_string()
                                    } else {
                                        name
                                    }
                                })
                                .unwrap_or_else(|| "[No Name]".to_string());
                            let label = format!("[{number}] {name} ");
                            col = col.saturating_add(
                                label
                                    .chars()
                                    .map(|ch| rvim_types::unicode::char_width(ch).max(1) as u16)
                                    .sum::<u16>(),
                            );
                            out.push_str(&label);
                        }
                    }
                }
                Some(other) => {
                    out.push(other);
                    col = col.saturating_add(1);
                }
                None => {
                    out.push('%');
                    col = col.saturating_add(1);
                }
            }
        }
        close_tabline_click(&mut pending, col, &mut clicks);
        (out, clicks)
    }

    pub(crate) fn create_buffer(&mut self, path: Option<&Path>) -> Result<BufferId, VimError> {
        let id = self.buffers.insert(BufferEntry::new(BufferId::default()));
        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer.id = id;
        }
        if let Some(path) = path {
            let path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                pfs::cwd().join(path)
            };
            let bytes = match pfs::read_file(&path) {
                Ok(bytes) => bytes,
                Err(error) if error.is_not_found() => Vec::new(),
                Err(error) => return Err(VimError::new(13, error.to_string())),
            };
            if let Some(buffer) = self.buffers.get_mut(id) {
                buffer.set_name(path);
                buffer.load_bytes(&bytes);
                buffer.set_modified(false);
            }
        }
        Ok(id)
    }

    pub(crate) fn new_empty_buffer(&mut self) -> Result<(), VimError> {
        let id = self.create_buffer(None)?;
        self.current_buffer = id;
        if let Some(window) = self.view.current_window_mut() {
            window.buffer = id;
            window.set_cursor(BufPos::start());
        }
        self.sync_window_options();
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn split_window(
        &mut self,
        dir: SplitDir,
        path: Option<&Path>,
    ) -> Result<(), VimError> {
        let buffer = match path {
            Some(path) => self.create_buffer(Some(path))?,
            None => self.current_buffer,
        };
        let after = if dir == SplitDir::Horizontal {
            self.options.get_bool("splitbelow")
        } else {
            self.options.get_bool("splitright")
        };
        let Some(window) = self.view.split_ordered(buffer, dir, after) else {
            return Err(VimError::plain("E36: Not enough room"));
        };
        let _ = window;
        if self.options.get_bool("equalalways") {
            self.view.equalize_windows();
        }
        self.current_buffer = buffer;
        self.sync_window_options();
        self.update_window_buffer();
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn close_window_or_error(&mut self) -> Result<(), VimError> {
        if self
            .current_buffer()
            .is_some_and(|buffer| buffer.modified())
        {
            return Err(VimError::new(
                37,
                "No write since last change (add ! to override)",
            ));
        }
        if !self.view.close_current_window() {
            return Err(VimError::new(444, "Cannot close last window"));
        }
        if let Some(window) = self.view.current_window() {
            self.current_buffer = window.buffer;
        }
        if self.options.get_bool("equalalways") {
            self.view.equalize_windows();
        }
        self.sync_window_options();
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn only_window(&mut self) {
        self.view.only_current_window();
        self.request_redraw();
    }

    pub(crate) fn cycle_window(&mut self, forward: bool) {
        let ids = self.view.window_ids_in_current_tab();
        if ids.is_empty() {
            return;
        }
        let current = self.view.current_tab().map(|tab| tab.current);
        let index = current
            .and_then(|current| ids.iter().position(|id| *id == current))
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % ids.len()
        } else {
            (index + ids.len() - 1) % ids.len()
        };
        self.view.set_current_window(ids[next]);
        if let Some(window) = self.view.current_window() {
            self.current_buffer = window.buffer;
        }
        self.sync_window_options();
        self.request_redraw();
    }

    pub(crate) fn tab_new(&mut self, path: Option<&Path>) -> Result<(), VimError> {
        let buffer = self.create_buffer(path)?;
        self.view.new_tab(buffer);
        self.current_buffer = buffer;
        self.sync_window_options();
        self.update_window_buffer();
        self.fire_event(Event::TabNew);
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn tab_next(&mut self) {
        self.fire_event(Event::TabLeave);
        self.view.next_tab();
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.fire_event(Event::TabEnter);
        self.request_redraw();
    }

    pub(crate) fn tab_previous(&mut self) {
        self.fire_event(Event::TabLeave);
        self.view.previous_tab();
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.fire_event(Event::TabEnter);
        self.request_redraw();
    }

    pub(crate) fn tab_only(&mut self, force: bool) -> Result<(), VimError> {
        if !force {
            let current = self.view.current_tab;
            let modified =
                self.view
                    .tabs
                    .iter()
                    .filter(|(id, _)| *id != current)
                    .any(|(_, page)| {
                        rvim_view::layout::all_windows(&page.root)
                            .into_iter()
                            .any(|window| {
                                self.view
                                    .windows
                                    .get(&window)
                                    .and_then(|window| self.buffers.get(window.buffer))
                                    .is_some_and(|buffer| buffer.modified())
                            })
                    });
            if modified {
                return Err(VimError::new(445, "Other window contains changes"));
            }
        }
        if !self.view.only_current_tab() {
            return Err(VimError::new(444, "Cannot close last tab page"));
        }
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn tab_close(&mut self, force: bool) -> Result<(), VimError> {
        if !force
            && self
                .current_buffer()
                .is_some_and(|buffer| buffer.modified())
        {
            return Err(VimError::new(
                37,
                "No write since last change (add ! to override)",
            ));
        }
        self.fire_event(Event::TabLeave);
        if !self.view.close_current_tab() {
            return Err(VimError::new(444, "Cannot close last tab page"));
        }
        self.fire_event(Event::TabClosed);
        self.sync_current_buffer_to_window();
        self.sync_window_options();
        self.fire_event(Event::TabEnter);
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn sync_current_buffer_to_window(&mut self) {
        if let Some(window) = self.view.current_window() {
            self.current_buffer = window.buffer;
        }
    }

    fn dispatch_click(&mut self, click: &ClickRegion, nclicks: u8, mouse: &MouseEvent) {
        let mut info = rvim_viml::VimDict::new();
        info.set(
            VimString::from("minwid"),
            rvim_viml::Value::Number(click.minwid as i64),
        );
        info.set(
            VimString::from("nclicks"),
            rvim_viml::Value::Number(nclicks as i64),
        );
        info.set(
            VimString::from("button"),
            rvim_viml::Value::string(match mouse.button {
                MouseButton::Left => "l",
                MouseButton::Middle => "m",
                MouseButton::Right => "r",
                _ => "l",
            }),
        );
        let mut mods = String::new();
        if mouse.mods.contains(Mods::SHIFT) {
            mods.push('s');
        }
        if mouse.mods.contains(Mods::CTRL) {
            mods.push('c');
        }
        if mouse.mods.contains(Mods::ALT) {
            mods.push('a');
        }
        info.set(VimString::from("mods"), rvim_viml::Value::string(mods));
        info.set(
            VimString::from("winid"),
            rvim_viml::Value::Number(click.winid as i64),
        );
        info.set(
            VimString::from("area"),
            rvim_viml::Value::string(match click.area {
                ClickArea::Statusline => "statusline",
                ClickArea::Tabline => "tabline",
                ClickArea::Tabpanel => "tabpanel",
            }),
        );
        info.set(
            VimString::from("tabnr"),
            rvim_viml::Value::Number(click.tabnr as i64),
        );
        match self.call_viml_function(&click.function, vec![rvim_viml::Value::dict(info)]) {
            Ok(value) => {
                if value.as_number().unwrap_or(0) != 0 {
                    self.request_redraw();
                }
            }
            Err(error) => self.set_error(error),
        }
    }

    fn scroll_window_display(&mut self, window_id: WindowId, down: bool, count: u32) {
        let Some(window) = self.view.windows.get(&window_id) else {
            return;
        };
        let buffer_id = window.buffer;
        let options = window.options.clone();
        let width = window.text_width();
        let mut topline = window.viewport.topline;
        let mut skipcol = window.viewport.skipcol as usize;
        let Some(buffer) = self.buffers.get(buffer_id) else {
            return;
        };
        let max_top = buffer.text.line_count().max(1);
        let smooth = options.smoothscroll && options.wrap;
        for _ in 0..count.max(1) {
            if !smooth {
                if down {
                    topline = (topline + 1).min(max_top);
                } else {
                    topline = topline.saturating_sub(1).max(1);
                }
                skipcol = 0;
                continue;
            }
            let Some(line) = buffer.text.line(topline) else {
                break;
            };
            let chunks = rvim_view::display_chunks(line, width, &options);
            let starts: Vec<usize> = chunks
                .iter()
                .map(|(start, _)| {
                    rvim_types::unicode::display_width(&line[..*start], 0, options.tabstop.max(1))
                })
                .collect();
            let index = starts
                .iter()
                .rposition(|start| *start <= skipcol)
                .unwrap_or(0);
            if down {
                if let Some(next) = starts.get(index + 1) {
                    skipcol = *next;
                } else if topline < max_top {
                    topline += 1;
                    skipcol = 0;
                }
            } else if index > 0 {
                skipcol = starts[index - 1];
            } else if topline > 1 {
                topline -= 1;
                skipcol = buffer
                    .text
                    .line(topline)
                    .map(|line| {
                        rvim_view::display_chunks(line, width, &options)
                            .iter()
                            .map(|(start, _)| {
                                rvim_types::unicode::display_width(
                                    &line[..*start],
                                    0,
                                    options.tabstop.max(1),
                                )
                            })
                            .next_back()
                            .unwrap_or(0)
                    })
                    .unwrap_or(0);
            }
        }
        if let Some(window) = self.view.window_mut(window_id) {
            window.viewport.topline = topline;
            window.viewport.skipcol = skipcol as u32;
        }
    }

    fn screen_to_buffer_pos(
        &self,
        window_id: WindowId,
        row: u16,
        text_col: u16,
        line_count: u32,
    ) -> BufPos {
        let Some(window) = self.view.windows.get(&window_id) else {
            return self.cursor();
        };
        let options = window.options.clone();
        let topline = window.viewport.topline;
        let leftcol = window.viewport.leftcol;
        let skipcol = window.viewport.skipcol as usize;
        let width = window.text_width();
        let Some(buffer) = self.buffers.get(window.buffer) else {
            return self.cursor();
        };
        let tabstop = options.tabstop.max(1);
        if !options.wrap {
            let lnum = (topline + row as u32).min(line_count.max(1));
            let display_col = leftcol + text_col as u32;
            let col = rvim_edit::byte_col(&buffer.text, lnum, display_col, tabstop);
            return BufPos::new(lnum, col);
        }
        let mut lnum = topline;
        let mut remaining = row as u32;
        while lnum <= line_count.max(1) {
            let line = buffer.text.line(lnum).unwrap_or(&[]);
            let chunks = rvim_view::display_chunks(line, width, &options);
            let skipped = if lnum == topline && skipcol > 0 && !chunks.is_empty() {
                chunks
                    .iter()
                    .position(|(start, _)| {
                        rvim_types::unicode::display_width(&line[..*start], 0, tabstop) >= skipcol
                    })
                    .unwrap_or(chunks.len().saturating_sub(1))
            } else {
                0
            };
            let visible = chunks.len().saturating_sub(skipped);
            if remaining < visible as u32 {
                let index = skipped + remaining as usize;
                let (start, _) = chunks[index];
                let prefix = if index > skipped {
                    rvim_view::display_prefix_width(line, width, &options) as u32
                } else {
                    0
                };
                let chunk_start = rvim_types::unicode::display_width(&line[..start], 0, tabstop);
                let display_col =
                    (chunk_start + (text_col as usize).saturating_sub(prefix as usize)) as u32;
                let col = rvim_edit::byte_col(&buffer.text, lnum, display_col, tabstop);
                return BufPos::new(lnum, col);
            }
            remaining -= visible as u32;
            lnum += 1;
        }
        BufPos::new(line_count.max(1), 0)
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        self.last_mouse = Some((mouse.row, mouse.col));
        if mouse.kind == MouseKind::Release {
            self.border_drag = None;
        }
        if let Some(drag) = self.border_drag {
            if mouse.kind == MouseKind::Drag {
                let delta = if drag.vertical {
                    mouse.col as i32 - drag.last_col as i32
                } else {
                    mouse.row as i32 - drag.last_row as i32
                };
                if delta != 0 && self.view.resize_window(drag.window, drag.vertical, delta) {
                    self.border_drag = Some(BorderDrag {
                        window: drag.window,
                        vertical: drag.vertical,
                        last_row: mouse.row,
                        last_col: mouse.col,
                    });
                    self.sync_window_options();
                    self.request_redraw();
                }
                return;
            }
        }
        let click_count = if mouse.kind == MouseKind::Press {
            let now = Instant::now();
            let count = match self.last_click {
                Some((when, row, col, button, count))
                    if button == mouse.button
                        && row == mouse.row
                        && col == mouse.col
                        && now.duration_since(when) <= Duration::from_millis(500) =>
                {
                    (count % 3) + 1
                }
                _ => 1,
            };
            self.last_click = Some((now, mouse.row, mouse.col, mouse.button, count));
            count
        } else {
            1
        };
        let click = self.view.last_frame.as_ref().and_then(|frame| {
            frame
                .clicks
                .iter()
                .find(|click| click.rect.contains(mouse.row, mouse.col))
                .cloned()
        });
        if let Some(click) = click {
            if mouse.kind == MouseKind::Press {
                self.dispatch_click(&click, click_count, &mouse);
                return;
            }
        }
        let line_count = self.line_count().max(1);
        let frame_action = self.view.last_frame.as_ref().and_then(|frame| {
            frame
                .hit_regions
                .iter()
                .find(|region| region.rect.contains(mouse.row, mouse.col))
                .cloned()
        });
        if let Some(region) = &frame_action {
            match region.action.kind {
                ActionKind::Window => {
                    let target_window = {
                        self.view
                            .windows
                            .iter()
                            .find(|(_, window)| rvim_types::Id::raw(window.id) == region.action.id)
                            .map(|(id, _)| id)
                    };
                    if let Some(window) = target_window {
                        self.view.set_current_window(window);
                        self.current_buffer = self
                            .view
                            .current_window()
                            .map(|window| window.buffer)
                            .unwrap_or(self.current_buffer);
                        self.sync_window_options();
                    }
                }
                ActionKind::Statusline => {
                    let target_window = {
                        self.view
                            .windows
                            .iter()
                            .find(|(_, window)| rvim_types::Id::raw(window.id) == region.action.id)
                            .map(|(id, _)| id)
                    };
                    if let Some(window) = target_window {
                        self.view.set_current_window(window);
                        self.current_buffer = self
                            .view
                            .current_window()
                            .map(|window| window.buffer)
                            .unwrap_or(self.current_buffer);
                        self.sync_window_options();
                        self.request_redraw();
                    }
                    return;
                }
                ActionKind::WindowBorder => {
                    let target_window = {
                        self.view
                            .windows
                            .iter()
                            .find(|(_, window)| rvim_types::Id::raw(window.id) == region.action.id)
                            .map(|(id, _)| id)
                    };
                    if let Some(window) = target_window {
                        self.view.set_current_window(window);
                        self.current_buffer = self
                            .view
                            .current_window()
                            .map(|window| window.buffer)
                            .unwrap_or(self.current_buffer);
                        self.sync_window_options();
                        if mouse.kind == MouseKind::Press {
                            let vertical = self
                                .view
                                .layout_rects(0, 0)
                                .iter()
                                .find(|rect| rvim_types::Id::raw(rect.window) == region.action.id)
                                .and_then(|rect| rect.separator)
                                .map(|separator| separator.cols == 1)
                                .unwrap_or(true);
                            self.border_drag = Some(BorderDrag {
                                window,
                                vertical,
                                last_row: mouse.row,
                                last_col: mouse.col,
                            });
                        }
                        self.request_redraw();
                    }
                    return;
                }
                ActionKind::Tab => {
                    let target_tab = {
                        self.view
                            .tabs
                            .keys()
                            .find(|tab| rvim_types::Id::raw(*tab) == region.action.id)
                    };
                    if let Some(tab) = target_tab {
                        self.view.set_tab(tab);
                        self.sync_current_buffer_to_window();
                        self.sync_window_options();
                        self.request_redraw();
                    }
                    return;
                }
                ActionKind::Fold => {
                    let target_window = self
                        .view
                        .windows
                        .iter()
                        .find(|(_, window)| rvim_types::Id::raw(window.id) == region.action.id)
                        .map(|(id, _)| id);
                    if let Some(window) = target_window {
                        self.view.set_current_window(window);
                        self.current_buffer = self
                            .view
                            .current_window()
                            .map(|window| window.buffer)
                            .unwrap_or(self.current_buffer);
                        self.sync_window_options();
                    }
                    if mouse.kind == MouseKind::Press {
                        let lnum = region.action.index;
                        self.fold_toggle(lnum);
                    }
                    self.request_redraw();
                    return;
                }
                ActionKind::Sign => {
                    let target_window = self
                        .view
                        .windows
                        .iter()
                        .find(|(_, window)| rvim_types::Id::raw(window.id) == region.action.id)
                        .map(|(id, _)| id);
                    if let Some(window) = target_window {
                        self.view.set_current_window(window);
                        self.current_buffer = self
                            .view
                            .current_window()
                            .map(|window| window.buffer)
                            .unwrap_or(self.current_buffer);
                        self.sync_window_options();
                    }
                    if mouse.kind == MouseKind::Press {
                        let lnum = region.action.index;
                        self.set_cursor(BufPos::new(lnum.max(1), 0));
                        self.request_redraw();
                    }
                    return;
                }
                _ => {}
            }
        }
        let rects = self.view.layout_rects(0, 0);
        let Some(hit) = rects
            .iter()
            .find(|rect| rect.text.contains(mouse.row, mouse.col))
            .copied()
        else {
            return;
        };
        self.view.set_current_window(hit.window);
        self.sync_current_buffer_to_window();
        if mouse.kind == MouseKind::Wheel {
            match mouse.button {
                MouseButton::WheelUp => self.scroll_window_display(hit.window, false, 3),
                MouseButton::WheelDown => self.scroll_window_display(hit.window, true, 3),
                _ => {}
            }
            self.request_redraw();
            return;
        }
        if mouse.kind != MouseKind::Press || mouse.button != MouseButton::Left {
            return;
        }
        let number_width = self
            .view
            .windows
            .get(&hit.window)
            .map(|window| {
                if window.options.number || window.options.relativenumber {
                    window.options.numberwidth as u16 + 1
                } else {
                    0
                }
            })
            .unwrap_or(0);
        let text_col = mouse.col.saturating_sub(hit.text.col + number_width);
        let pos = self.screen_to_buffer_pos(
            hit.window,
            mouse.row.saturating_sub(hit.text.row),
            text_col,
            line_count,
        );
        self.set_cursor(pos);
        self.sync_window_options();
        self.request_redraw();
    }

    pub(crate) fn resize_current_window_public(&mut self, vertical: bool, delta: i32) {
        self.resize_current_window(vertical, delta);
    }

    fn resize_current_window(&mut self, vertical: bool, delta: i32) {
        let Some(window) = self.view.current_window() else {
            return;
        };
        if (vertical && window.options.winfixwidth) || (!vertical && window.options.winfixheight) {
            return;
        }
        let id = window.id;
        let current_size = self
            .view
            .layout_rects(0, 0)
            .into_iter()
            .find(|rect| rect.window == id)
            .map(|rect| {
                if vertical {
                    rect.text.cols
                } else {
                    rect.text.rows
                }
            })
            .unwrap_or(1);
        let minimum = if vertical {
            self.options.get_number("winwidth").max(1) as u16
        } else {
            self.options.get_number("winheight").max(1) as u16
        };
        let delta = if delta < 0 {
            let current = current_size as i32;
            let next = (current + delta).max(minimum as i32);
            next - current
        } else {
            delta
        };
        if delta == 0 {
            return;
        }
        if self.view.resize_window(id, vertical, delta) {
            self.request_redraw();
        }
    }

    fn move_window_direction(&mut self, direction: char) {
        let rects = self.view.layout_rects(0, 0);
        let Some(current_id) = self.view.current_tab().map(|tab| tab.current) else {
            return;
        };
        let Some(current) = rects.iter().find(|rect| rect.window == current_id).copied() else {
            return;
        };
        let candidate = rects
            .iter()
            .filter(|rect| rect.window != current_id)
            .filter(|rect| match direction {
                'h' => rect.text.right() <= current.text.col,
                'l' => rect.text.col >= current.text.right(),
                'k' => rect.text.bottom() <= current.text.row,
                'j' => rect.text.row >= current.text.bottom(),
                _ => false,
            })
            .min_by_key(|rect| match direction {
                'h' => current.text.col.saturating_sub(rect.text.right()),
                'l' => rect.text.col.saturating_sub(current.text.right()),
                'k' => current.text.row.saturating_sub(rect.text.bottom()),
                'j' => rect.text.row.saturating_sub(current.text.bottom()),
                _ => 0,
            });
        if let Some(candidate) = candidate {
            self.view.set_current_window(candidate.window);
            self.sync_current_buffer_to_window();
            self.sync_window_options();
            self.request_redraw();
        }
    }

    fn handle_ctrl_w_key(&mut self, key: KeyEvent) {
        self.pending_ctrl_w = false;
        let Key::Char(ch) = key.key else {
            return;
        };
        match ch {
            '+' => self.resize_current_window(false, 1),
            '-' => self.resize_current_window(false, -1),
            '>' => self.resize_current_window(true, 1),
            '<' => self.resize_current_window(true, -1),
            'h' => self.move_window_direction('h'),
            'j' => self.move_window_direction('j'),
            'k' => self.move_window_direction('k'),
            'l' => self.move_window_direction('l'),
            'w' => self.cycle_window(true),
            'W' => self.cycle_window(false),
            'c' | 'q' => {
                if let Err(error) = self.close_window_or_error() {
                    self.set_error(error);
                }
            }
            'o' => self.only_window(),
            '}' => {
                if let Some(keyword) = self.keyword_under_cursor() {
                    if let Err(error) = self.open_preview_tag(&keyword) {
                        self.set_error(error);
                    }
                }
            }
            'z' => {
                if let Err(error) = self.close_preview() {
                    self.set_error(error);
                }
            }
            ']' => {
                if let Err(error) = self.split_window(SplitDir::Horizontal, None) {
                    self.set_error(error);
                } else if let Some(keyword) = self.keyword_under_cursor() {
                    if let Err(error) = self.jump_to_tag(&keyword) {
                        self.set_error(error);
                    }
                }
            }
            's' => {
                if let Err(error) = self.split_window(SplitDir::Horizontal, None) {
                    self.set_error(error);
                }
            }
            'v' => {
                if let Err(error) = self.split_window(SplitDir::Vertical, None) {
                    self.set_error(error);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    pub(crate) fn mark_position(&self, mark: char) -> Option<BufPos> {
        self.resolve_mark(mark)
    }

    pub(crate) fn set_mark_position(&mut self, mark: char, pos: BufPos) {
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.marks.insert(mark, pos);
        }
    }

    pub(crate) fn buffer_number_of(&self, id: u64) -> u32 {
        self.buffer_ids()
            .iter()
            .position(|buffer| rvim_types::Id::raw(*buffer) == id)
            .map_or(0, |index| index as u32 + 1)
    }

    fn resolve_mark(&self, mark: char) -> Option<BufPos> {
        if mark == '\'' {
            return self.jumplist.get(self.jump_index).map(|(_, pos)| *pos);
        }
        self.current_buffer()
            .and_then(|buffer| buffer.marks.get(&mark).copied())
    }

    fn record_jump(&mut self) {
        if self.keepjumps_depth > 0 {
            return;
        }
        let entry = (self.current_buffer, self.cursor());
        if self.jumplist.get(self.jump_index) == Some(&entry) {
            return;
        }
        if self.jump_index + 1 < self.jumplist.len() {
            self.jumplist.truncate(self.jump_index + 1);
        }
        self.jumplist.push(entry);
        while self.jumplist.len() > 100 {
            self.jumplist.remove(0);
        }
        self.jump_index = self.jumplist.len().saturating_sub(1);
    }

    fn change_older(&mut self) {
        if self.change_index == 0 {
            self.set_error("E664: changelist is empty");
            return;
        }
        self.change_index -= 1;
        if let Some((buffer, pos)) = self.changelist.get(self.change_index).copied() {
            if buffer != self.current_buffer {
                let _ = self.switch_to_buffer(buffer);
            }
            self.set_cursor(pos);
        }
    }

    fn change_newer(&mut self) {
        if self.change_index + 1 >= self.changelist.len() {
            self.set_error("E665: end of changelist");
            return;
        }
        self.change_index += 1;
        if let Some((buffer, pos)) = self.changelist.get(self.change_index).copied() {
            if buffer != self.current_buffer {
                let _ = self.switch_to_buffer(buffer);
            }
            self.set_cursor(pos);
        }
    }

    pub(crate) fn changes_summary(&self) -> String {
        let mut out = String::from(" change line col file\n");
        for (index, (buffer, pos)) in self.changelist.iter().enumerate() {
            let marker = if index == self.change_index { '>' } else { ' ' };
            let name = self
                .buffers
                .get(*buffer)
                .map(|buffer| buffer.short_name())
                .unwrap_or_default();
            out.push_str(&format!(
                "{marker} {:>4} {:>4} {}\n",
                pos.lnum,
                pos.col + 1,
                name
            ));
        }
        out.trim_end().to_string()
    }

    fn jump_older(&mut self) {
        if self.jump_index == 0 {
            self.set_error("E662: At start of jump list");
            return;
        }
        self.jump_index -= 1;
        if let Some((buffer, pos)) = self.jumplist.get(self.jump_index).copied() {
            if buffer != self.current_buffer {
                let _ = self.switch_to_buffer(buffer);
            }
            self.set_cursor(pos);
        }
    }

    fn jump_newer(&mut self) {
        if self.jump_index + 1 >= self.jumplist.len() {
            self.set_error("E663: At end of jump list");
            return;
        }
        self.jump_index += 1;
        if let Some((buffer, pos)) = self.jumplist.get(self.jump_index).copied() {
            if buffer != self.current_buffer {
                let _ = self.switch_to_buffer(buffer);
            }
            self.set_cursor(pos);
        }
    }

    pub(crate) fn marks_summary(&self) -> String {
        let mut out = String::new();
        if let Some(buffer) = self.current_buffer() {
            let mut marks: Vec<(&char, &BufPos)> = buffer.marks.iter().collect();
            marks.sort_by_key(|(mark, _)| **mark);
            for (mark, pos) in marks {
                out.push_str(&format!(
                    "{mark:>3} {:>6} {:>4} {}\n",
                    pos.lnum,
                    pos.col + 1,
                    buffer.short_name()
                ));
            }
        }
        out.trim_end().to_string()
    }

    pub(crate) fn jumps_summary(&self) -> String {
        let mut out = String::from(" jump line  col file\n");
        for (index, (buffer, pos)) in self.jumplist.iter().enumerate() {
            let marker = if index == self.jump_index { '>' } else { ' ' };
            let name = self
                .buffers
                .get(*buffer)
                .map(|buffer| buffer.short_name())
                .unwrap_or_default();
            out.push_str(&format!(
                "{marker} {:>3} {:>3} {:>4} {}\n",
                index + 1,
                pos.lnum,
                pos.col + 1,
                name
            ));
        }
        out.trim_end().to_string()
    }

    fn viewport_goto(&mut self, which: u8) {
        let Some(window) = self.view.current_window() else {
            return;
        };
        let top = window.viewport.topline.max(1);
        let height = window.height.max(1);
        let line = match which {
            0 => top,
            1 => top + (height / 2) as u32,
            _ => top + height.saturating_sub(1) as u32,
        }
        .min(self.line_count().max(1));
        self.set_cursor(BufPos::new(line, 0));
        self.goto_motion(Motion::FirstNonBlank);
    }

    fn handle_z_key(&mut self, key: KeyEvent) {
        self.pending_z = false;
        let Key::Char(ch) = key.key else {
            return;
        };
        let cursor_line = self.cursor().lnum;
        let height = self
            .view
            .current_window()
            .map_or(1, |window| window.height.max(1));
        match ch {
            'z' => {
                if let Some(window) = self.view.current_window_mut() {
                    window.viewport.topline = cursor_line.saturating_sub(height as u32 / 2).max(1);
                }
            }
            't' => {
                if let Some(window) = self.view.current_window_mut() {
                    window.viewport.topline = cursor_line;
                }
            }
            'b' | '-' => {
                if let Some(window) = self.view.current_window_mut() {
                    window.viewport.topline = cursor_line
                        .saturating_sub(height.saturating_sub(1) as u32)
                        .max(1);
                }
            }
            '.' => {
                if let Some(window) = self.view.current_window_mut() {
                    window.viewport.topline = cursor_line.saturating_sub(height as u32 / 2).max(1);
                }
            }
            'o' => self.fold_open(cursor_line, false),
            'O' => self.fold_open(cursor_line, true),
            'c' => self.fold_close(cursor_line, false),
            'C' => self.fold_close(cursor_line, true),
            'a' => self.fold_toggle(cursor_line),
            'R' => self.fold_open_all(),
            'M' => self.fold_close_all(),
            'r' => {
                let count = self.pending_count.take().unwrap_or(1).max(1) as i64;
                self.fold_reduce(count);
            }
            'm' => {
                let count = self.pending_count.take().unwrap_or(1).max(1) as i64;
                self.fold_more(count);
            }
            'v' => self.fold_open(cursor_line, false),
            'x' => self.fold_open(cursor_line, true),
            'X' => {
                let level = self
                    .options
                    .window_number(self.current_window_id(), "foldlevel");
                self.set_foldlevel(level);
            }
            'j' => self.fold_jump(true),
            'k' => self.fold_jump(false),
            'd' => {
                let _ = self.delete_fold(cursor_line, false);
            }
            'D' => {
                let _ = self.delete_fold(cursor_line, true);
            }
            'E' => self.eliminate_folds(),
            'i' => self.fold_toggle_enable(),
            'n' => self.fold_set_enable(false),
            'N' => self.fold_set_enable(true),
            'f' => self.begin_operator(Operator::Fold),
            'F' => self.begin_operator(Operator::Fold),
            _ => {}
        }
        // Keep topline on a displayed line.
        self.sync_folds();
        if let Some(window) = self.view.current_window_mut() {
            if window.folds.is_hidden(window.viewport.topline) {
                window.viewport.topline = window.folds.owner(window.viewport.topline).max(1);
            }
        }
        self.request_redraw();
    }

    fn format_paragraph(&mut self) {
        if self.deny_modification() {
            return;
        }
        let mut start = self.cursor().lnum;
        let mut end = start;
        while start > 1 && self.line(start - 1).is_some_and(|line| !line.is_empty()) {
            start -= 1;
        }
        let last = self.line_count().max(1);
        while end < last && self.line(end + 1).is_some_and(|line| !line.is_empty()) {
            end += 1;
        }
        let first = self.line(start).unwrap_or(b"").to_vec();
        let indent_len = first
            .iter()
            .take_while(|byte| **byte == b' ' || **byte == b'\t')
            .count();
        let indent = String::from_utf8_lossy(&first[..indent_len]).into_owned();
        let mut words = Vec::new();
        for lnum in start..=end {
            if let Some(line) = self.line(lnum) {
                words.extend(
                    String::from_utf8_lossy(line)
                        .split_whitespace()
                        .map(str::to_string),
                );
            }
        }
        if words.is_empty() {
            return;
        }
        let configured = self
            .options
            .buffer_number(self.current_buffer_id(), "textwidth")
            .max(1) as usize;
        let width = if configured == 1 { 78 } else { configured };
        let available = width.saturating_sub(indent.chars().count()).max(1);
        let mut formatted: Vec<String> = Vec::new();
        let mut current = String::new();
        for word in words {
            if current.is_empty() {
                current.push_str(&word);
            } else if current.chars().count() + 1 + word.chars().count() <= available {
                current.push(' ');
                current.push_str(&word);
            } else {
                formatted.push(std::mem::take(&mut current));
                current.push_str(&word);
            }
        }
        if !current.is_empty() {
            formatted.push(current);
        }
        self.record_undo();
        let lines: Vec<VimString> = formatted
            .into_iter()
            .map(|line| VimString::from(format!("{indent}{line}")))
            .collect();
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.lines.remove(start - 1, end - start + 1);
            buffer.text.lines.insert_many(start - 1, lines);
            buffer.text.touch();
        }
        self.set_cursor(BufPos::new(start, indent_len as u32));
        self.commit_undo();
        self.request_redraw();
    }

    fn display_chunks_for(&self, line: &[u8]) -> Vec<(usize, usize)> {
        let Some(window) = self.view.current_window() else {
            return vec![(0, line.len())];
        };
        if !window.options.wrap {
            return vec![(0, line.len())];
        }
        rvim_view::display_chunks(line, window.text_width(), &window.options)
    }

    fn display_prefix_for(&self, line: &[u8]) -> u16 {
        let Some(window) = self.view.current_window() else {
            return 0;
        };
        rvim_view::display_prefix_width(line, window.text_width(), &window.options)
    }

    fn display_tabstop(&self) -> usize {
        self.view
            .current_window()
            .map_or(8, |window| window.options.tabstop.max(1))
    }

    fn byte_for_display_col(&self, line: &[u8], start: usize, target: usize) -> usize {
        let tabstop = self.display_tabstop();
        let mut used = 0usize;
        let mut index = start;
        while index < line.len() {
            let (ch, len) = if line[index] == b'\t' {
                ('\t', 1)
            } else if let Some((ch, len)) = rvim_types::unicode::char_at(line, index) {
                (ch, len)
            } else {
                ('\u{FFFD}', 1)
            };
            let cells = if ch == '\t' {
                tabstop - (used % tabstop)
            } else {
                rvim_types::unicode::char_width(ch).max(1)
            };
            if used + cells > target {
                return index;
            }
            used += cells;
            index += len;
        }
        line.len()
    }

    fn display_col_for(&self, line: &[u8], col: u32) -> usize {
        let tabstop = self.display_tabstop();
        rvim_types::unicode::display_width(&line[..(col as usize).min(line.len())], 0, tabstop)
    }

    fn display_chunk_index(&self, chunks: &[(usize, usize)], col: usize) -> usize {
        chunks
            .iter()
            .position(|(start, end)| col >= *start && col < *end)
            .unwrap_or_else(|| chunks.len().saturating_sub(1))
    }

    fn goto_display_edge(&mut self, middle: bool, end: bool) {
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let chunks = self.display_chunks_for(&line);
        let index = self.display_chunk_index(&chunks, pos.col as usize);
        let (start, last) = chunks[index];
        let prefix = if index > 0 {
            self.display_prefix_for(&line) as usize
        } else {
            0
        };
        let prefix_cells = if index > 0 { prefix } else { 0 };
        let content_start = start;
        let content_end = last;
        let content_width = if content_end > content_start {
            rvim_types::unicode::display_width(
                &line[content_start..content_end],
                0,
                self.display_tabstop(),
            )
        } else {
            0
        };
        let target = if middle {
            prefix_cells + content_width / 2
        } else if end {
            prefix_cells + content_width.saturating_sub(1)
        } else {
            0
        };
        let col =
            self.byte_for_display_col(&line, content_start, target.saturating_sub(prefix_cells));
        self.set_cursor(BufPos::new(pos.lnum, col as u32));
    }

    fn move_display_line(&mut self, down: bool, count: u32) {
        for _ in 0..count.max(1) {
            let pos = self.cursor();
            let Some(line) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(pos.lnum))
                .map(|line| line.to_vec())
            else {
                return;
            };
            let chunks = self.display_chunks_for(&line);
            let index = self.display_chunk_index(&chunks, pos.col as usize);
            let chunk_start_screen = self.display_col_for(&line, chunks[index].0 as u32);
            let local = self
                .display_col_for(&line, pos.col)
                .saturating_sub(chunk_start_screen);
            if down && index + 1 < chunks.len() {
                let target = chunks[index + 1];
                let col = self.byte_for_display_col(&line, target.0, local);
                self.set_cursor(BufPos::new(pos.lnum, col as u32));
                continue;
            }
            if !down && index > 0 {
                let target = chunks[index - 1];
                let col = self.byte_for_display_col(&line, target.0, local);
                self.set_cursor(BufPos::new(pos.lnum, col as u32));
                continue;
            }
            let next_lnum = if down {
                (pos.lnum + 1).min(self.line_count().max(1))
            } else {
                pos.lnum.saturating_sub(1).max(1)
            };
            if next_lnum == pos.lnum {
                break;
            }
            let Some(next_line) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(next_lnum))
                .map(|line| line.to_vec())
            else {
                break;
            };
            let next_chunks = self.display_chunks_for(&next_line);
            let mut target_index = next_chunks.len().saturating_sub(1);
            let mut offset = 0usize;
            for (chunk_index, (start, end)) in next_chunks.iter().enumerate() {
                let width = rvim_types::unicode::display_width(
                    &next_line[*start..*end],
                    0,
                    self.display_tabstop(),
                );
                if local < offset + width {
                    target_index = chunk_index;
                    break;
                }
                offset += width;
            }
            let target = next_chunks[target_index];
            let col = self.byte_for_display_col(&next_line, target.0, local);
            self.set_cursor(BufPos::new(next_lnum, col as u32));
        }
    }

    fn handle_g_key(&mut self, key: KeyEvent) {
        self.pending_g = false;
        let count = self.pending_count.take();
        let Key::Char(ch) = key.key else {
            return;
        };
        let count = count.unwrap_or(1);
        if let Some(op) = self.pending_operator.take() {
            let motion = match ch {
                'g' => Some(Motion::FileStart),
                'e' => Some(Motion::WordEndBackward),
                'E' => Some(Motion::BigWordEndBackward),
                'j' => Some(Motion::Down),
                'k' => Some(Motion::Up),
                '0' => Some(Motion::LineStart),
                '^' => Some(Motion::FirstNonBlank),
                '$' => Some(Motion::LineEnd),
                _ => None,
            };
            if let Some(motion) = motion {
                self.apply_operator_motion(op, motion, count);
            }
            return;
        }
        match ch {
            'g' => {
                let line = if count == 1 { 1 } else { count };
                self.goto_line(line);
            }
            'e' => self.move_count(Motion::WordEndBackward),
            'E' => self.move_count(Motion::BigWordEndBackward),
            'j' => self.move_display_line(true, count),
            'k' => self.move_display_line(false, count),
            'h' => self.move_count(Motion::Left),
            'l' => self.move_count(Motion::Right),
            '0' => self.goto_display_edge(false, false),
            '^' => self.move_count(Motion::FirstNonBlank),
            '$' => self.goto_display_edge(false, true),
            'm' => self.goto_display_edge(true, false),
            '_' => {
                let lnum = self.cursor().lnum;
                let len = self.current_buffer().map_or(0, |b| b.text.line_len(lnum));
                let line = self
                    .current_buffer()
                    .and_then(|b| b.text.line(lnum))
                    .unwrap_or(b"");
                let mut col = len.saturating_sub(1);
                while col > 0 && (line[col as usize] == b' ' || line[col as usize] == b'\t') {
                    col -= 1;
                }
                self.set_cursor(BufPos::new(lnum, col));
            }
            't' => self.tab_next(),
            'T' => self.tab_previous(),
            'J' => self.join_lines(count.max(2), false),
            ']' => {
                if let Some(keyword) = self.keyword_under_cursor() {
                    let result = if key.mods.contains(Mods::CTRL) {
                        self.jump_to_tag(&keyword)
                    } else {
                        self.tag_select(&keyword)
                    };
                    if let Err(error) = result {
                        self.set_error(error);
                    }
                }
            }
            'q' | 'w' => self.format_paragraph(),
            ';' => self.change_older(),
            ',' => self.change_newer(),
            'I' => self.goto_first_line(),
            'i' => {
                let mark = self
                    .current_buffer()
                    .and_then(|buffer| buffer.marks.get(&'^').copied());
                if let Some(pos) = mark {
                    self.set_cursor(pos);
                    self.enter_insert(InsertKind::Before);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    fn handle_pending_char(&mut self, pending: CharPending, key: KeyEvent) {
        self.pending_char = None;
        let Key::Char(ch) = key.key else {
            return;
        };
        let count = self.take_count();
        match pending {
            CharPending::Find { forward, till } => {
                let motion = Motion::FindChar { ch, forward, till };
                if let Some(op) = self.pending_operator.take() {
                    self.apply_operator_motion(op, motion, count);
                } else {
                    let start = self.cursor();
                    let mut pos = start;
                    for _ in 0..count {
                        let next = move_cursor(
                            self.current_buffer().map(|b| &b.text).unwrap(),
                            pos,
                            motion,
                            self.options.get_number("tabstop") as usize,
                        );
                        if next == pos {
                            break;
                        }
                        pos = next;
                    }
                    self.set_cursor(pos);
                }
            }
            CharPending::Replace => {
                self.replace_chars(ch, count);
            }
            CharPending::Mark => {
                let cursor = self.cursor();
                if let Some(buffer) = self.current_buffer_mut() {
                    buffer.marks.insert(ch, cursor);
                }
                self.set_message(format!("mark {ch} set"));
            }
            CharPending::Z => match ch {
                'Z' => self.write_quit(),
                'Q' => self.quit(true),
                _ => {}
            },
            CharPending::Jump { exact } => {
                if let Some(pos) = self.resolve_mark(ch) {
                    self.record_jump();
                    let pos = if exact {
                        pos
                    } else {
                        let line = self
                            .current_buffer()
                            .and_then(|b| b.text.line(pos.lnum))
                            .unwrap_or(b"");
                        let col = line
                            .iter()
                            .position(|b| *b != b' ' && *b != b'\t')
                            .unwrap_or(0) as u32;
                        BufPos::new(pos.lnum, col)
                    };
                    self.set_cursor(pos);
                } else {
                    self.set_error(format!("E20: Mark not set: {ch}"));
                }
            }
            CharPending::Shift { right } => {
                if (right && ch == '>') || (!right && ch == '<') {
                    let count = count.max(1);
                    let start = self.cursor().lnum;
                    let end = (start + count - 1).min(self.line_count().max(1));
                    self.shift_lines(start, end, right);
                }
            }
            CharPending::MacroRecord => {
                if ch == ':' {
                    if let Err(error) = self.open_cmdwin(CommandKind::Ex, false, "") {
                        self.set_error(error);
                    }
                } else if ch == '/' {
                    if let Err(error) = self.open_cmdwin(CommandKind::Search, false, "") {
                        self.set_error(error);
                    }
                } else if ch == '?' {
                    if let Err(error) = self.open_cmdwin(CommandKind::Search, true, "") {
                        self.set_error(error);
                    }
                } else if ch.is_ascii_alphabetic() {
                    self.recording = Some((ch, Vec::new()));
                    self.messages.info(format!("recording @{ch}"));
                } else {
                    self.set_error("E354: Invalid register name");
                }
            }
            CharPending::MacroPlay => {
                let register = if ch == '@' { self.last_macro } else { Some(ch) };
                let Some(register) = register else {
                    self.set_error("E748: No previously used register");
                    return;
                };
                let Some(keys) = self.macros.get(&register).cloned() else {
                    self.set_error(format!("E748: No previously used register: {register}"));
                    return;
                };
                self.last_macro = Some(register);
                for _ in 0..count {
                    for key in keys.clone() {
                        self.dispatch_key(key);
                    }
                }
            }
            CharPending::TextObject { around } => {
                let range = match ch {
                    'w' | 'W' => self.text_object_range(around, true),
                    _ => None,
                };
                if let (Some(range), Some(op)) = (range, self.pending_operator.take()) {
                    self.apply_operator_range(op, range);
                }
            }
        }
        self.request_redraw();
    }

    fn handle_operator_pending(&mut self, op: Operator, key: KeyEvent) {
        let Key::Char(ch) = key.key else {
            self.pending_operator = None;
            return;
        };
        if ch.is_ascii_digit() && !(ch == '0' && self.pending_count.is_none()) {
            self.accumulate_count(ch);
            return;
        }
        let count = self.take_count();
        self.pending_operator = None;
        let same = matches!(
            (op, ch),
            (Operator::Delete, 'd') | (Operator::Change, 'c') | (Operator::Yank, 'y')
        );
        if same {
            self.apply_linewise_operator(op, count);
            return;
        }
        // "do" and "dp" are diff commands, not d + motion.
        if op == Operator::Delete && matches!(ch, 'o' | 'p') {
            let result = if ch == 'o' {
                self.diff_get()
            } else {
                self.diff_put()
            };
            if let Err(error) = result {
                self.set_error(error);
            }
            return;
        }
        let motion = match ch {
            'w' => Motion::WordForward,
            'W' => Motion::BigWordForward,
            'b' => Motion::WordBackward,
            'B' => Motion::BigWordBackward,
            'e' => Motion::WordEnd,
            'E' => Motion::BigWordEnd,
            'h' => Motion::Left,
            'l' => Motion::Right,
            'j' => Motion::Down,
            'k' => Motion::Up,
            '0' => Motion::LineStart,
            '^' => Motion::FirstNonBlank,
            '$' => Motion::LineEnd,
            'G' => Motion::FileEnd,
            'g' => {
                self.pending_g = true;
                self.operator_count = Some(count);
                self.pending_operator = Some(op);
                return;
            }
            'i' | 'a' => {
                self.pending_operator = Some(op);
                self.pending_char = Some(CharPending::TextObject { around: ch == 'a' });
                self.pending_count = Some(count);
                return;
            }
            'f' | 'F' | 't' | 'T' => {
                self.pending_operator = Some(op);
                self.pending_char = Some(CharPending::Find {
                    forward: matches!(ch, 'f' | 't'),
                    till: matches!(ch, 't' | 'T'),
                });
                self.pending_count = Some(count);
                return;
            }
            _ => return,
        };
        self.apply_operator_motion(op, motion, count);
    }

    fn begin_operator(&mut self, op: Operator) {
        if !matches!(op, Operator::Yank | Operator::Fold) && self.deny_modification() {
            return;
        }
        let count = self.pending_count.take();
        self.operator_count = count;
        self.pending_operator = Some(op);
        self.request_redraw();
    }

    fn move_count(&mut self, motion: Motion) {
        let count = self.take_count();
        self.goto_motion(motion);
        if count > 1 {
            let vertical = matches!(motion, Motion::Down | Motion::Up);
            let tabstop = self.options.get_number("tabstop") as usize;
            let mut pos = self.cursor();
            for _ in 1..count {
                let next = {
                    let Some(buffer) = self.current_buffer() else {
                        return;
                    };
                    move_cursor(&buffer.text, pos, motion, tabstop)
                };
                let next = if vertical {
                    self.fold_adjust_vertical(pos, next)
                } else {
                    next
                };
                if next == pos {
                    break;
                }
                pos = next;
            }
            self.set_cursor(pos);
        }
        self.after_normal_key_ch(' ');
    }

    pub(crate) fn goto_motion(&mut self, motion: Motion) {
        let Some(buffer) = self.current_buffer() else {
            return;
        };
        let start = self.cursor();
        let mut pos = move_cursor(
            &buffer.text,
            start,
            motion,
            self.options.get_number("tabstop") as usize,
        );
        if matches!(motion, Motion::Down | Motion::Up) {
            pos = self.fold_adjust_vertical(start, pos);
        }
        self.set_cursor(pos);
        self.request_redraw();
    }

    pub(crate) fn goto_line(&mut self, line: u32) {
        self.record_jump();
        let line = line.max(1).min(self.line_count().max(1));
        self.set_cursor(BufPos::new(line, 0));
        self.goto_motion(Motion::FirstNonBlank);
    }

    fn page_move(&mut self, down: bool) {
        let step = self
            .view
            .current_window()
            .map_or(10, |w| (w.height / 2).max(1));
        let delta = if down { step as i64 } else { -(step as i64) };
        let target =
            (self.cursor().lnum as i64 + delta).clamp(1, self.line_count().max(1) as i64) as u32;
        self.set_cursor(BufPos::new(target, self.cursor().col));
        self.request_redraw();
    }

    fn handle_normal_ctrl(&mut self, ch: char) {
        let count = self.take_count();
        match ch.to_ascii_lowercase() {
            'r' => {
                for _ in 0..count {
                    self.redo();
                }
            }
            'f' => {
                let step =
                    self.view.current_window().map_or(10, |w| w.height.max(1)) * count as u16;
                let target = (self.cursor().lnum + step as u32).min(self.line_count().max(1));
                self.set_cursor(BufPos::new(target, self.cursor().col));
            }
            'b' => {
                let step =
                    self.view.current_window().map_or(10, |w| w.height.max(1)) * count as u16;
                let target = self.cursor().lnum.saturating_sub(step as u32).max(1);
                self.set_cursor(BufPos::new(target, self.cursor().col));
            }
            'd' => {
                let configured = self.options.get_number("scroll").max(0) as u16;
                let base = if configured > 0 {
                    configured
                } else {
                    self.view
                        .current_window()
                        .map_or(10, |w| (w.height / 2).max(1))
                };
                let step = (base * count as u16).max(1);
                let target = (self.cursor().lnum + step as u32).min(self.line_count().max(1));
                self.set_cursor(BufPos::new(target, self.cursor().col));
            }
            'u' => {
                let configured = self.options.get_number("scroll").max(0) as u16;
                let base = if configured > 0 {
                    configured
                } else {
                    self.view
                        .current_window()
                        .map_or(10, |w| (w.height / 2).max(1))
                };
                let step = (base * count as u16).max(1);
                let target = self.cursor().lnum.saturating_sub(step as u32).max(1);
                self.set_cursor(BufPos::new(target, self.cursor().col));
            }
            'e' => {
                self.scroll_line(true, count);
            }
            'y' => {
                self.scroll_line(false, count);
            }
            'v' => {
                self.mode = Mode::Visual {
                    linewise: false,
                    block: true,
                    start: self.cursor(),
                };
            }
            'w' => self.pending_ctrl_w = true,
            ']' => {
                if let Some(keyword) = self.keyword_under_cursor() {
                    if let Err(error) = self.tag_select(&keyword) {
                        self.set_error(error);
                    }
                }
            }
            't' => {
                if let Err(error) = self.tag_back() {
                    self.set_error(error);
                }
            }
            'l' => {
                self.messages.clear();
                self.request_redraw();
            }
            'a' => self.increment_number(1, count),
            'x' => self.increment_number(-1, count),
            'o' => self.jump_older(),
            'i' => self.jump_newer(),
            '^' => {
                if let Some(alternate) = self.alternate_buffer {
                    if let Err(error) = self.switch_to_buffer(alternate) {
                        self.set_error(error);
                    }
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    fn scroll_line(&mut self, down: bool, count: u32) {
        let max_top = self.line_count().max(1);
        for _ in 0..count.max(1) {
            let smooth = self
                .view
                .current_window()
                .is_some_and(|window| window.options.smoothscroll && window.options.wrap);
            if smooth {
                let Some(window) = self.view.current_window() else {
                    return;
                };
                let topline = window.viewport.topline;
                let skipcol = window.viewport.skipcol as usize;
                let width = window.text_width();
                let options = window.options.clone();
                let Some(line) = self
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(topline))
                    .map(|line| line.to_vec())
                else {
                    return;
                };
                let chunks = rvim_view::display_chunks(&line, width, &options);
                let starts: Vec<usize> = chunks
                    .iter()
                    .map(|(start, _)| {
                        rvim_types::unicode::display_width(
                            &line[..*start],
                            0,
                            options.tabstop.max(1),
                        )
                    })
                    .collect();
                let index = starts
                    .iter()
                    .rposition(|start| *start <= skipcol)
                    .unwrap_or(0);
                if down {
                    if let Some(next) = starts.get(index + 1) {
                        if let Some(window) = self.view.current_window_mut() {
                            window.viewport.skipcol = *next as u32;
                        }
                    } else if topline < max_top {
                        self.fold_scroll_topline(true);
                    }
                } else if index > 0 {
                    if let Some(window) = self.view.current_window_mut() {
                        window.viewport.skipcol = starts[index - 1] as u32;
                    }
                } else if topline > 1 {
                    let previous = topline - 1;
                    let previous_line = self
                        .current_buffer()
                        .and_then(|buffer| buffer.text.line(previous))
                        .map(|line| line.to_vec());
                    let previous_starts = previous_line.map(|line| {
                        rvim_view::display_chunks(&line, width, &options)
                            .iter()
                            .map(|(start, _)| {
                                rvim_types::unicode::display_width(
                                    &line[..*start],
                                    0,
                                    options.tabstop.max(1),
                                )
                            })
                            .collect::<Vec<_>>()
                    });
                    if let Some(window) = self.view.current_window_mut() {
                        window.viewport.topline = previous;
                        window.viewport.skipcol = previous_starts
                            .and_then(|starts| starts.last().copied())
                            .unwrap_or(0) as u32;
                    }
                }
            } else {
                self.fold_scroll_topline(down);
                let _ = max_top;
            }
        }
    }

    /// Scroll the window one display line, skipping lines hidden by folds.
    fn fold_scroll_topline(&mut self, down: bool) {
        self.sync_folds();
        let Some(window) = self.view.current_window() else {
            return;
        };
        let folds = window.folds.clone();
        let mut top = window.viewport.topline.max(1);
        let line_count = self.line_count().max(1);
        if down {
            if top < line_count {
                top += 1;
            }
        } else if top > 1 {
            top -= 1;
        }
        if folds.is_hidden(top) && folds.owner(top) != top {
            if down {
                top = folds
                    .hidden_end
                    .get((top - 1) as usize)
                    .copied()
                    .unwrap_or(top)
                    .saturating_add(1);
            } else {
                top = folds.owner(top).max(1);
            }
        }
        top = top.clamp(1, line_count);
        if let Some(window) = self.view.current_window_mut() {
            window.viewport.topline = top;
            window.viewport.skipcol = 0;
        }
    }

    pub(crate) fn start_insert(&mut self) {
        self.record_undo();
        let pos = self.cursor();
        self.mode = Mode::Insert(InsertState {
            start: pos,
            append: true,
            replace: false,
            undo_started: true,
        });
        self.last_change = Some(LastChange {
            insert: true,
            text: String::new(),
        });
        self.request_redraw();
    }

    pub(crate) fn stop_insert(&mut self) {
        if self.mode.is_insert() {
            self.finish_insert();
        }
    }

    fn deny_modification(&mut self) -> bool {
        if self
            .options
            .buffer_bool(self.current_buffer_id(), "modifiable")
        {
            false
        } else {
            self.set_error("E21: Cannot make changes, 'modifiable' is off");
            true
        }
    }

    fn open_cmdwin(
        &mut self,
        kind: CommandKind,
        backward: bool,
        pending_text: &str,
    ) -> Result<(), VimError> {
        if self.cmdwin.is_some() {
            return Ok(());
        }
        let return_buffer = self.current_buffer;
        let buffer = self.create_buffer(None)?;
        let lines: Vec<VimString> = self
            .command_history
            .iter()
            .cloned()
            .map(VimString::from)
            .collect();
        let mut lines = lines;
        if !pending_text.is_empty() {
            lines.push(VimString::from(pending_text));
        }
        if lines.is_empty() {
            lines.push(VimString::new());
        }
        if let Some(entry) = self.buffers.get_mut(buffer) {
            entry.text.restore_lines(lines.clone());
            entry.text.set_modified(false);
            entry.buftype = "nofile".to_string();
            entry.filetype = "vim".to_string();
            entry.set_name(std::path::PathBuf::from("[Command Line]"));
        }
        let _ = self.options.set_buffer(
            rvim_types::Id::raw(buffer),
            "buftype",
            crate::options::OptionValue::String(VimString::from("nofile")),
        );
        let _ = self.options.set_buffer(
            rvim_types::Id::raw(buffer),
            "filetype",
            crate::options::OptionValue::String(VimString::from("vim")),
        );
        let after = self.options.get_bool("splitbelow");
        let Some(_window) = self.view.split_ordered(buffer, SplitDir::Horizontal, after) else {
            return Err(VimError::plain("E36: Not enough room"));
        };
        self.current_buffer = buffer;
        self.cmdwin = Some(CmdwinState {
            buffer,
            return_buffer,
            kind,
            backward,
        });
        let last = lines.len().max(1) as u32;
        self.set_cursor(BufPos::new(last, 0));
        // Size the window from 'cmdwinheight'.
        let target = self.options.get_number("cmdwinheight").max(1) as u16;
        if let Some(id) = self.view.current_window().map(|window| window.id) {
            if self.view.set_window_text_height(id, target) {
                self.request_redraw();
            }
        }
        self.mode = Mode::Normal;
        self.sync_window_options();
        self.update_window_buffer();
        self.request_redraw();
        Ok(())
    }

    fn execute_cmdwin(&mut self) {
        let Some(state) = self.cmdwin.take() else {
            return;
        };
        let line = self
            .line(self.cursor().lnum)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default();
        if let Some(buffer) = self.buffers.get_mut(state.buffer) {
            buffer.set_modified(false);
        }
        let _ = self.view.close_current_window();
        self.buffers.remove(state.buffer);
        if let Some(window) = self.view.current_window() {
            self.current_buffer = window.buffer;
        }
        self.sync_window_options();
        if line.is_empty() {
            self.request_redraw();
            return;
        }
        match state.kind {
            CommandKind::Ex => {
                self.command_history.push_back(line.clone());
                self.execute_ex(&line);
            }
            CommandKind::Search => {
                self.set_search_direction(&line, !state.backward);
            }
        }
        let _ = state.return_buffer;
        self.request_redraw();
    }

    pub(crate) fn close_cmdwin(&mut self) {
        let Some(state) = self.cmdwin.take() else {
            return;
        };
        if let Some(buffer) = self.buffers.get_mut(state.buffer) {
            buffer.set_modified(false);
        }
        let _ = self.view.close_current_window();
        self.buffers.remove(state.buffer);
        if let Some(window) = self.view.current_window() {
            self.current_buffer = window.buffer;
        }
        self.sync_window_options();
        self.request_redraw();
    }

    fn enter_insert(&mut self, kind: InsertKind) {
        if self.deny_modification() {
            return;
        }
        let old = self.cursor();
        let line_len = self.line(1).map(|_| 0).unwrap_or(0);
        let _ = line_len;
        self.record_undo();
        let target = match kind {
            InsertKind::Before | InsertKind::Replace => old,
            InsertKind::After => {
                let len = self
                    .current_buffer()
                    .map_or(0, |b| b.text.line_len(old.lnum));
                if len == 0 {
                    old
                } else {
                    BufPos::new(old.lnum, self.next_boundary(old.lnum, old.col).min(len))
                }
            }
            InsertKind::FirstNonBlank => {
                let col = self
                    .current_buffer()
                    .and_then(|b| b.text.line(old.lnum))
                    .map(|line| {
                        line.iter()
                            .position(|b| *b != b' ' && *b != b'\t')
                            .unwrap_or(0) as u32
                    })
                    .unwrap_or(0);
                BufPos::new(old.lnum, col)
            }
            InsertKind::EndOfLine => {
                let len = self
                    .current_buffer()
                    .map_or(0, |b| b.text.line_len(old.lnum));
                BufPos::new(old.lnum, len)
            }
            InsertKind::OpenBelow | InsertKind::OpenAbove => {
                let before = if kind == InsertKind::OpenBelow {
                    old.lnum + 1
                } else {
                    old.lnum
                };
                if let Some(buffer) = self.current_buffer_mut() {
                    buffer.text.insert_line(before, VimString::new());
                }
                BufPos::new(before, 0)
            }
        };
        self.set_insert_cursor(target);
        let replace = kind == InsertKind::Replace;
        self.last_change = Some(LastChange {
            insert: true,
            text: String::new(),
        });
        self.mode = Mode::Insert(InsertState {
            start: target,
            append: !matches!(kind, InsertKind::Before),
            replace,
            undo_started: true,
        });
        self.fire_event(Event::InsertEnter);
        self.request_redraw();
    }

    fn handle_insert_key(&mut self, key: KeyEvent, _state: InsertState) {
        if self.insert_ctrl_x {
            self.insert_ctrl_x = false;
            match key.key {
                Key::Char('u') => self.complete_from_func("completefunc"),
                Key::Char('o') => self.complete_from_func("omnifunc"),
                _ => {}
            }
            return;
        }
        if self.insert_register_pending {
            self.insert_register_pending = false;
            if let Key::Char(ch) = key.key {
                if let Some(register) = RegisterName::parse(ch) {
                    if let Some(content) = self.registers.get(register).cloned() {
                        let text = content.text();
                        self.insert_text(text.as_bytes());
                        if content.kind == RegisterKind::Linewise {
                            self.insert_text(b"\n");
                        }
                    }
                }
            }
            return;
        }
        if self.insert_literal_pending {
            self.insert_literal_pending = false;
            if let Key::Char(ch) = key.key {
                let mut buffer = [0u8; 4];
                self.insert_text(ch.encode_utf8(&mut buffer).as_bytes());
            }
            return;
        }
        if let Some(first) = self.digraph_pending {
            let Key::Char(ch) = key.key else {
                self.digraph_pending = None;
                return;
            };
            if first == '\0' {
                self.digraph_pending = Some(ch);
            } else {
                self.digraph_pending = None;
                if let Some(combined) = digraph(first, ch) {
                    let mut buffer = [0u8; 4];
                    self.insert_text(combined.encode_utf8(&mut buffer).as_bytes());
                }
            }
            return;
        }
        if matches!(key.key, Key::Char(' ') | Key::Enter | Key::Esc) {
            self.expand_abbreviation();
        }
        match key.key {
            Key::Esc => {
                self.finish_insert();
                return;
            }
            Key::Char(ch) => {
                if key.mods.contains(Mods::CTRL) {
                    if ch == 'c' {
                        if self
                            .current_buffer()
                            .is_some_and(|buffer| buffer.buftype == "prompt")
                        {
                            self.prompt_submit(true);
                        } else {
                            self.finish_insert();
                        }
                        return;
                    }
                    self.insert_ctrl(ch);
                    return;
                }
                if key.mods.contains(Mods::ALT) {
                    return;
                }
                let mut buffer = [0u8; 4];
                let text = ch.encode_utf8(&mut buffer);
                self.insert_text(text.as_bytes());
                if ch == '}' {
                    self.reindent_closing_brace();
                } else if ch == '{' {
                    self.reindent_opening_brace();
                } else if ch == ':' {
                    self.reindent_case_label();
                    self.reindent_scope_label();
                    self.reindent_jump_label();
                } else if ch == ';' {
                    self.reindent_break();
                } else if ch == '#' {
                    self.reindent_preprocessor();
                }
            }
            Key::Enter => {
                if self
                    .current_buffer()
                    .is_some_and(|buffer| buffer.buftype == "prompt")
                {
                    self.prompt_submit(false);
                    return;
                }
                let indent = self.newline_indent();
                self.insert_text(format!("\n{indent}").as_bytes());
            }
            Key::Tab => {
                let buffer_id = self.current_buffer_id();
                let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
                if expandtab {
                    let width = self.options.buffer_number(buffer_id, "shiftwidth").max(1) as usize;
                    let col = self.cursor().col as usize;
                    let spaces = width - (col % width);
                    self.insert_text(&vec![b' '; spaces]);
                } else {
                    self.insert_text(b"\t");
                }
            }
            Key::Backspace => self.insert_backspace(),
            Key::Delete => {
                let pos = self.cursor();
                if let Some(buffer) = self.current_buffer_mut() {
                    if pos.col < buffer.text.line_len(pos.lnum) {
                        buffer
                            .text
                            .delete_range(pos, BufPos::new(pos.lnum, pos.col + 1));
                    } else if pos.lnum < buffer.text.line_count() {
                        buffer.text.delete_range(pos, BufPos::new(pos.lnum + 1, 0));
                    }
                }
                self.request_redraw();
            }
            Key::Left => {
                if self.cursor().col > 0 {
                    let pos = self.cursor();
                    self.set_insert_cursor(BufPos::new(pos.lnum, pos.col - 1));
                }
            }
            Key::Right => {
                let pos = self.cursor();
                let len = self
                    .current_buffer()
                    .map_or(0, |b| b.text.line_len(pos.lnum));
                if pos.col < len {
                    self.set_insert_cursor(BufPos::new(pos.lnum, pos.col + 1));
                }
            }
            Key::Up => {
                let pos = self.cursor();
                if pos.lnum > 1 {
                    let col = pos.col.min(
                        self.current_buffer()
                            .map_or(0, |b| b.text.line_len(pos.lnum - 1)),
                    );
                    self.set_insert_cursor(BufPos::new(pos.lnum - 1, col));
                }
            }
            Key::Down => {
                let pos = self.cursor();
                if pos.lnum < self.line_count() {
                    let col = pos.col.min(
                        self.current_buffer()
                            .map_or(0, |b| b.text.line_len(pos.lnum + 1)),
                    );
                    self.set_insert_cursor(BufPos::new(pos.lnum + 1, col));
                }
            }
            Key::Home => {
                let pos = self.cursor();
                self.set_insert_cursor(BufPos::new(pos.lnum, 0));
            }
            Key::End => {
                let pos = self.cursor();
                let len = self
                    .current_buffer()
                    .map_or(0, |b| b.text.line_len(pos.lnum));
                self.set_insert_cursor(BufPos::new(pos.lnum, len));
            }
            _ => {}
        }
        self.request_redraw();
    }

    fn complete_from_func(&mut self, option: &str) {
        let name = self.options.buffer_string(self.current_buffer_id(), option);
        if name.is_empty() {
            return;
        }
        let result = self.call_viml_function(
            &name,
            vec![
                rvim_viml::Value::Number(1),
                rvim_viml::Value::string(String::new()),
            ],
        );
        let start = match result {
            Ok(value) => value.echo_string().parse::<i64>().unwrap_or(-1),
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        if start < 0 {
            return;
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let start = (start as usize).min(pos.col as usize).min(line.len());
        let base = String::from_utf8_lossy(&line[start..pos.col as usize]).into_owned();
        let result = self.call_viml_function(
            &name,
            vec![rvim_viml::Value::Number(0), rvim_viml::Value::string(base)],
        );
        let candidates: Vec<String> = match result {
            Ok(value) => value
                .as_list()
                .map(|list| {
                    list.borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect()
                })
                .unwrap_or_default(),
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        if candidates.is_empty() {
            return;
        }
        let start = BufPos::new(pos.lnum, start as u32);
        self.delete_insert_range(start, pos);
        self.insert_text(candidates[0].as_bytes());
        self.insert_completion = Some(InsertCompletion {
            start,
            candidates,
            index: 0,
        });
    }

    pub(crate) fn complete_matches(&mut self, startcol: u32, matches: Vec<String>) {
        let cursor = self.cursor();
        let start = BufPos::new(cursor.lnum, startcol.saturating_sub(1));
        self.insert_completion = Some(InsertCompletion {
            start,
            candidates: matches,
            index: 0,
        });
        self.request_redraw();
    }

    pub(crate) fn complete_add_matches(&mut self, matches: Vec<String>) {
        if let Some(state) = &mut self.insert_completion {
            state.candidates.extend(matches);
            self.request_redraw();
        } else {
            self.complete_matches(self.cursor().col + 1, matches);
        }
    }

    pub(crate) fn completion_visible(&self) -> bool {
        self.insert_completion.is_some()
    }

    pub(crate) fn completion_items(&self) -> Vec<String> {
        self.insert_completion
            .as_ref()
            .map(|state| state.candidates.clone())
            .unwrap_or_default()
    }

    fn insert_completion_candidates(&self, prefix: &str) -> Vec<String> {
        let mut words = HashSet::new();
        for lnum in 1..=self.line_count() {
            let Some(line) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
            else {
                continue;
            };
            let mut start = None;
            for (index, byte) in line.iter().enumerate() {
                if byte.is_ascii_alphanumeric() || *byte == b'_' {
                    if start.is_none() {
                        start = Some(index);
                    }
                } else if let Some(begin) = start.take() {
                    let word = String::from_utf8_lossy(&line[begin..index]).into_owned();
                    if word.starts_with(prefix) && word != prefix {
                        words.insert(word);
                    }
                }
            }
            if let Some(begin) = start {
                let word = String::from_utf8_lossy(&line[begin..]).into_owned();
                if word.starts_with(prefix) && word != prefix {
                    words.insert(word);
                }
            }
        }
        let mut words: Vec<String> = words.into_iter().collect();
        words.sort();
        words
    }

    fn complete_insert(&mut self, forward: bool) {
        if let Some(mut state) = self.insert_completion.take() {
            if !state.candidates.is_empty() {
                let len = state.candidates.len();
                state.index = if forward {
                    (state.index + 1) % len
                } else {
                    (state.index + len - 1) % len
                };
                let candidate = state.candidates[state.index].clone();
                let pos = self.cursor();
                self.delete_insert_range(state.start, pos);
                self.insert_text(candidate.as_bytes());
                self.insert_completion = Some(state);
            }
            return;
        }
        let pos = self.cursor();
        let line = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .unwrap_or(b"");
        let mut start = pos.col as usize;
        while start > 0 && (line[start - 1].is_ascii_alphanumeric() || line[start - 1] == b'_') {
            start -= 1;
        }
        if start == pos.col as usize {
            return;
        }
        let prefix = String::from_utf8_lossy(&line[start..pos.col as usize]).into_owned();
        let candidates = self.insert_completion_candidates(&prefix);
        if candidates.is_empty() {
            return;
        }
        let index = if forward { 0 } else { candidates.len() - 1 };
        let candidate = candidates[index].clone();
        let start = BufPos::new(pos.lnum, start as u32);
        self.delete_insert_range(start, pos);
        self.insert_text(candidate.as_bytes());
        self.insert_completion = Some(InsertCompletion {
            start,
            candidates,
            index,
        });
    }

    fn reindent_opening_brace(&mut self) {
        let buffer_id = self.current_buffer_id();
        if !self.options.buffer_bool(buffer_id, "cindent") {
            return;
        }
        let cinkeys = self.options.buffer_string(buffer_id, "cinkeys");
        if !cinkeys.contains('{') {
            return;
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let indent_len = line
            .iter()
            .take_while(|byte| **byte == b' ' || **byte == b'\t')
            .count();
        let trimmed = &line[indent_len..];
        if trimmed != b"{" {
            return;
        }
        let mut depth = 0i32;
        for lnum in 1..pos.lnum {
            if let Some(previous) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
            {
                for byte in previous {
                    match byte {
                        b'{' => depth += 1,
                        b'}' => depth = (depth - 1).max(0),
                        _ => {}
                    }
                }
            }
        }
        let width = self.options.buffer_number(buffer_id, "shiftwidth").max(0) as usize;
        let width = if width == 0 {
            self.options.buffer_number(buffer_id, "tabstop").max(1) as usize
        } else {
            width
        };
        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        let mut previous_indent = 0usize;
        let mut previous_trimmed = Vec::new();
        let mut lnum = pos.lnum.saturating_sub(1);
        while lnum >= 1 {
            if let Some(previous) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
            {
                if !previous.is_empty() {
                    let indent = previous
                        .iter()
                        .take_while(|byte| **byte == b' ' || **byte == b'\t')
                        .count();
                    previous_trimmed = previous[indent..].to_vec();
                    previous_indent = indent;
                    break;
                }
            }
            lnum = lnum.saturating_sub(1);
        }
        let cinwords = self.options.buffer_string(buffer_id, "cinwords");
        let previous_text = String::from_utf8_lossy(&previous_trimmed);
        let previous_start = previous_text.trim_start();
        let control = previous_start.starts_with("else")
            || cinwords.split(',').any(|word| {
                !word.is_empty()
                    && previous_start.strip_prefix(word).is_some_and(|rest| {
                        rest.is_empty() || rest.starts_with(' ') || rest.starts_with('(')
                    })
            });
        let desired = if depth == 0 && !control {
            cino_amount(&cino, 'f', width).unwrap_or(0).max(0) as usize
        } else {
            previous_indent + cino_amount(&cino, '{', width).unwrap_or(0).max(0) as usize
        };
        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        let indent = if expandtab {
            " ".repeat(desired)
        } else {
            format!(
                "{}{}",
                "\t".repeat(desired / width.max(1)),
                " ".repeat(desired % width.max(1))
            )
        };
        let mut new_line = indent.as_bytes().to_vec();
        new_line.extend_from_slice(trimmed);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer
                .text
                .replace_line(pos.lnum, VimString::from(new_line));
        }
        self.set_insert_cursor(BufPos::new(pos.lnum, indent.len() as u32 + 1));
    }

    fn reindent_jump_label(&mut self) {
        let buffer_id = self.current_buffer_id();
        if !self.options.buffer_bool(buffer_id, "cindent") {
            return;
        }
        let cinkeys = self.options.buffer_string(buffer_id, "cinkeys");
        if !cinkeys.contains(':') {
            return;
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let indent_len = line
            .iter()
            .take_while(|byte| **byte == b' ' || **byte == b'\t')
            .count();
        let text = String::from_utf8_lossy(&line[indent_len..]);
        let Some(label) = text.strip_suffix(':') else {
            return;
        };
        if matches!(
            label,
            "public" | "private" | "protected" | "signals" | "slots"
        ) {
            return;
        }
        if label.is_empty()
            || !label
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            || !label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return;
        }
        let width = self.options.buffer_number(buffer_id, "shiftwidth").max(0) as usize;
        let width = if width == 0 {
            self.options.buffer_number(buffer_id, "tabstop").max(1) as usize
        } else {
            width
        };
        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        if cino_amount(&cino, 'J', width).unwrap_or(0) != 0 {
            let mut depth = 0i32;
            let mut lnum = pos.lnum.saturating_sub(1);
            while lnum >= 1 {
                let Some(previous) = self
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(lnum))
                else {
                    break;
                };
                let mut opening = None;
                for byte in previous.iter().rev() {
                    match byte {
                        b'}' => depth += 1,
                        b'{' => {
                            if depth == 0 {
                                opening = Some(previous.to_vec());
                                break;
                            }
                            depth -= 1;
                        }
                        _ => {}
                    }
                }
                if let Some(opening) = opening {
                    let text = String::from_utf8_lossy(&opening);
                    let text = text.trim_start();
                    if text.starts_with("var ")
                        || text.starts_with("let ")
                        || text.starts_with("const ")
                        || text.contains(" = ")
                        || text.ends_with('=')
                    {
                        return;
                    }
                    break;
                }
                lnum = lnum.saturating_sub(1);
            }
        }
        let desired = match cino_amount(&cino, 'L', width) {
            Some(amount) if amount >= 0 => indent_len.saturating_sub(amount as usize),
            _ => 0,
        };
        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        let indent = if expandtab {
            " ".repeat(desired)
        } else {
            format!(
                "{}{}",
                "\t".repeat(desired / width.max(1)),
                " ".repeat(desired % width.max(1))
            )
        };
        let mut new_line = indent.as_bytes().to_vec();
        new_line.extend_from_slice(&line[indent_len..]);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer
                .text
                .replace_line(pos.lnum, VimString::from(new_line));
        }
        let col = indent.len() as u32 + (pos.col.saturating_sub(indent_len as u32));
        self.set_insert_cursor(BufPos::new(pos.lnum, col));
    }

    fn reindent_preprocessor(&mut self) {
        let buffer_id = self.current_buffer_id();
        if !self.options.buffer_bool(buffer_id, "cindent") {
            return;
        }
        let cinkeys = self.options.buffer_string(buffer_id, "cinkeys");
        if !cinkeys.split(',').any(|key| key == "0#" || key == "#") {
            return;
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let indent_len = line
            .iter()
            .take_while(|byte| **byte == b' ' || **byte == b'\t')
            .count();
        if line.get(indent_len) != Some(&b'#') {
            return;
        }
        let trimmed = line[indent_len..].to_vec();
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.replace_line(pos.lnum, VimString::from(trimmed));
        }
        let col = pos.col.saturating_sub(indent_len as u32);
        self.set_insert_cursor(BufPos::new(pos.lnum, col));
    }

    fn reindent_break(&mut self) {
        let buffer_id = self.current_buffer_id();
        if !self.options.buffer_bool(buffer_id, "cindent") {
            return;
        }
        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        let width = self.options.buffer_number(buffer_id, "shiftwidth").max(0) as usize;
        let width = if width == 0 {
            self.options.buffer_number(buffer_id, "tabstop").max(1) as usize
        } else {
            width
        };
        if cino_amount(&cino, 'b', width).unwrap_or(0) == 0 {
            return;
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let indent_len = line
            .iter()
            .take_while(|byte| **byte == b' ' || **byte == b'\t')
            .count();
        if &line[indent_len..] != b"break;" {
            return;
        }
        let mut lnum = pos.lnum.saturating_sub(1);
        let mut desired = None;
        while lnum >= 1 {
            let Some(previous) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
                .map(|line| line.to_vec())
            else {
                break;
            };
            let indent = previous
                .iter()
                .take_while(|byte| **byte == b' ' || **byte == b'\t')
                .count();
            let text = String::from_utf8_lossy(&previous[indent..]);
            if text.starts_with("case ") || text.starts_with("default:") {
                desired = Some(indent);
                break;
            }
            lnum = lnum.saturating_sub(1);
        }
        let Some(desired) = desired else {
            return;
        };
        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        let indent = if expandtab {
            " ".repeat(desired)
        } else {
            format!(
                "{}{}",
                "\t".repeat(desired / width.max(1)),
                " ".repeat(desired % width.max(1))
            )
        };
        let mut new_line = indent.as_bytes().to_vec();
        new_line.extend_from_slice(b"break;");
        if let Some(buffer) = self.current_buffer_mut() {
            buffer
                .text
                .replace_line(pos.lnum, VimString::from(new_line));
        }
        self.set_insert_cursor(BufPos::new(pos.lnum, indent.len() as u32 + 6));
    }

    fn reindent_scope_label(&mut self) {
        let buffer_id = self.current_buffer_id();
        if !self.options.buffer_bool(buffer_id, "cindent") {
            return;
        }
        let cinkeys = self.options.buffer_string(buffer_id, "cinkeys");
        if !cinkeys.contains(':') {
            return;
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let indent_len = line
            .iter()
            .take_while(|byte| **byte == b' ' || **byte == b'\t')
            .count();
        let text = String::from_utf8_lossy(&line[indent_len..]);
        if !matches!(
            text.as_ref(),
            "public:" | "private:" | "protected:" | "signals:" | "slots:"
        ) {
            return;
        }
        let mut depth = 0i32;
        let mut base_indent = None;
        let mut lnum = pos.lnum.saturating_sub(1);
        while lnum >= 1 {
            let Some(previous) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
                .map(|line| line.to_vec())
            else {
                break;
            };
            for byte in previous.iter().rev() {
                match byte {
                    b'}' => depth += 1,
                    b'{' => {
                        if depth == 0 {
                            let opening = String::from_utf8_lossy(&previous);
                            let trimmed = opening.trim_start();
                            if trimmed.starts_with("class ")
                                || trimmed.starts_with("struct ")
                                || trimmed.starts_with("union ")
                                || trimmed.starts_with("namespace ")
                            {
                                base_indent = Some(
                                    previous
                                        .iter()
                                        .take_while(|byte| **byte == b' ' || **byte == b'\t')
                                        .count(),
                                );
                            }
                            break;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            if base_indent.is_some() {
                break;
            }
            lnum = lnum.saturating_sub(1);
        }
        let Some(base_indent) = base_indent else {
            return;
        };
        let width = self.options.buffer_number(buffer_id, "shiftwidth").max(0) as usize;
        let width = if width == 0 {
            self.options.buffer_number(buffer_id, "tabstop").max(1) as usize
        } else {
            width
        };
        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        let desired = base_indent
            + cino_amount(&cino, 'g', width)
                .unwrap_or(width as i32)
                .max(0) as usize;
        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        let indent = if expandtab {
            " ".repeat(desired)
        } else {
            format!(
                "{}{}",
                "\t".repeat(desired / width.max(1)),
                " ".repeat(desired % width.max(1))
            )
        };
        let mut new_line = indent.as_bytes().to_vec();
        new_line.extend_from_slice(&line[indent_len..]);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer
                .text
                .replace_line(pos.lnum, VimString::from(new_line));
        }
        let col = indent.len() as u32 + (pos.col.saturating_sub(indent_len as u32));
        self.set_insert_cursor(BufPos::new(pos.lnum, col));
    }

    fn reindent_case_label(&mut self) {
        let buffer_id = self.current_buffer_id();
        if !self.options.buffer_bool(buffer_id, "cindent") {
            return;
        }
        let cinkeys = self.options.buffer_string(buffer_id, "cinkeys");
        if !cinkeys.contains(':') {
            return;
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let indent_len = line
            .iter()
            .take_while(|byte| **byte == b' ' || **byte == b'\t')
            .count();
        let text = String::from_utf8_lossy(&line[indent_len..]);
        if !text.starts_with("case ") && !text.starts_with("default:") {
            return;
        }
        let mut depth = 0i32;
        let mut switch_indent = None;
        let mut lnum = pos.lnum.saturating_sub(1);
        while lnum >= 1 {
            let Some(previous) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
                .map(|line| line.to_vec())
            else {
                break;
            };
            for byte in previous.iter().rev() {
                match byte {
                    b'}' => depth += 1,
                    b'{' => {
                        if depth == 0 {
                            let opening = String::from_utf8_lossy(&previous);
                            if opening.trim_start().starts_with("switch") {
                                switch_indent = Some(
                                    previous
                                        .iter()
                                        .take_while(|byte| **byte == b' ' || **byte == b'\t')
                                        .count(),
                                );
                            }
                            break;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            if switch_indent.is_some() {
                break;
            }
            lnum = lnum.saturating_sub(1);
        }
        let Some(switch_indent) = switch_indent else {
            return;
        };
        let width = self.options.buffer_number(buffer_id, "shiftwidth").max(0) as usize;
        let width = if width == 0 {
            self.options.buffer_number(buffer_id, "tabstop").max(1) as usize
        } else {
            width
        };
        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        let amount = cino_amount(&cino, ':', width)
            .unwrap_or(width as i32)
            .max(0) as usize;
        let desired = switch_indent + amount;
        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        let indent = if expandtab {
            " ".repeat(desired)
        } else {
            format!(
                "{}{}",
                "\t".repeat(desired / width.max(1)),
                " ".repeat(desired % width.max(1))
            )
        };
        let mut new_line = indent.as_bytes().to_vec();
        new_line.extend_from_slice(&line[indent_len..]);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer
                .text
                .replace_line(pos.lnum, VimString::from(new_line));
        }
        let col = indent.len() as u32 + (pos.col.saturating_sub(indent_len as u32));
        self.set_insert_cursor(BufPos::new(pos.lnum, col));
    }

    fn reindent_closing_brace(&mut self) {
        let buffer_id = self.current_buffer_id();
        let cindent = self.options.buffer_bool(buffer_id, "cindent");
        let smartindent = self.options.buffer_bool(buffer_id, "smartindent");
        if !cindent && !smartindent {
            return;
        }
        if cindent {
            let cinkeys = self.options.buffer_string(buffer_id, "cinkeys");
            if !cinkeys.contains("0}") && !cinkeys.split(',').any(|key| key == "}") {
                return;
            }
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let indent_len = line
            .iter()
            .take_while(|byte| **byte == b' ' || **byte == b'\t')
            .count();
        if line.get(indent_len) != Some(&b'}') {
            return;
        }
        let mut depth = 0i32;
        for lnum in 1..pos.lnum {
            if let Some(previous) = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
            {
                for byte in previous {
                    match byte {
                        b'{' => depth += 1,
                        b'}' => depth = (depth - 1).max(0),
                        _ => {}
                    }
                }
            }
        }
        let width = self.options.buffer_number(buffer_id, "shiftwidth").max(0) as usize;
        let width = if width == 0 {
            self.options.buffer_number(buffer_id, "tabstop").max(1) as usize
        } else {
            width
        };
        let desired = (depth.saturating_sub(1) as usize) * width;
        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        let indent = if expandtab {
            " ".repeat(desired)
        } else {
            let tabs = desired / width.max(1);
            let spaces = desired % width.max(1);
            format!("{}{}", "\t".repeat(tabs), " ".repeat(spaces))
        };
        let mut new_line = indent.as_bytes().to_vec();
        new_line.extend_from_slice(&line[indent_len..]);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer
                .text
                .replace_line(pos.lnum, VimString::from(new_line));
        }
        self.set_insert_cursor(BufPos::new(pos.lnum, indent.len() as u32 + 1));
    }

    fn insert_ctrl(&mut self, ch: char) {
        match ch.to_ascii_lowercase() {
            'w' => {
                let pos = self.cursor();
                let line = self
                    .current_buffer()
                    .and_then(|b| b.text.line(pos.lnum))
                    .unwrap_or(b"");
                let mut col = pos.col as usize;
                while col > 0 && (line[col - 1] == b' ' || line[col - 1] == b'\t') {
                    col -= 1;
                }
                while col > 0 && line[col - 1] != b' ' && line[col - 1] != b'\t' {
                    col -= 1;
                }
                self.delete_insert_range(BufPos::new(pos.lnum, col as u32), pos);
            }
            'u' => {
                let pos = self.cursor();
                self.delete_insert_range(BufPos::new(pos.lnum, 0), pos);
            }
            'h' => {
                self.insert_backspace();
            }
            'k' => {
                self.digraph_pending = Some('\0');
            }
            'x' => self.insert_ctrl_x = true,
            'n' => self.complete_insert(true),
            'p' => self.complete_insert(false),
            'r' => {
                self.insert_register_pending = true;
            }
            'v' | 'q' => {
                self.insert_literal_pending = true;
            }
            'a' => {
                if let Some(change) = self.last_change.clone() {
                    self.insert_text(change.text.as_bytes());
                }
            }
            'e' => {
                let pos = self.cursor();
                if let Some(line) = self
                    .current_buffer()
                    .and_then(|b| b.text.line(pos.lnum + 1))
                    .map(|l| l.to_vec())
                {
                    if let Some((ch, _)) = rvim_types::unicode::char_at(&line, pos.col as usize) {
                        let mut buf = [0u8; 4];
                        self.insert_text(ch.encode_utf8(&mut buf).as_bytes());
                    }
                }
            }
            'y' => {
                let pos = self.cursor();
                if pos.lnum > 1 {
                    if let Some(line) = self
                        .current_buffer()
                        .and_then(|b| b.text.line(pos.lnum - 1))
                        .map(|l| l.to_vec())
                    {
                        if let Some((ch, _)) = rvim_types::unicode::char_at(&line, pos.col as usize)
                        {
                            let mut buf = [0u8; 4];
                            self.insert_text(ch.encode_utf8(&mut buf).as_bytes());
                        }
                    }
                }
            }
            't' => self.shift_current_line(true),
            'd' => self.shift_current_line(false),
            '[' => {}
            _ => {}
        }
    }

    fn expand_abbreviation(&mut self) {
        if self.abbreviations.is_empty() {
            return;
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|b| b.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let mut start = pos.col as usize;
        while start > 0 && (line[start - 1].is_ascii_alphanumeric() || line[start - 1] == b'_') {
            start -= 1;
        }
        if start == pos.col as usize {
            return;
        }
        let word = String::from_utf8_lossy(&line[start..pos.col as usize]).into_owned();
        let Some(replacement) = self.abbreviations.get(&word).cloned() else {
            return;
        };
        let from = BufPos::new(pos.lnum, start as u32);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.delete_range(from, pos);
        }
        self.set_insert_cursor(from);
        self.insert_text(replacement.as_bytes());
    }

    fn shift_lines(&mut self, start: u32, end: u32, right: bool) {
        if self.deny_modification() {
            return;
        }
        let width = self
            .options
            .buffer_number(self.current_buffer_id(), "shiftwidth")
            .max(1) as usize;
        self.record_undo();
        for lnum in start..=end {
            let Some(line) = self.line(lnum).map(|line| line.to_vec()) else {
                continue;
            };
            let new_line = if right {
                let mut next = vec![b' '; width];
                next.extend_from_slice(&line);
                next
            } else {
                let remove = line
                    .iter()
                    .take(width)
                    .filter(|byte| **byte == b' ' || **byte == b'\t')
                    .count();
                line[remove..].to_vec()
            };
            if let Some(buffer) = self.current_buffer_mut() {
                buffer.text.replace_line(lnum, VimString::from(new_line));
            }
        }
        self.set_cursor(BufPos::new(start, 0));
        self.goto_motion(Motion::FirstNonBlank);
        self.commit_undo();
        self.request_redraw();
    }

    fn shift_current_line(&mut self, right: bool) {
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let width = self
            .options
            .buffer_number(self.current_buffer_id(), "shiftwidth")
            .max(1) as usize;
        if right {
            let mut next = vec![b' '; width];
            next.extend_from_slice(&line);
            if let Some(buffer) = self.current_buffer_mut() {
                buffer.text.replace_line(pos.lnum, VimString::from(next));
            }
            self.set_insert_cursor(BufPos::new(pos.lnum, pos.col.saturating_add(width as u32)));
        } else {
            let remove = line
                .iter()
                .take(width)
                .filter(|byte| **byte == b' ' || **byte == b'\t')
                .count();
            if remove == 0 {
                return;
            }
            if let Some(buffer) = self.current_buffer_mut() {
                buffer
                    .text
                    .replace_line(pos.lnum, VimString::from(&line[remove..]));
            }
            self.set_insert_cursor(BufPos::new(pos.lnum, pos.col.saturating_sub(remove as u32)));
        }
    }

    fn insert_backspace(&mut self) {
        let pos = self.cursor();
        if pos.col > 0 {
            let start = self.prev_char_pos(pos);
            self.delete_insert_range(start, pos);
        } else if pos.lnum > 1 {
            let prev_line = pos.lnum - 1;
            let prev_len = self
                .current_buffer()
                .map_or(0, |b| b.text.line_len(prev_line));
            self.delete_insert_range(BufPos::new(prev_line, prev_len), pos);
        }
        self.request_redraw();
    }

    fn delete_insert_range(&mut self, start: BufPos, end: BufPos) {
        if start == end {
            return;
        }
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.delete_range(start, end);
        }
        self.set_insert_cursor(start);
    }

    fn insert_text(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let replace = matches!(self.mode, Mode::Insert(InsertState { replace: true, .. }));
        let pos = self.cursor();
        if replace && !bytes.contains(&b'\n') {
            let line_len = self
                .current_buffer()
                .map_or(0, |buffer| buffer.text.line_len(pos.lnum));
            let mut end = pos;
            for ch in String::from_utf8_lossy(bytes).chars() {
                if ch == '\n' {
                    break;
                }
                if end.col < line_len {
                    end.col = self.next_boundary(end.lnum, end.col);
                }
            }
            if end != pos {
                if let Some(buffer) = self.current_buffer_mut() {
                    buffer.text.delete_range(pos, end);
                }
            }
        }
        let new_pos = if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.insert_bytes(pos.lnum, pos.col, bytes)
        } else {
            return;
        };
        self.set_insert_cursor(new_pos);
        self.request_redraw();
    }

    fn insert_paste(&mut self, text: &str) {
        let pos = self.cursor();
        self.record_undo();
        let new_pos = if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.insert_bytes(pos.lnum, pos.col, text.as_bytes())
        } else {
            return;
        };
        self.set_cursor(new_pos);
        self.commit_undo();
        self.request_redraw();
    }

    fn finish_insert(&mut self) {
        self.insert_completion = None;
        self.insert_ctrl_x = false;
        let insert_start = match self.mode {
            Mode::Insert(state) => Some(state.start),
            _ => None,
        };
        let pos = self.cursor();
        let new_pos = if pos.col > 0 {
            BufPos::new(pos.lnum, pos.col - 1)
        } else {
            pos
        };
        self.mode = Mode::Normal;
        self.set_cursor(new_pos);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.marks.insert('.', new_pos);
            if let Some(start) = insert_start {
                buffer.marks.insert('^', start);
            }
        }
        self.commit_undo();
        self.fire_event(Event::InsertLeave);
        self.request_redraw();
    }

    fn newline_indent(&mut self) -> String {
        let buffer_id = self.current_buffer_id();
        if self.options.get_bool("paste") {
            return String::new();
        }
        let indentexpr = self.options.buffer_string(buffer_id, "indentexpr");
        if !indentexpr.is_empty() {
            let lnum = self.cursor().lnum + 1;
            let previous = self.viml.get_vvar("lnum");
            self.viml
                .set_vvar("lnum", rvim_viml::Value::Number(lnum as i64));
            let result = self.eval_viml(&indentexpr);
            if let Some(previous) = previous {
                self.viml.set_vvar("lnum", previous);
            } else {
                self.viml.set_vvar("lnum", rvim_viml::Value::Number(0));
            }
            match result {
                Ok(value) => {
                    let text = value.echo_string();
                    if let Ok(count) = text.parse::<i64>() {
                        if count >= 0 {
                            return " ".repeat(count as usize);
                        }
                    } else if !text.is_empty() {
                        return text;
                    }
                }
                Err(error) => self.messages.error(error.to_string()),
            }
        }
        let autoindent = self.options.buffer_bool(buffer_id, "autoindent");
        let smartindent = self.options.buffer_bool(buffer_id, "smartindent");
        let cindent = self.options.buffer_bool(buffer_id, "cindent");
        let lisp = self.options.buffer_bool(buffer_id, "lisp");
        if !autoindent && !smartindent && !cindent && !lisp {
            return String::new();
        }
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|buffer| buffer.text.line(pos.lnum))
        else {
            return String::new();
        };
        let mut indent: Vec<u8> = line
            .iter()
            .copied()
            .take_while(|byte| *byte == b' ' || *byte == b'\t')
            .collect();
        let trimmed = line.iter().copied().skip(indent.len()).collect::<Vec<u8>>();
        let configured_width = self.options.buffer_number(buffer_id, "shiftwidth");
        let shiftwidth = if configured_width <= 0 {
            self.options.buffer_number(buffer_id, "tabstop").max(1) as usize
        } else {
            configured_width as usize
        };
        let expandtab = self.options.buffer_bool(buffer_id, "expandtab");
        if lisp {
            let pos = self.cursor();
            let mut depth = 0i32;
            for lnum in 1..=pos.lnum {
                if let Some(previous) = self
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(lnum))
                {
                    for byte in previous {
                        match byte {
                            b'(' => depth += 1,
                            b')' => depth = (depth - 1).max(0),
                            _ => {}
                        }
                    }
                }
            }
            let width = (depth as usize) * shiftwidth;
            return if expandtab {
                " ".repeat(width)
            } else {
                let tabs = width / shiftwidth.max(1);
                let spaces = width % shiftwidth.max(1);
                format!("{}{}", "\t".repeat(tabs), " ".repeat(spaces))
            };
        }
        let cino = self.options.buffer_string(buffer_id, "cinoptions");
        let mut add_amount = 0i32;
        let mut continuation = false;
        let mut comment_opener = false;
        let mut comment_column = None;
        let mut scope_decl = false;
        let mut initializer = false;
        let mut paren_depth = 0i32;
        let mut case_inline = false;
        let mut knr_params = false;
        let mut add_indent = (smartindent || cindent) && trimmed.last() == Some(&b'{');
        if cindent && !add_indent {
            let text = String::from_utf8_lossy(&trimmed);
            let cinwords = self.options.buffer_string(buffer_id, "cinwords");
            add_indent = cinwords.split(',').any(|word| {
                !word.is_empty()
                    && text.trim_start().strip_prefix(word).is_some_and(|rest| {
                        rest.is_empty()
                            || rest.starts_with(' ')
                            || rest.starts_with('(')
                            || rest.starts_with(':')
                    })
            }) && trimmed.last() != Some(&b'}');
            if !add_indent {
                let start = text.trim_start();
                if start.starts_with("case ") || start.starts_with("default:") {
                    add_indent = true;
                    if let Some(colon) = start.find(':') {
                        if !start[colon + 1..].trim().is_empty()
                            && cino_amount(&cino, 'l', shiftwidth).unwrap_or(0) != 0
                        {
                            case_inline = true;
                        }
                    }
                } else if start.starts_with("public:")
                    || start.starts_with("private:")
                    || start.starts_with("protected:")
                    || start.starts_with("signals:")
                    || start.starts_with("slots:")
                {
                    add_indent = true;
                    scope_decl = true;
                }
            }
            if !add_indent {
                let start = text.trim_start();
                let base_class = start.starts_with("class ") || start.starts_with("struct ");
                if start.ends_with(':') && (base_class || start.contains("::")) {
                    add_indent = true;
                    initializer = true;
                }
            }
            if !add_indent {
                let start = text.trim_start();
                if start.ends_with(')')
                    && start.contains('(')
                    && !start.contains(';')
                    && start
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
                {
                    add_indent = true;
                    knr_params = true;
                }
            }
            if !add_indent && text.trim_start() == "/*" {
                add_indent = true;
                comment_opener = true;
            }
            if !add_indent {
                let start = text.trim_start();
                if let Some(index) = start.find("/*") {
                    let after = &start[index + 2..];
                    if !after.trim().is_empty() {
                        add_indent = true;
                        comment_opener = true;
                        comment_column =
                            Some(index + 2 + after.len().saturating_sub(after.trim_start().len()));
                    }
                }
            }
            if !add_indent {
                for byte in &trimmed {
                    match byte {
                        b'(' => paren_depth += 1,
                        b')' => paren_depth = (paren_depth - 1).max(0),
                        _ => {}
                    }
                }
                if paren_depth > 0 {
                    add_indent = true;
                    continuation = true;
                }
            }
        }
        if add_indent {
            if smartindent && !cindent {
                add_amount = shiftwidth as i32;
            } else if continuation {
                add_amount = if paren_depth > 0 {
                    cino_amount(&cino, '(', shiftwidth).unwrap_or((shiftwidth * 2) as i32)
                } else {
                    cino_amount(&cino, '+', shiftwidth).unwrap_or(shiftwidth as i32)
                };
            } else if scope_decl {
                add_amount = cino_amount(&cino, 'h', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if initializer {
                add_amount = cino_amount(&cino, 'i', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if case_inline {
                add_amount = 0;
            } else if knr_params {
                add_amount = cino_amount(&cino, 'p', shiftwidth).unwrap_or(shiftwidth as i32);
            } else if comment_opener {
                let comment_indent = cino_amount(&cino, 'c', shiftwidth).unwrap_or(3);
                let forced = cino_amount(&cino, 'C', shiftwidth).unwrap_or(0) != 0;
                add_amount = match comment_column {
                    Some(column) if !forced => column as i32,
                    _ => comment_indent,
                };
            } else {
                add_amount = cino_amount(&cino, '>', shiftwidth).unwrap_or(shiftwidth as i32);
                if trimmed.last() == Some(&b'{') {
                    if trimmed == b"{" && indent.is_empty() {
                        add_amount += cino_amount(&cino, '^', shiftwidth).unwrap_or(0);
                    } else {
                        add_amount += cino_amount(&cino, 'e', shiftwidth).unwrap_or(0);
                        let text = String::from_utf8_lossy(&trimmed);
                        if text.contains("namespace") {
                            add_amount += cino_amount(&cino, 'N', shiftwidth).unwrap_or(0);
                        }
                        if text.contains("extern") && text.contains("\"C\"") {
                            add_amount += cino_amount(&cino, 'E', shiftwidth).unwrap_or(0);
                        }
                    }
                } else if String::from_utf8_lossy(&trimmed)
                    .trim_start()
                    .starts_with("case ")
                    || String::from_utf8_lossy(&trimmed)
                        .trim_start()
                        .starts_with("default:")
                {
                    add_amount = cino_amount(&cino, '=', shiftwidth).unwrap_or(shiftwidth as i32);
                } else {
                    add_amount += cino_amount(&cino, 'n', shiftwidth).unwrap_or(0);
                }
            }
        }
        if add_amount > 0 {
            let amount = add_amount as usize;
            if expandtab {
                indent.extend(std::iter::repeat_n(b' ', amount));
            } else {
                let tabs = amount / shiftwidth.max(1);
                let spaces = amount % shiftwidth.max(1);
                indent.extend(std::iter::repeat_n(b'\t', tabs));
                indent.extend(std::iter::repeat_n(b' ', spaces));
            }
        } else if add_amount < 0 {
            let amount = (-add_amount) as usize;
            indent.truncate(indent.len().saturating_sub(amount));
        }
        if cindent && trimmed.first() == Some(&b'}') {
            let close = cino_amount(&cino, '}', shiftwidth)
                .unwrap_or(shiftwidth as i32)
                .max(0) as usize;
            indent.truncate(indent.len().saturating_sub(close));
        }
        String::from_utf8_lossy(&indent).into_owned()
    }

    fn set_insert_cursor(&mut self, pos: BufPos) {
        let Some(buffer) = self.current_buffer() else {
            return;
        };
        let lnum = pos.lnum.max(1).min(buffer.text.line_count().max(1));
        let len = buffer.text.line_len(lnum);
        let pos = BufPos::new(lnum, pos.col.min(len));
        if let Some(window) = self.view.current_window_mut() {
            window.set_cursor(pos);
        }
    }

    fn begin_command(&mut self, kind: CommandKind) {
        if kind == CommandKind::Search {
            self.search_origin = Some(self.cursor());
        }
        self.cmdline = CommandLine {
            text: String::new(),
            cursor: 0,
            prompt: match kind {
                CommandKind::Ex => ':',
                CommandKind::Search => '/',
            },
            kind: Some(kind),
        };
        self.mode = Mode::Command(kind);
        self.cmdline_completion = None;
        self.command_history_index = self.command_history.len();
        self.messages.clear();
        self.request_redraw();
    }

    /// True when `key` is the 'cedit' key used to open the command-line
    /// window from Command-line mode.
    fn command_mode_cedit(&self, key: &KeyEvent) -> bool {
        let cedit = self.options.get_string("cedit");
        if cedit.is_empty() {
            return false;
        }
        let Some(configured) = crate::keymap::parse_keys(&cedit).into_iter().next() else {
            return false;
        };
        if configured == *key {
            return true;
        }
        let (Key::Char(configured), Key::Char(typed)) = (&configured.key, &key.key) else {
            return false;
        };
        key.mods.contains(Mods::CTRL)
            && (*configured as u32) < 0x20
            && (*typed as u32) == (*configured as u32 | 0x60)
    }
    fn handle_command_key(&mut self, key: KeyEvent, kind: CommandKind) {
        match key.key {
            Key::Esc => {
                if kind == CommandKind::Search {
                    if let Some(origin) = self.search_origin.take() {
                        self.set_cursor(origin);
                    }
                }
                self.mode = Mode::Normal;
                self.cmdline = CommandLine::default();
                self.cmdline_completion = None;
            }
            Key::Enter => {
                let text = self.cmdline.text.clone();
                self.mode = Mode::Normal;
                self.cmdline = CommandLine::default();
                self.cmdline_completion = None;
                if !text.is_empty() {
                    self.command_history.push_back(text.clone());
                    while self.command_history.len() > 200 {
                        self.command_history.pop_front();
                    }
                }
                self.search_origin = None;
                match kind {
                    CommandKind::Ex => {
                        if let Err(error) = self.execute_ex_line(&text) {
                            self.set_error(error);
                        }
                    }
                    CommandKind::Search => {
                        self.set_search(&text);
                    }
                }
            }
            Key::Backspace => {
                self.cmdline_completion = None;
                if self.cmdline.cursor > 0 {
                    self.cmdline.text.remove(self.cmdline.cursor - 1);
                    self.cmdline.cursor -= 1;
                } else {
                    self.mode = Mode::Normal;
                    self.cmdline = CommandLine::default();
                }
            }
            Key::Left => {
                if self.cmdline.cursor > 0 {
                    self.cmdline.cursor -= 1;
                }
            }
            Key::Right => {
                if self.cmdline.cursor < self.cmdline.text.len() {
                    self.cmdline.cursor += 1;
                }
            }
            Key::Home => self.cmdline.cursor = 0,
            Key::End => self.cmdline.cursor = self.cmdline.text.len(),
            Key::Up => {
                if self.command_history_index > 0 {
                    self.command_history_index -= 1;
                    if let Some(text) = self.command_history.get(self.command_history_index) {
                        self.cmdline.text = text.clone();
                        self.cmdline.cursor = self.cmdline.text.len();
                    }
                }
            }
            Key::Down => {
                if self.command_history_index + 1 < self.command_history.len() {
                    self.command_history_index += 1;
                    if let Some(text) = self.command_history.get(self.command_history_index) {
                        self.cmdline.text = text.clone();
                    }
                } else {
                    self.command_history_index = self.command_history.len();
                    self.cmdline.text.clear();
                }
                self.cmdline.cursor = self.cmdline.text.len();
            }
            Key::Tab => self.complete_or_cycle(key.mods.contains(Mods::SHIFT)),
            Key::Char(ch) => {
                if key.mods.contains(Mods::CTRL) && self.command_mode_cedit(&key) {
                    let pending = self.cmdline.text.clone();
                    self.cmdline = CommandLine::default();
                    self.cmdline_completion = None;
                    self.search_origin = None;
                    if let Err(error) = self.open_cmdwin(kind, false, &pending) {
                        self.set_error(error);
                    }
                    return;
                }
                if key.mods.contains(Mods::CTRL) {
                    if ch.eq_ignore_ascii_case(&'u') {
                        self.cmdline.text.clear();
                        self.cmdline.cursor = 0;
                    } else if ch.eq_ignore_ascii_case(&'w') {
                        while self.cmdline.cursor > 0
                            && !self.cmdline.text.as_bytes()[self.cmdline.cursor - 1]
                                .is_ascii_whitespace()
                        {
                            self.cmdline.text.remove(self.cmdline.cursor - 1);
                            self.cmdline.cursor -= 1;
                        }
                    }
                } else if !key.mods.contains(Mods::ALT) {
                    self.cmdline_completion = None;
                    self.cmdline.text.insert(self.cmdline.cursor, ch);
                    self.cmdline.cursor += ch.len_utf8();
                }
            }
            _ => {}
        }
        if kind == CommandKind::Search {
            self.update_incsearch();
        }
        self.request_redraw();
    }

    fn update_incsearch(&mut self) {
        if !self.options.get_bool("incsearch") {
            return;
        }
        let pattern = self.cmdline.text.clone();
        if pattern.is_empty() {
            if let Some(origin) = self.search_origin {
                self.set_cursor(origin);
            }
            return;
        }
        if let Ok(Some(pos)) = crate::search::search(self, &pattern, true) {
            self.set_cursor(pos);
        }
    }

    fn complete_runtime_dir(&self, directory: &str, prefix: &str) -> Vec<String> {
        let mut names = Vec::new();
        for root in self.runtime.expanded_runtimepath() {
            let Ok(entries) = std::fs::read_dir(root.join(directory)) else {
                continue;
            };
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    let name = name.strip_suffix(".vim").unwrap_or(name);
                    if name.starts_with(prefix) {
                        names.push(name.to_string());
                    }
                }
            }
        }
        names
    }

    fn complete_or_cycle(&mut self, reverse: bool) {
        if let Some(mut state) = self.cmdline_completion.take() {
            if !state.candidates.is_empty() {
                if reverse {
                    state.index =
                        (state.index + state.candidates.len() - 1) % state.candidates.len();
                } else {
                    state.index = (state.index + 1) % state.candidates.len();
                }
                let candidate = state.candidates[state.index].clone();
                if state.start <= self.cmdline.text.len() {
                    self.cmdline.text.truncate(state.start);
                    self.cmdline.text.push_str(&candidate);
                    self.cmdline.cursor = self.cmdline.text.len();
                }
                let message = state
                    .candidates
                    .iter()
                    .enumerate()
                    .map(|(index, candidate)| {
                        if index == state.index {
                            format!("[{candidate}]")
                        } else {
                            candidate.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                self.messages.info(message);
                self.cmdline_completion = Some(state);
            }
            return;
        }
        let text = self.cmdline.text.clone();
        let start = text
            .char_indices()
            .rev()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(index, ch)| index + ch.len_utf8())
            .unwrap_or(0);
        let prefix = &text[start..];
        let before = text[..start].trim();
        let set_context =
            before == "set" || before == "setlocal" || before == "setglobal" || before == "se";
        let mut candidates: Vec<String> = if set_context {
            let mut names = Vec::new();
            for spec in crate::options::specs() {
                if spec.name.starts_with(prefix) {
                    names.push(spec.name.to_string());
                }
                if !spec.short.is_empty() && spec.short.starts_with(prefix) {
                    names.push(spec.short.to_string());
                }
            }
            names
        } else if start == 0 {
            crate::registry::completions(prefix)
                .into_iter()
                .map(str::to_string)
                .collect()
        } else if matches!(before, "highlight" | "hi" | "hi!") {
            self.highlights
                .iter()
                .map(|(name, _)| name)
                .filter(|name| name.starts_with(prefix))
                .map(str::to_string)
                .collect()
        } else if matches!(before, "autocmd" | "au" | "doautocmd" | "doau") {
            crate::registry_generated::ALL_EVENTS
                .iter()
                .map(|spec| spec.name)
                .filter(|name| {
                    name.to_ascii_lowercase()
                        .starts_with(&prefix.to_ascii_lowercase())
                })
                .map(str::to_string)
                .collect()
        } else if matches!(before, "command" | "com") {
            self.user_commands
                .keys()
                .filter(|name| name.starts_with(prefix))
                .cloned()
                .collect()
        } else if matches!(
            before,
            "tag" | "ta" | "tjump" | "tju" | "tselect" | "ts" | "help" | "h"
        ) {
            self.tag_index
                .names()
                .filter(|name| name.starts_with(prefix))
                .map(str::to_string)
                .collect()
        } else if matches!(
            before,
            "buffer" | "b" | "bdelete" | "bd" | "bwipeout" | "bw" | "sbuffer" | "sb"
        ) {
            self.buffers
                .values()
                .map(|buffer| buffer.name_string())
                .filter(|name| !name.is_empty() && name.starts_with(prefix))
                .collect()
        } else if matches!(
            before,
            "function" | "fun" | "delfunction" | "delf" | "call" | "cal"
        ) || before.starts_with("call ")
        {
            let mut names: Vec<String> = crate::registry_generated::ALL_FUNCTIONS
                .iter()
                .map(|spec| spec.name.to_string())
                .filter(|name| name.starts_with(prefix))
                .collect();
            names.extend(
                self.viml
                    .function_names()
                    .into_iter()
                    .filter(|name| name.starts_with(prefix)),
            );
            names
        } else if matches!(before, "syntax" | "syn") {
            self.highlights
                .iter()
                .map(|(name, _)| name)
                .filter(|name| name.starts_with(prefix))
                .map(str::to_string)
                .collect()
        } else if matches!(before, "compiler" | "comp") {
            self.complete_runtime_dir("compiler", prefix)
        } else if matches!(before, "colorscheme" | "colo" | "colors") {
            self.complete_runtime_dir("colors", prefix)
        } else if prefix.starts_with('$') {
            std::env::vars()
                .map(|(name, _)| {
                    let mut value = String::from("$");
                    value.push_str(&name);
                    value
                })
                .filter(|name| name.starts_with(prefix))
                .collect()
        } else if prefix.starts_with("g:") {
            let mut names = Vec::new();
            if let Ok(globals) = self.viml.globals().try_borrow() {
                for entry in &globals.entries {
                    let name = format!("g:{}", entry.key.to_string_lossy());
                    if name.starts_with(prefix) {
                        names.push(name);
                    }
                }
            }
            names
        } else {
            complete_path(prefix)
        };
        candidates.sort();
        candidates.dedup();
        if candidates.is_empty() {
            return;
        }
        if candidates.len() == 1 {
            self.cmdline.text.truncate(start);
            self.cmdline.text.push_str(&candidates[0]);
            self.cmdline.cursor = self.cmdline.text.len();
            return;
        }
        let common = common_prefix(&candidates);
        if common.len() > prefix.len() {
            self.cmdline.text.truncate(start);
            self.cmdline.text.push_str(&common);
            self.cmdline.cursor = self.cmdline.text.len();
        }
        self.messages.info(candidates.join(" "));
        self.cmdline_completion = Some(CmdCompletion {
            start,
            candidates,
            index: 0,
        });
    }

    fn handle_visual_key(&mut self, key: KeyEvent) {
        if let Key::Char(ch) = key.key {
            if key.mods.contains(Mods::CTRL) {
                if ch == 'v' {
                    if let Mode::Visual { start, .. } = self.mode {
                        self.mode = Mode::Visual {
                            linewise: false,
                            block: true,
                            start,
                        };
                        self.request_redraw();
                    }
                }
                return;
            }
            if ch.is_ascii_digit() {
                self.accumulate_count(ch);
                return;
            }
        }
        match key.key {
            Key::Esc => {
                self.finish_visual();
            }
            Key::Char('v') => {
                self.finish_visual();
            }
            Key::Char('V') => {
                if let Mode::Visual {
                    linewise, start, ..
                } = self.mode.clone()
                {
                    self.mode = Mode::Visual {
                        linewise: !linewise,
                        block: false,
                        start,
                    };
                    self.request_redraw();
                }
            }
            Key::Char('o') => {
                if let Mode::Visual {
                    linewise,
                    block,
                    start,
                } = self.mode.clone()
                {
                    let cursor = self.cursor();
                    self.mode = Mode::Visual {
                        linewise,
                        block,
                        start: cursor,
                    };
                    self.set_cursor(start);
                }
            }
            Key::Char('d') | Key::Char('x') => self.delete_visual(),
            Key::Char('y') => self.yank_visual(),
            Key::Char('c') => self.change_visual(),
            Key::Char('~') => {
                let range = self.visual_range();
                if let Some(range) = range {
                    self.record_undo();
                    self.change_case_range(range, false);
                    self.commit_undo();
                    self.finish_visual();
                }
            }
            Key::Char('U') => {
                let range = self.visual_range();
                if let Some(range) = range {
                    self.record_undo();
                    self.change_case_range(range, true);
                    self.commit_undo();
                    self.finish_visual();
                }
            }
            Key::Char('J') => {
                let range = self.visual_range();
                if let Some(range) = range {
                    self.join_range(
                        rvim_ex::ResolvedRange::new(range.start.lnum, range.end.lnum),
                        true,
                    );
                    self.finish_visual();
                }
            }
            Key::Char(':') => {
                self.set_visual_marks();
                self.begin_command(CommandKind::Ex);
                self.cmdline.text = "'<,'>".to_string();
                self.cmdline.cursor = self.cmdline.text.len();
            }
            Key::Char('g') => {
                if let Key::Char(next) = key.key {
                    let _ = next;
                }
            }
            Key::Char(ch) => {
                let motion = match ch {
                    'h' => Some(Motion::Left),
                    'j' => Some(Motion::Down),
                    'k' => Some(Motion::Up),
                    'l' => Some(Motion::Right),
                    'w' => Some(Motion::WordForward),
                    'b' => Some(Motion::WordBackward),
                    'e' => Some(Motion::WordEnd),
                    'W' => Some(Motion::BigWordForward),
                    'B' => Some(Motion::BigWordBackward),
                    '0' => Some(Motion::LineStart),
                    '^' => Some(Motion::FirstNonBlank),
                    '$' => Some(Motion::LineEnd),
                    'G' => Some(Motion::FileEnd),
                    '{' => Some(Motion::ParagraphBackward),
                    '}' => Some(Motion::ParagraphForward),
                    _ => None,
                };
                if let Some(motion) = motion {
                    self.move_count(motion);
                }
            }
            _ => {}
        }
    }

    fn visual_range(&self) -> Option<rvim_edit::TextRange> {
        let Mode::Visual {
            linewise,
            block,
            start,
        } = self.mode
        else {
            return None;
        };
        let end = self.cursor();
        Some(if linewise {
            rvim_edit::TextRange::linewise(start.lnum, end.lnum)
        } else if block {
            rvim_edit::TextRange::blockwise(start, end)
        } else {
            rvim_edit::TextRange::characterwise(start, end, true)
        })
    }

    fn save_last_visual(&mut self) {
        if let Mode::Visual {
            linewise,
            block,
            start,
        } = self.mode
        {
            self.last_visual = Some((start, self.cursor(), linewise, block));
        }
    }

    fn finish_visual(&mut self) {
        self.save_last_visual();
        self.mode = Mode::Normal;
        self.request_redraw();
    }

    fn set_visual_marks(&mut self) {
        if let Mode::Visual { start, .. } = self.mode.clone() {
            let cursor = self.cursor();
            let (start, end) = if start <= cursor {
                (start, cursor)
            } else {
                (cursor, start)
            };
            if let Some(buffer) = self.current_buffer_mut() {
                buffer.marks.insert('<', start);
                buffer.marks.insert('>', end);
            }
        }
    }

    fn delete_visual(&mut self) {
        let range = self.visual_range();
        if let Some(range) = range {
            self.save_last_visual();
            self.apply_operator_range(Operator::Delete, range);
            self.mode = Mode::Normal;
        }
    }

    fn yank_visual(&mut self) {
        let range = self.visual_range();
        if let Some(range) = range {
            self.save_last_visual();
            self.apply_operator_range(Operator::Yank, range);
            self.mode = Mode::Normal;
        }
    }

    fn change_visual(&mut self) {
        let range = self.visual_range();
        if let Some(range) = range {
            self.save_last_visual();
            self.apply_operator_range(Operator::Change, range);
            // apply_operator_range switched to Insert for a change.
        }
    }

    pub(crate) fn sync_visual_selection_public(&mut self) {
        self.sync_visual_selection();
    }

    pub(crate) fn clear_search_highlight(&mut self) {
        self.search_highlight_active = false;
        self.request_redraw();
    }

    fn sync_visual_selection(&mut self) {
        for window in self.view.windows.values_mut() {
            window.visual = None;
        }
        if let Mode::Visual {
            linewise,
            block,
            start,
        } = self.mode
        {
            let end = self.cursor();
            if let Some(window) = self.view.current_window_mut() {
                window.visual = Some(rvim_view::VisualSelection {
                    start,
                    end,
                    linewise,
                    block,
                });
            }
        }
    }

    fn push_search_history(&mut self, pattern: &str) {
        if pattern.is_empty()
            || self
                .search_history
                .back()
                .is_some_and(|last| last == pattern)
        {
            return;
        }
        self.search_history.push_back(pattern.to_string());
        let limit = self.options.get_number("history").max(0) as usize;
        while self.search_history.len() > limit {
            self.search_history.pop_front();
        }
    }

    pub(crate) fn history(&self, kind: &str) -> Option<&VecDeque<String>> {
        match kind {
            ":" | "cmd" | "command" => Some(&self.command_history),
            "/" | "search" => Some(&self.search_history),
            _ => None,
        }
    }

    pub(crate) fn history_mut(&mut self, kind: &str) -> Option<&mut VecDeque<String>> {
        match kind {
            ":" | "cmd" | "command" => Some(&mut self.command_history),
            "/" | "search" => Some(&mut self.search_history),
            _ => None,
        }
    }

    fn set_search(&mut self, pattern: &str) {
        if pattern.is_empty() {
            return;
        }
        self.search_pattern = Some(pattern.to_string());
        self.search_highlight_active = true;
        self.push_search_history(pattern);
        self.last_search = Some((pattern.to_string(), 1));
        self.viml
            .set_vvar("searchforward", rvim_viml::Value::Number(1));
        self.search_next(false);
    }

    fn set_search_direction(&mut self, pattern: &str, forward: bool) {
        if pattern.is_empty() {
            return;
        }
        self.search_pattern = Some(pattern.to_string());
        self.search_highlight_active = true;
        self.push_search_history(pattern);
        self.last_search = Some((pattern.to_string(), if forward { 1 } else { -1 }));
        self.viml.set_vvar(
            "searchforward",
            rvim_viml::Value::Number(if forward { 1 } else { -1 }),
        );
        self.search_next(!forward);
    }

    fn search_next(&mut self, reverse: bool) {
        if let Some(pattern) = self.search_pattern.clone() {
            self.search_highlight_active = true;
            let result = crate::ex::simple_search(self, &pattern, !reverse);
            match result {
                Some(pos) => self.set_cursor(pos),
                None => self.set_error(format!("E486: Pattern not found: {pattern}")),
            }
        }
    }

    fn replace_chars(&mut self, ch: char, count: u32) {
        if self.deny_modification() {
            return;
        }
        let pos = self.cursor();
        let Some(buffer) = self.current_buffer() else {
            return;
        };
        let line = buffer.text.line(pos.lnum).unwrap_or(b"").to_vec();
        let mut output = Vec::with_capacity(line.len() + 4);
        let mut source = 0usize;
        let mut replaced = 0u32;
        while source < line.len() {
            if source == pos.col as usize && replaced < count {
                output.extend_from_slice(ch.to_string().as_bytes());
                source = next_char_boundary(&line, source);
                replaced += 1;
            } else {
                let next = next_char_boundary(&line, source);
                output.extend_from_slice(&line[source..next]);
                source = next;
            }
        }
        if replaced == 0 {
            return;
        }
        self.record_undo();
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.replace_line(pos.lnum, VimString::from(output));
        }
        self.commit_undo();
        self.set_cursor(pos);
        self.request_redraw();
    }

    fn delete_chars(&mut self, before: bool) {
        if self.deny_modification() {
            return;
        }
        let count = self.take_count();
        let pos = self.cursor();
        let line_len = self
            .current_buffer()
            .map_or(0, |b| b.text.line_len(pos.lnum));
        if line_len == 0 {
            return;
        }
        let start = if before {
            let mut col = pos.col;
            for _ in 0..count {
                col = self.prev_boundary(pos.lnum, col);
            }
            BufPos::new(pos.lnum, col)
        } else {
            pos
        };
        let mut end_col = pos.col;
        if before {
            if pos.col == 0 {
                return;
            }
        } else {
            for _ in 0..count {
                if end_col >= line_len {
                    break;
                }
                end_col = self.next_boundary(pos.lnum, end_col);
            }
        }
        let end = BufPos::new(pos.lnum, end_col.min(line_len));
        if start == end {
            return;
        }
        self.record_undo();
        let text = self.range_text(start, end, false);
        self.set_register(RegisterKind::Characterwise, text, false);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.delete_range(start, end);
        }
        self.set_cursor(start);
        self.commit_undo();
        self.request_redraw();
    }

    fn delete_to_end(&mut self) {
        let pos = self.cursor();
        let len = self
            .current_buffer()
            .map_or(0, |b| b.text.line_len(pos.lnum));
        if pos.col >= len {
            self.delete_chars(false);
            return;
        }
        self.record_undo();
        let text = self.range_text(pos, BufPos::new(pos.lnum, len), false);
        self.set_register(RegisterKind::Characterwise, text, false);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.delete_range(pos, BufPos::new(pos.lnum, len));
        }
        self.set_cursor(pos);
        self.commit_undo();
    }

    fn change_to_end(&mut self) {
        self.delete_to_end();
        self.enter_insert(InsertKind::Before);
    }

    fn yank_line(&mut self) {
        let line = self.cursor().lnum;
        let text = self
            .current_buffer()
            .and_then(|b| b.text.line(line))
            .map(|line| line.to_vec())
            .unwrap_or_default();
        self.set_register(RegisterKind::Linewise, vec![VimString::from(text)], true);
        self.messages.info("1 line yanked");
    }

    fn substitute_chars(&mut self) {
        self.delete_chars(false);
        self.enter_insert(InsertKind::Before);
    }

    fn change_line(&mut self) {
        let line = self.cursor().lnum;
        self.record_undo();
        let text = self
            .current_buffer()
            .and_then(|b| b.text.line(line))
            .map(|l| l.to_vec())
            .unwrap_or_default();
        self.set_register(RegisterKind::Linewise, vec![VimString::from(text)], false);
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.replace_line(line, VimString::new());
        }
        self.set_cursor(BufPos::new(line, 0));
        self.mode = Mode::Insert(InsertState {
            start: BufPos::new(line, 0),
            append: false,
            replace: false,
            undo_started: true,
        });
    }

    fn increment_number(&mut self, delta: i64, count: u32) {
        let pos = self.cursor();
        let Some(line) = self
            .current_buffer()
            .and_then(|b| b.text.line(pos.lnum))
            .map(|line| line.to_vec())
        else {
            return;
        };
        let mut start = pos.col as usize;
        while start < line.len() && !line[start].is_ascii_digit() && line[start] != b'-' {
            start += 1;
        }
        if start >= line.len() {
            return;
        }
        let negative = line[start] == b'-'
            && line
                .get(start + 1)
                .is_some_and(|byte| byte.is_ascii_digit());
        let digits_start = if negative { start + 1 } else { start };
        let mut end = digits_start;
        if line.get(end) == Some(&b'0') && matches!(line.get(end + 1), Some(b'x') | Some(b'X')) {
            end += 2;
            while end < line.len() && line[end].is_ascii_hexdigit() {
                end += 1;
            }
        } else {
            while end < line.len() && line[end].is_ascii_digit() {
                end += 1;
            }
        }
        if end == digits_start
            || (end == digits_start + 2
                && line
                    .get(digits_start + 1)
                    .is_some_and(|b| *b == b'x' || *b == b'X'))
        {
            return;
        }
        let text = String::from_utf8_lossy(&line[digits_start..end]).into_owned();
        let (base, digits) =
            if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                (16, rest)
            } else if text.len() > 1
                && text.starts_with('0')
                && text[1..].bytes().all(|byte| (b'0'..=b'7').contains(&byte))
            {
                (8, text.as_str())
            } else {
                (10, text.as_str())
            };
        let Ok(mut value) = i64::from_str_radix(digits, base) else {
            return;
        };
        if negative {
            value = -value;
        }
        value = value.saturating_add(delta.saturating_mul(count as i64));
        let replacement = if base == 16 {
            if value < 0 {
                format!("-0x{:x}", -value)
            } else {
                format!("0x{:x}", value)
            }
        } else if base == 8 && value >= 0 {
            format!("0{:o}", value)
        } else {
            value.to_string()
        };
        let mut next = Vec::new();
        next.extend_from_slice(&line[..start]);
        next.extend_from_slice(replacement.as_bytes());
        next.extend_from_slice(&line[end..]);
        self.record_undo();
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.text.replace_line(pos.lnum, VimString::from(next));
        }
        self.commit_undo();
        self.set_cursor(BufPos::new(pos.lnum, start as u32));
    }

    fn toggle_case(&mut self) {
        let count = self.take_count();
        let pos = self.cursor();
        self.record_undo();
        if let Some(buffer) = self.current_buffer_mut() {
            let line = buffer.text.line(pos.lnum).unwrap_or(b"").to_vec();
            let mut col = pos.col as usize;
            let mut result = line.clone();
            for _ in 0..count {
                if col >= result.len() {
                    break;
                }
                let (ch, len) =
                    rvim_types::unicode::char_at(&result, col).unwrap_or(('\u{FFFD}', 1));
                let replacement: String = if ch.is_uppercase() {
                    ch.to_lowercase().collect()
                } else {
                    ch.to_uppercase().collect()
                };
                result.splice(col..col + len, replacement.bytes());
                col += replacement.len();
            }
            buffer.text.replace_line(pos.lnum, VimString::from(result));
        }
        self.commit_undo();
        self.request_redraw();
    }

    fn join_lines(&mut self, count: u32, insert_space: bool) {
        if self.deny_modification() {
            return;
        }
        let start = self.cursor().lnum;
        if start >= self.line_count() {
            return;
        }
        let count = count.max(2);
        self.record_undo();
        for _ in 1..count {
            let lnum = start;
            if lnum >= self.line_count() {
                break;
            }
            let first_len = self.current_buffer().map_or(0, |b| b.text.line_len(lnum));
            let next = self.line_count().min(lnum + 1);
            let next_len = self.current_buffer().map_or(0, |b| b.text.line_len(next));
            let mut joined = self
                .current_buffer()
                .and_then(|b| b.text.line(lnum))
                .map(|l| l.to_vec())
                .unwrap_or_default();
            let second = self
                .current_buffer()
                .and_then(|b| b.text.line(next))
                .map(|l| l.to_vec())
                .unwrap_or_default();
            let no_space = !insert_space
                || joined.last().is_some_and(|b| *b == b' ' || *b == b'\t')
                || second.first().is_some_and(|b| *b == b')');
            if !no_space {
                joined.push(b' ');
            }
            joined.extend_from_slice(&second);
            if let Some(buffer) = self.current_buffer_mut() {
                buffer.text.replace_line(lnum, VimString::from(joined));
                buffer.text.delete_line(next);
            }
            let _ = (first_len, next_len);
            self.set_cursor(BufPos::new(
                lnum,
                if no_space { first_len } else { first_len + 1 },
            ));
        }
        self.commit_undo();
        self.request_redraw();
    }

    pub(crate) fn replace_line_range(&mut self, range: rvim_ex::ResolvedRange, lines: Vec<String>) {
        self.record_undo();
        if let Some(buffer) = self.current_buffer_mut() {
            buffer
                .text
                .lines
                .remove(range.start - 1, range.line_count());
            for (offset, line) in lines.iter().enumerate() {
                buffer.text.lines.insert(
                    range.start - 1 + offset as u32,
                    VimString::from(line.as_str()),
                );
            }
            if buffer.text.line_count() == 0 {
                buffer.text.lines.insert(0, VimString::new());
            }
            buffer.text.touch();
        }
        self.set_cursor(BufPos::new(range.start.min(self.line_count().max(1)), 0));
        self.commit_undo();
        self.request_redraw();
    }

    pub(crate) fn copy_lines(
        &mut self,
        source: rvim_ex::ResolvedRange,
        target: u32,
    ) -> Result<(), VimError> {
        let lines: Vec<VimString> = (source.start..=source.end)
            .filter_map(|lnum| {
                self.current_buffer()
                    .and_then(|buffer| buffer.text.line_vim(lnum))
                    .cloned()
            })
            .collect();
        self.record_undo();
        let target = target.min(self.line_count());
        let insert_at = if target == 0 { 0 } else { target };
        if let Some(buffer) = self.current_buffer_mut() {
            for (offset, line) in lines.iter().enumerate() {
                buffer
                    .text
                    .lines
                    .insert(insert_at + offset as u32, line.clone());
            }
            buffer.text.touch();
        }
        let cursor_line = insert_at + 1;
        self.set_cursor(BufPos::new(cursor_line, 0));
        self.commit_undo();
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn move_lines(
        &mut self,
        source: rvim_ex::ResolvedRange,
        target: u32,
    ) -> Result<(), VimError> {
        if source.start <= target && target <= source.end {
            return Ok(());
        }
        self.record_undo();
        let lines = self.delete_linewise_for_move(source.start, source.end);
        let count = lines.len() as u32;
        let insert_index = if target >= source.end {
            target.saturating_sub(count)
        } else {
            target
        };
        if let Some(buffer) = self.current_buffer_mut() {
            for (offset, line) in lines.iter().enumerate() {
                buffer
                    .text
                    .lines
                    .insert(insert_index + offset as u32, line.clone());
            }
            buffer.text.touch();
        }
        self.set_cursor(BufPos::new(insert_index + 1, 0));
        self.commit_undo();
        self.request_redraw();
        Ok(())
    }

    fn delete_linewise_for_move(&mut self, start: u32, end: u32) -> Vec<VimString> {
        let mut lines = Vec::new();
        if let Some(buffer) = self.current_buffer_mut() {
            lines = (start..=end)
                .filter_map(|lnum| buffer.text.line_vim(lnum).cloned())
                .collect();
            buffer.text.lines.remove(start - 1, end - start + 1);
            if buffer.text.line_count() == 0 {
                buffer.text.lines.insert(0, VimString::new());
            }
            buffer.text.touch();
        }
        lines
    }

    pub(crate) fn substitute_lines(
        &mut self,
        mut replacements: Vec<(u32, Vec<u8>)>,
        pattern: &str,
    ) {
        if replacements.is_empty() {
            return;
        }
        self.record_undo();
        replacements.sort_by_key(|(lnum, _)| std::cmp::Reverse(*lnum));
        for (lnum, bytes) in replacements {
            let Some(buffer) = self.current_buffer_mut() else {
                continue;
            };
            if !bytes.contains(&b'\n') {
                buffer.text.replace_line(lnum, VimString::from(bytes));
                continue;
            }
            let new_lines: Vec<VimString> = bytes
                .split(|byte| *byte == b'\n')
                .map(VimString::from)
                .collect();
            buffer.text.lines.remove(lnum - 1, 1);
            for (offset, line) in new_lines.into_iter().enumerate() {
                buffer.text.lines.insert(lnum - 1 + offset as u32, line);
            }
            buffer.text.touch();
        }
        self.search_pattern = Some(pattern.to_string());
        self.last_search = Some((pattern.to_string(), 1));
        self.commit_undo();
        self.request_redraw();
    }

    pub(crate) fn undo_public(&mut self) {
        self.undo();
    }

    pub(crate) fn redo_public(&mut self) {
        self.redo();
    }

    pub(crate) fn undo_summary(&self) -> String {
        if let Some(buffer) = self.current_buffer() {
            format!(
                "undo {}  redo {}  changedtick {}",
                buffer.undo.undo_count(),
                buffer.undo.redo_count(),
                buffer.text.meta.changedtick
            )
        } else {
            String::new()
        }
    }

    fn undo(&mut self) {
        let cursor = self.cursor();
        let Some(buffer) = self.current_buffer_mut() else {
            return;
        };
        if let Some(pos) = buffer.undo.undo(&mut buffer.text, cursor) {
            self.set_cursor(pos);
            self.set_message("1 change; before #1");
        } else {
            self.set_error("Already at oldest change");
        }
        self.request_redraw();
    }

    fn redo(&mut self) {
        let cursor = self.cursor();
        let Some(buffer) = self.current_buffer_mut() else {
            return;
        };
        if let Some(pos) = buffer.undo.redo(&mut buffer.text, cursor) {
            self.set_cursor(pos);
            self.set_message("1 change; after #1");
        } else {
            self.set_error("Already at newest change");
        }
        self.request_redraw();
    }

    pub(crate) fn record_undo(&mut self) {
        let cursor = self.cursor();
        let Some(buffer) = self.current_buffer_mut() else {
            return;
        };
        buffer.undo.record(&buffer.text, cursor);
    }

    pub(crate) fn commit_undo(&mut self) {
        let cursor = self.cursor();
        let after_cursor = self.cursor();
        let Some(buffer) = self.current_buffer_mut() else {
            return;
        };
        if buffer.undo.commit(&buffer.text, after_cursor) {
            buffer.text.set_modified(true);
            buffer.marks.insert('.', cursor);
            self.changelist.push((self.current_buffer, cursor));
            while self.changelist.len() > 100 {
                self.changelist.remove(0);
            }
            self.change_index = self.changelist.len().saturating_sub(1);
        }
        self.update_swap_for(self.current_buffer);
    }

    fn delete_linewise(&mut self, start: u32, end: u32) -> Vec<VimString> {
        let start = start.max(1);
        let end = end.min(self.line_count()).max(start);
        let lines: Vec<VimString> = (start..=end)
            .filter_map(|lnum| {
                self.current_buffer()
                    .and_then(|b| b.text.line_vim(lnum))
                    .cloned()
            })
            .collect();
        if let Some(buffer) = self.current_buffer_mut() {
            let count = end - start + 1;
            buffer.text.lines.remove(start - 1, count);
            if buffer.text.line_count() == 0 {
                buffer.text.lines.insert(0, VimString::new());
            }
            buffer.text.touch();
        }
        let new_line = start.min(self.line_count().max(1));
        let col = self
            .current_buffer()
            .and_then(|b| b.text.line(new_line))
            .map_or(0, |l| {
                l.iter()
                    .position(|b| *b != b' ' && *b != b'\t')
                    .unwrap_or(0) as u32
            });
        self.set_cursor(BufPos::new(new_line, col));
        lines
    }

    fn apply_linewise_operator(&mut self, op: Operator, count: u32) {
        let start = self.cursor().lnum;
        let end = (start + count - 1).min(self.line_count().max(1));
        let range = rvim_edit::TextRange::linewise(start, end);
        self.apply_operator_range(op, range);
    }

    fn apply_operator_motion(&mut self, op: Operator, motion: Motion, count: u32) {
        let start = self.cursor();
        let Some(buffer) = self.current_buffer() else {
            return;
        };
        let linewise = matches!(
            motion,
            Motion::Up
                | Motion::Down
                | Motion::FileStart
                | Motion::FileEnd
                | Motion::ParagraphBackward
                | Motion::ParagraphForward
        );
        if linewise {
            let mut target = start;
            for _ in 0..count {
                let next = move_cursor(
                    &buffer.text,
                    target,
                    motion,
                    self.options.get_number("tabstop") as usize,
                );
                if next == target {
                    break;
                }
                target = next;
            }
            let range = rvim_edit::TextRange::linewise(start.lnum, target.lnum);
            self.apply_operator_range(op, range);
            return;
        }
        let effective = if op == Operator::Change && motion == Motion::WordForward {
            Motion::WordEnd
        } else {
            motion
        };
        let mut target = start;
        for _ in 0..count {
            let next = move_cursor(
                &buffer.text,
                target,
                effective,
                self.options.get_number("tabstop") as usize,
            );
            if next == target {
                break;
            }
            target = next;
        }
        let mut inclusive = matches!(
            effective,
            Motion::WordEnd
                | Motion::BigWordEnd
                | Motion::LineEnd
                | Motion::MatchBracket
                | Motion::WordEndBackward
                | Motion::BigWordEndBackward
                | Motion::FindChar { .. }
        );
        let line_len = self
            .current_buffer()
            .map_or(0, |buffer| buffer.text.line_len(start.lnum));
        if effective == Motion::LineEnd || (target.lnum > start.lnum && target.col == 0) {
            target = BufPos::new(start.lnum, line_len);
            inclusive = false;
        }
        let range = rvim_edit::TextRange::characterwise(start, target, inclusive);
        self.apply_operator_range(op, range);
    }

    fn block_segments(&self, range: rvim_edit::TextRange) -> Vec<(u32, u32, u32)> {
        let Some(buffer) = self.current_buffer() else {
            return Vec::new();
        };
        let start_col = range.start.col.min(range.end.col);
        let end_col = range.start.col.max(range.end.col);
        let mut segments = Vec::new();
        for lnum in range.start.lnum..=range.end.lnum {
            let len = buffer.text.line_len(lnum);
            let start = start_col.min(len);
            let end = (end_col.saturating_add(1)).min(len);
            if end > start {
                segments.push((lnum, start, end));
            }
        }
        segments
    }

    fn apply_block_operator(&mut self, op: Operator, range: rvim_edit::TextRange) {
        let segments = self.block_segments(range);
        let lines: Vec<VimString> = segments
            .iter()
            .filter_map(|(lnum, start, end)| {
                self.current_buffer()
                    .and_then(|buffer| buffer.text.line(*lnum))
                    .map(|line| VimString::from(&line[*start as usize..*end as usize]))
            })
            .collect();
        match op {
            Operator::Fold => {
                let _ = self.create_fold(range.start.lnum, range.end.lnum.max(range.start.lnum));
                self.set_cursor(BufPos::new(range.start.lnum, 0));
            }
            Operator::Yank => {
                self.set_register(RegisterKind::Blockwise, lines, true);
                self.set_cursor(range.start);
            }
            Operator::Delete | Operator::Change => {
                self.set_register(RegisterKind::Blockwise, lines, false);
                for (lnum, start, end) in segments.iter().rev() {
                    if let Some(buffer) = self.current_buffer_mut() {
                        buffer
                            .text
                            .delete_range(BufPos::new(*lnum, *start), BufPos::new(*lnum, *end));
                    }
                }
                let len = self
                    .current_buffer()
                    .map_or(0, |buffer| buffer.text.line_len(range.start.lnum));
                self.set_cursor(BufPos::new(range.start.lnum, range.start.col.min(len)));
                if op == Operator::Change {
                    self.mode = Mode::Insert(InsertState {
                        start: self.cursor(),
                        append: false,
                        replace: false,
                        undo_started: true,
                    });
                }
            }
            Operator::UpperCase | Operator::LowerCase => {
                for (lnum, start, end) in segments {
                    if let Some(buffer) = self.current_buffer_mut() {
                        let line = buffer.text.line(lnum).unwrap_or(b"").to_vec();
                        let mut text = String::from_utf8_lossy(&line[start as usize..end as usize])
                            .into_owned();
                        text = if op == Operator::UpperCase {
                            text.to_uppercase()
                        } else {
                            text.to_lowercase()
                        };
                        buffer
                            .text
                            .delete_range(BufPos::new(lnum, start), BufPos::new(lnum, end));
                        buffer.text.insert_bytes(lnum, start, text.as_bytes());
                    }
                }
                self.set_cursor(range.start);
            }
            _ => {}
        }
    }

    fn apply_operator_range(&mut self, op: Operator, range: rvim_edit::TextRange) {
        self.record_undo();
        if range.kind == rvim_edit::RangeKind::Blockwise {
            self.apply_block_operator(op, range);
            self.commit_undo();
            self.request_redraw();
            return;
        }
        match op {
            Operator::Delete | Operator::Change => {
                let lines = match range.kind {
                    rvim_edit::RangeKind::Linewise => {
                        self.delete_linewise(range.start.lnum, range.end.lnum)
                    }
                    _ => {
                        let (start, end) = self.range_exclusive(range);
                        let text = self.range_text(start, end, false);
                        if let Some(buffer) = self.current_buffer_mut() {
                            buffer.text.delete_range(start, end);
                            if buffer.text.line_count() == 0 {
                                buffer.text.lines.insert(0, VimString::new());
                            }
                        }
                        self.set_cursor(start);
                        text
                    }
                };
                let kind = match range.kind {
                    rvim_edit::RangeKind::Linewise => RegisterKind::Linewise,
                    rvim_edit::RangeKind::Blockwise => RegisterKind::Blockwise,
                    rvim_edit::RangeKind::Characterwise => RegisterKind::Characterwise,
                };
                self.set_register(kind, lines, false);
                if op == Operator::Change {
                    self.mode = Mode::Insert(InsertState {
                        start: self.cursor(),
                        append: false,
                        replace: false,
                        undo_started: true,
                    });
                }
            }
            Operator::Yank => {
                let lines = match range.kind {
                    rvim_edit::RangeKind::Linewise => (range.start.lnum..=range.end.lnum)
                        .filter_map(|lnum| {
                            self.current_buffer()
                                .and_then(|b| b.text.line_vim(lnum))
                                .cloned()
                        })
                        .collect(),
                    _ => {
                        let (start, end) = self.range_exclusive(range);
                        self.range_lines(start, end)
                    }
                };
                let kind = match range.kind {
                    rvim_edit::RangeKind::Linewise => RegisterKind::Linewise,
                    rvim_edit::RangeKind::Blockwise => RegisterKind::Blockwise,
                    rvim_edit::RangeKind::Characterwise => RegisterKind::Characterwise,
                };
                self.set_register(kind, lines, true);
                self.set_cursor(range.start);
            }
            Operator::UpperCase | Operator::LowerCase => {
                self.change_case_range(range, op == Operator::UpperCase);
            }
            _ => {}
        }
        self.commit_undo();
        self.request_redraw();
    }

    fn range_exclusive(&self, range: rvim_edit::TextRange) -> (BufPos, BufPos) {
        let start = range.start;
        let mut end = range.end;
        if range.inclusive {
            let len = self
                .current_buffer()
                .map_or(0, |b| b.text.line_len(end.lnum));
            if end.col < len {
                end = BufPos::new(end.lnum, self.next_boundary(end.lnum, end.col));
            } else if end.lnum < self.line_count() {
                end = BufPos::new(end.lnum + 1, 0);
            }
        }
        (start, end)
    }

    fn range_text(&self, start: BufPos, end: BufPos, _linewise: bool) -> Vec<VimString> {
        self.range_lines(start, end)
    }

    fn range_lines(&self, start: BufPos, end: BufPos) -> Vec<VimString> {
        let Some(buffer) = self.current_buffer() else {
            return Vec::new();
        };
        if start.lnum == end.lnum {
            let line = buffer.text.line(start.lnum).unwrap_or(b"");
            let start_col = (start.col as usize).min(line.len());
            let end_col = (end.col as usize).min(line.len()).max(start_col);
            return vec![VimString::from(&line[start_col..end_col])];
        }
        let mut out = Vec::new();
        let first = buffer.text.line(start.lnum).unwrap_or(b"");
        out.push(VimString::from(
            &first[(start.col as usize).min(first.len())..],
        ));
        for lnum in start.lnum + 1..end.lnum {
            if let Some(line) = buffer.text.line_vim(lnum) {
                out.push(line.clone());
            }
        }
        let last = buffer.text.line(end.lnum).unwrap_or(b"");
        out.push(VimString::from(&last[..(end.col as usize).min(last.len())]));
        out
    }

    fn change_case_range(&mut self, range: rvim_edit::TextRange, upper: bool) {
        let start = range.start.lnum;
        let end = range.end.lnum;
        let mut current = self.cursor();
        for lnum in start..=end {
            let Some(line) = self
                .current_buffer()
                .and_then(|b| b.text.line(lnum))
                .map(|l| l.to_vec())
            else {
                continue;
            };
            let result: Vec<u8> = match std::str::from_utf8(&line) {
                Ok(text) => if upper {
                    text.to_uppercase()
                } else {
                    text.to_lowercase()
                }
                .into_bytes(),
                Err(_) => line
                    .iter()
                    .map(|b| {
                        if upper {
                            b.to_ascii_uppercase()
                        } else {
                            b.to_ascii_lowercase()
                        }
                    })
                    .collect(),
            };
            if let Some(buffer) = self.current_buffer_mut() {
                buffer.text.replace_line(lnum, VimString::from(result));
            }
        }
        self.set_cursor(current);
        current.lnum = start;
        self.set_cursor(current);
    }

    fn set_register(&mut self, kind: RegisterKind, lines: Vec<VimString>, yank: bool) {
        let register = self
            .pending_register
            .take()
            .and_then(RegisterName::parse)
            .unwrap_or(RegisterName::Unnamed);
        let content = RegisterContent::new(lines, kind);
        if yank {
            self.registers.yank(register, content.clone());
        } else {
            self.registers.delete(register, content.clone());
        }
        if matches!(register, RegisterName::Clipboard | RegisterName::Selection) {
            let platform_kind = if register == RegisterName::Selection {
                rvim_platform::ClipboardKind::Selection
            } else {
                rvim_platform::ClipboardKind::Unnamed
            };
            if let Err(error) =
                rvim_platform::clipboard_set(platform_kind, &content.text().to_string_lossy())
            {
                self.messages.error(format!("clipboard: {error}"));
            }
        }
    }

    fn put(&mut self, before: bool) {
        self.put_from(None, before);
    }

    fn put_from(&mut self, named: Option<RegisterName>, before: bool) {
        if self.deny_modification() {
            return;
        }
        let register = named
            .or_else(|| self.pending_register.take().and_then(RegisterName::parse))
            .unwrap_or(RegisterName::Unnamed);
        let content = self.registers.get(register).cloned().or_else(|| {
            if matches!(register, RegisterName::Clipboard | RegisterName::Selection) {
                let kind = if register == RegisterName::Selection {
                    rvim_platform::ClipboardKind::Selection
                } else {
                    rvim_platform::ClipboardKind::Unnamed
                };
                rvim_platform::clipboard_get(kind)
                    .ok()
                    .flatten()
                    .map(|text| {
                        RegisterContent::new(
                            vec![VimString::from(text)],
                            RegisterKind::Characterwise,
                        )
                    })
            } else {
                None
            }
        });
        let Some(content) = content else {
            self.set_error("E353: Nothing in register");
            return;
        };
        self.record_undo();
        let cursor = self.cursor();
        match content.kind {
            RegisterKind::Linewise => {
                let insert_at = if before { cursor.lnum } else { cursor.lnum + 1 };
                if let Some(buffer) = self.current_buffer_mut() {
                    for (index, line) in (insert_at - 1..).zip(content.lines.iter()) {
                        buffer.text.lines.insert(index, line.clone());
                    }
                    buffer.text.touch();
                }
                let col = self
                    .current_buffer()
                    .and_then(|b| b.text.line(insert_at))
                    .map_or(0, |l| {
                        l.iter()
                            .position(|b| *b != b' ' && *b != b'\t')
                            .unwrap_or(0) as u32
                    });
                self.set_cursor(BufPos::new(insert_at, col));
            }
            RegisterKind::Characterwise | RegisterKind::Blockwise => {
                let line_len = self
                    .current_buffer()
                    .map_or(0, |b| b.text.line_len(cursor.lnum));
                let mut col = if before {
                    cursor.col
                } else {
                    cursor.col.saturating_add(1).min(line_len)
                };
                if line_len == 0 {
                    col = 0;
                }
                let mut text = Vec::new();
                for (index, line) in content.lines.iter().enumerate() {
                    if index > 0 {
                        text.push(b'\n');
                    }
                    text.extend_from_slice(line.as_bytes());
                }
                let new_pos = if let Some(buffer) = self.current_buffer_mut() {
                    buffer.text.insert_bytes(cursor.lnum, col, &text)
                } else {
                    return;
                };
                self.set_cursor(new_pos);
            }
        }
        self.commit_undo();
        self.request_redraw();
    }

    fn text_object_range(&self, around: bool, word: bool) -> Option<rvim_edit::TextRange> {
        let pos = self.cursor();
        let buffer = self.current_buffer()?;
        let line = buffer.text.line(pos.lnum).unwrap_or(b"");
        if word {
            let mut start = pos.col as usize;
            while start > 0 && !line[start - 1].is_ascii_whitespace() {
                start -= 1;
            }
            let mut end = pos.col as usize;
            while end < line.len() && !line[end].is_ascii_whitespace() {
                end += 1;
            }
            if around {
                while start > 0 && line[start - 1].is_ascii_whitespace() {
                    start -= 1;
                }
                while end < line.len() && line[end].is_ascii_whitespace() {
                    end += 1;
                }
            }
            let inclusive = end > start;
            Some(rvim_edit::TextRange::characterwise(
                BufPos::new(pos.lnum, start as u32),
                BufPos::new(pos.lnum, end.saturating_sub(1) as u32),
                inclusive,
            ))
        } else {
            Some(rvim_edit::TextRange::characterwise(pos, pos, false))
        }
    }

    fn buffer_tick(&self) -> u64 {
        self.current_buffer()
            .map_or(0, |buffer| buffer.text.meta.changedtick)
    }

    fn maybe_finish_change(&mut self) {
        if self.replaying || !matches!(self.mode, Mode::Normal) {
            return;
        }
        if self.pending_operator.is_some()
            || self.pending_char.is_some()
            || self.pending_g
            || self.pending_ctrl_w
            || self.pending_z
            || self.pending_register.is_some()
            || self.pending_count.is_some()
        {
            return;
        }
        if self.change_keys.is_empty() || self.buffer_tick() == self.change_start_tick {
            return;
        }
        let first = &self.change_keys[0];
        let undo_key = matches!(first.key, Key::Char('u')) && first.mods.is_empty();
        let redo_key = matches!(first.key, Key::Char('r')) && first.mods.contains(Mods::CTRL);
        let time_travel = self.change_keys.len() >= 2
            && self.change_keys[0].key == Key::Char('g')
            && matches!(self.change_keys[1].key, Key::Char('-') | Key::Char('+'));
        if undo_key || redo_key || time_travel {
            self.change_keys.clear();
            return;
        }
        self.last_change_keys = std::mem::take(&mut self.change_keys);
        self.change_start_tick = self.buffer_tick();
    }

    fn repeat_change(&mut self, count: u32) {
        if self.last_change_keys.is_empty() {
            let Some(change) = self.last_change.clone() else {
                return;
            };
            if !change.insert || change.text.is_empty() {
                return;
            }
            for _ in 0..count {
                let pos = self.cursor();
                self.record_undo();
                if let Some(buffer) = self.current_buffer_mut() {
                    let _ = buffer
                        .text
                        .insert_bytes(pos.lnum, pos.col, change.text.as_bytes());
                    buffer.text.touch();
                }
                self.commit_undo();
            }
            self.request_redraw();
            return;
        }
        let keys = self.last_change_keys.clone();
        self.replaying = true;
        for _ in 0..count {
            for key in &keys {
                self.handle_key(key.clone());
            }
        }
        self.replaying = false;
        self.change_keys.clear();
        self.change_start_tick = self.buffer_tick();
        self.request_redraw();
    }

    fn find_package(&self, name: &str) -> Option<std::path::PathBuf> {
        let packpath = self.options.get_string("packpath");
        for entry in rvim_runtime::split_runtimepath(&packpath) {
            let entry = rvim_platform::fs::expand_env(&entry.to_string_lossy());
            let packroot = entry.join("pack");
            let Ok(packages) = std::fs::read_dir(&packroot) else {
                continue;
            };
            for package in packages.flatten() {
                for kind in ["start", "opt"] {
                    let candidate = package.path().join(kind).join(name);
                    if candidate.is_dir() {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }

    fn add_runtimepath_entry(&mut self, dir: &std::path::Path) {
        let mut runtimepath = self.options.get_string("runtimepath");
        if !runtimepath.is_empty() {
            runtimepath.push(',');
        }
        runtimepath.push_str(&dir.to_string_lossy());
        let _ = self.options.set_global(
            "runtimepath",
            crate::options::OptionValue::String(VimString::from(runtimepath.clone())),
        );
        self.runtime = RuntimePaths::discover(
            self.runtime.vimruntime().map(|path| path.to_path_buf()),
            Some(runtimepath),
        );
    }

    fn source_package_plugins(&mut self, dir: &std::path::Path) -> Result<(), VimError> {
        let mut files = Vec::new();
        collect_vim_files(&dir.join("plugin"), &mut files);
        files.sort();
        for path in files {
            let text = rvim_platform::fs::read_file(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            self.source_viml(&text, &path.display().to_string())?;
        }
        Ok(())
    }

    pub(crate) fn packadd(&mut self, name: &str, load: bool) -> Result<(), VimError> {
        let Some(dir) = self.find_package(name) else {
            return Err(VimError::new(
                919,
                format!("E919: Directory not found in 'packpath': pack/*/opt/{name}"),
            ));
        };
        if !self.loaded_packages.insert(name.to_string()) {
            return Ok(());
        }
        self.add_runtimepath_entry(&dir);
        if load {
            self.source_package_plugins(&dir)?;
        }
        Ok(())
    }

    pub(crate) fn packloadall(&mut self) -> Result<(), VimError> {
        let packpath = self.options.get_string("packpath");
        let mut names = Vec::new();
        for entry in rvim_runtime::split_runtimepath(&packpath) {
            let entry = rvim_platform::fs::expand_env(&entry.to_string_lossy());
            let Ok(packages) = std::fs::read_dir(entry.join("pack")) else {
                continue;
            };
            for package in packages.flatten() {
                let Ok(entries) = std::fs::read_dir(package.path().join("start")) else {
                    continue;
                };
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        }
        names.sort();
        names.dedup();
        for name in names {
            self.packadd(&name, true)?;
        }
        Ok(())
    }

    pub(crate) fn update_window_buffer(&mut self) {
        if let Some(window) = self.view.current_window_mut() {
            window.buffer = self.current_buffer;
        }
    }

    pub(crate) fn sync_window_options(&mut self) {
        let ids: Vec<_> = self.view.windows.keys().collect();
        let laststatus = self.options.get_number("laststatus");
        self.view.global_statusline = laststatus >= 3;
        let show_status = if laststatus >= 3 {
            false
        } else if self.view.windows.len() <= 1 {
            laststatus >= 2
        } else {
            laststatus != 0
        };
        let current_window_id = self.current_window_id();
        let mut global_rows = 1u16;
        for id in ids {
            let number = self.options.window_bool(rvim_types::Id::raw(id), "number");
            let relativenumber = self
                .options
                .window_bool(rvim_types::Id::raw(id), "relativenumber");
            let wrap = self.options.window_bool(rvim_types::Id::raw(id), "wrap");
            let linebreak = self
                .options
                .window_bool(rvim_types::Id::raw(id), "linebreak");
            let breakat = self
                .options
                .window_string(rvim_types::Id::raw(id), "breakat");
            let showbreak = self
                .options
                .window_string(rvim_types::Id::raw(id), "showbreak");
            let breakindent = self
                .options
                .window_bool(rvim_types::Id::raw(id), "breakindent");
            let smoothscroll = self
                .options
                .window_bool(rvim_types::Id::raw(id), "smoothscroll");
            let list = self.options.window_bool(rvim_types::Id::raw(id), "list");
            let listchars = self
                .options
                .window_string(rvim_types::Id::raw(id), "listchars");
            let cursorline = self
                .options
                .window_bool(rvim_types::Id::raw(id), "cursorline");
            let cursorcolumn = self
                .options
                .window_bool(rvim_types::Id::raw(id), "cursorcolumn");
            let colorcolumn = self
                .options
                .window_string(rvim_types::Id::raw(id), "colorcolumn");
            let statusline = self
                .options
                .window_string(rvim_types::Id::raw(id), "statusline");
            let statuslineopt = self
                .options
                .window_string(rvim_types::Id::raw(id), "statuslineopt");
            let maxheight = statuslineopt
                .split(',')
                .find_map(|item| item.trim().strip_prefix("maxheight:"))
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(1)
                .max(1);
            let fixedheight = statuslineopt
                .split(',')
                .any(|item| item.trim() == "fixedheight");
            let newlines = statusline.matches("%@").count() as u16;
            let window_height = self.view.windows.get(&id).map_or(1, |window| window.height);
            let maxheight = maxheight.min(window_height.saturating_sub(1).max(1));
            let tabstop = self
                .options
                .buffer_number(self.current_buffer_id(), "tabstop")
                .max(1) as usize;
            let shiftwidth = self
                .options
                .buffer_number(self.current_buffer_id(), "shiftwidth")
                .max(1) as usize;
            let scrolloff = self
                .options
                .window_number(rvim_types::Id::raw(id), "scrolloff")
                .max(0) as usize;
            let sidescrolloff = self
                .options
                .window_number(rvim_types::Id::raw(id), "sidescrolloff")
                .max(0) as usize;
            let numberwidth = self
                .options
                .window_number(rvim_types::Id::raw(id), "numberwidth")
                .clamp(1, 20) as u8;
            let numberformat = self
                .options
                .window_string(rvim_types::Id::raw(id), "numberformat");
            let foldcolumn = self
                .options
                .window_number(rvim_types::Id::raw(id), "foldcolumn")
                .clamp(0, 12) as u8;
            let signcolumn = self
                .options
                .window_string(rvim_types::Id::raw(id), "signcolumn");
            let winfixheight = self
                .options
                .window_bool(rvim_types::Id::raw(id), "winfixheight");
            let winfixwidth = self
                .options
                .window_bool(rvim_types::Id::raw(id), "winfixwidth");
            let fillchars = self
                .options
                .window_string(rvim_types::Id::raw(id), "fillchars");
            let foldmethod = self
                .options
                .window_string(rvim_types::Id::raw(id), "foldmethod");
            let foldmarker = self
                .options
                .window_string(rvim_types::Id::raw(id), "foldmarker");
            let foldignore = self
                .options
                .window_string(rvim_types::Id::raw(id), "foldignore");
            let foldminlines = self
                .options
                .window_number(rvim_types::Id::raw(id), "foldminlines");
            let foldnestmax = self
                .options
                .window_number(rvim_types::Id::raw(id), "foldnestmax");
            let foldtext = self
                .options
                .window_string(rvim_types::Id::raw(id), "foldtext");
            let foldexpr = self
                .options
                .window_string(rvim_types::Id::raw(id), "foldexpr");
            let _ = sidescrolloff;
            if let Some(window) = self.view.window_mut(id) {
                window.options.number = number;
                window.options.relativenumber = relativenumber;
                window.options.wrap = wrap;
                window.options.linebreak = linebreak;
                window.options.breakat = breakat;
                window.options.showbreak = showbreak;
                window.options.breakindent = breakindent;
                window.options.smoothscroll = smoothscroll;
                window.options.list = list;
                window.options.listchars = listchars;
                window.options.cursorline = cursorline;
                window.options.cursorcolumn = cursorcolumn;
                window.options.colorcolumn = colorcolumn;
                window.options.statusline = (!statusline.is_empty()).then_some(statusline);
                window.options.tabstop = tabstop;
                window.options.shiftwidth = shiftwidth;
                window.options.numberwidth = numberwidth;
                window.options.numberformat = numberformat.clone();
                window.options.foldcolumn = foldcolumn;
                window.options.signcolumn = signcolumn.clone();
                window.options.winfixheight = winfixheight;
                window.options.winfixwidth = winfixwidth;
                window.options.fillchars = fillchars;
                window.options.foldmethod = foldmethod;
                window.options.foldmarker = foldmarker;
                window.options.foldignore = foldignore;
                window.options.foldminlines = foldminlines;
                window.options.foldnestmax = foldnestmax;
                window.options.foldtext = foldtext;
                window.options.foldexpr = foldexpr;
                window.options.scrolloff = scrolloff;
                window.options.sidescrolloff = sidescrolloff;
                let wanted = if fixedheight {
                    maxheight
                } else {
                    (1 + newlines).min(maxheight)
                };
                if rvim_types::Id::raw(id) == current_window_id {
                    global_rows = wanted.max(1);
                }
                window.statusline_rows = if show_status { wanted } else { 0 };
            }
        }
        self.view.global_statusline_rows = if laststatus >= 3 {
            global_rows.max(1)
        } else {
            0
        };
        self.sync_diffs();
        self.sync_folds();
        self.sync_signs();
    }

    fn next_boundary(&self, lnum: u32, col: u32) -> u32 {
        let line = self
            .current_buffer()
            .and_then(|b| b.text.line(lnum))
            .unwrap_or(b"");
        rvim_types::unicode::char_at(line, col as usize)
            .map(|(ch, _)| col + ch.len_utf8() as u32)
            .unwrap_or(col + 1)
    }

    fn prev_boundary(&self, lnum: u32, col: u32) -> u32 {
        let line = self
            .current_buffer()
            .and_then(|b| b.text.line(lnum))
            .unwrap_or(b"");
        let mut i = col.min(line.len() as u32).saturating_sub(1);
        while i > 0 && (line[i as usize] & 0xC0) == 0x80 {
            i -= 1;
        }
        i
    }

    fn prev_char_pos(&self, pos: BufPos) -> BufPos {
        BufPos::new(pos.lnum, self.prev_boundary(pos.lnum, pos.col))
    }

    pub fn fire_event(&mut self, event: Event) {
        if self.noautocmd_depth > 0 || self.autocmds.is_firing() {
            return;
        }
        let buffer = self.current_buffer;
        let file = self
            .current_buffer()
            .and_then(|buffer| buffer.text.meta.name.clone());
        let file = file.as_ref().map(|path| path.display().to_string());
        let subject = match &event {
            Event::FileType | Event::Syntax => self
                .current_buffer()
                .map(|buffer| buffer.filetype.clone())
                .filter(|filetype| !filetype.is_empty()),
            _ => file.clone(),
        };
        let matching = self.autocmds.matching(&event, subject.as_deref(), buffer);
        if matching.is_empty() {
            return;
        }
        self.autocmds.set_firing(true);
        let mut remove = Vec::new();
        for autocmd in matching {
            if autocmd.once {
                remove.push(autocmd.id);
            }
            if let Err(error) = self.execute_ex_line(&autocmd.command) {
                let text = format!("Error in {} autocmd: {}", event.name(), error);
                self.set_error(text);
            }
        }
        self.autocmds.set_firing(false);
        if !remove.is_empty() {
            self.autocmds.remove_ids(&remove);
        }
    }

    pub fn current_buffer_id(&self) -> u64 {
        rvim_types::Id::raw(self.current_buffer)
    }

    pub fn current_window_id(&self) -> u64 {
        self.view
            .current_window()
            .map(|window| rvim_types::Id::raw(window.id))
            .unwrap_or(0)
    }

    pub(crate) fn sync_window_options_public(&mut self) {
        self.sync_window_options();
    }

    pub(crate) fn update_window_buffer_public(&mut self) {
        self.update_window_buffer();
    }

    pub(crate) fn delete_lines(
        &mut self,
        range: rvim_ex::ResolvedRange,
        register: RegisterName,
    ) -> Result<(), VimError> {
        if range.start == 0 || range.end > self.line_count() {
            return Err(VimError::new(16, "Invalid range"));
        }
        self.record_undo();
        let lines = self.delete_linewise(range.start, range.end);
        let content = RegisterContent::new(lines, RegisterKind::Linewise);
        self.registers.delete(register, content);
        self.commit_undo();
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn yank_lines(&mut self, range: rvim_ex::ResolvedRange) -> Result<(), VimError> {
        if range.start == 0 || range.end > self.line_count() {
            return Err(VimError::new(16, "Invalid range"));
        }
        let lines: Vec<VimString> = (range.start..=range.end)
            .filter_map(|lnum| {
                self.current_buffer()
                    .and_then(|b| b.text.line_vim(lnum))
                    .cloned()
            })
            .collect();
        let count = lines.len();
        let register = self
            .pending_register
            .take()
            .and_then(RegisterName::parse)
            .unwrap_or(RegisterName::Unnamed);
        self.registers.yank(
            register,
            RegisterContent::new(lines, RegisterKind::Linewise),
        );
        self.messages.info(format!("{count} line(s) yanked"));
        Ok(())
    }

    pub(crate) fn put_register(
        &mut self,
        register: RegisterName,
        before: bool,
    ) -> Result<(), VimError> {
        self.put_from(Some(register), before);
        Ok(())
    }

    pub(crate) fn join_range(&mut self, range: rvim_ex::ResolvedRange, insert_space: bool) {
        if range.start >= range.end {
            return;
        }
        self.set_cursor(BufPos::new(range.start, 0));
        let count = range.end - range.start;
        for _ in 0..count {
            self.join_lines(2, insert_space);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn read_after_current(&mut self, bytes: &[u8], path: &Path) -> Result<(), VimError> {
        let after = self.cursor().lnum;
        self.read_after_line(after, bytes, path)
    }

    pub(crate) fn read_after_line(
        &mut self,
        after: u32,
        bytes: &[u8],
        path: &Path,
    ) -> Result<(), VimError> {
        let lines = rvim_text::from_bytes(bytes, rvim_text::FileFormat::Unix);
        if let Some(buffer) = self.current_buffer_mut() {
            for (offset, line) in lines.iter().enumerate() {
                buffer
                    .text
                    .insert_line(after + 1 + offset as u32, line.clone());
            }
            buffer.text.touch();
        }
        self.messages
            .info(format!("{}L read from {}", lines.len(), path.display()));
        self.request_redraw();
        Ok(())
    }

    pub(crate) fn run_normal_keys(&mut self, keys: &str, use_mappings: bool) {
        for key in crate::keymap::parse_keys(keys) {
            if use_mappings {
                self.handle_key(key);
            } else {
                self.dispatch_key(key);
            }
        }
        if use_mappings {
            self.flush_mappings();
        }
    }

    pub(crate) fn registers_summary(&self) -> String {
        let mut out = String::new();
        for (name, content) in self.registers.entries() {
            let kind = match content.kind {
                RegisterKind::Linewise => "l",
                RegisterKind::Blockwise => "b",
                RegisterKind::Characterwise => "c",
            };
            out.push_str(&format!(
                "\"{name}  {kind}  {}\n",
                content.text().to_string_lossy().replace('\n', "^J")
            ));
        }
        out.trim_end().to_string()
    }

    pub fn open_file(&mut self, path: &Path) -> Result<(), VimError> {
        self.open_path(path)
    }

    /// Encodes UTF-8 buffer bytes for writing using 'fileencoding', falling
    /// back to iconv(1) for encodings without a built-in converter.
    fn encode_with(
        &self,
        bytes: &[u8],
        fileencoding: &str,
        bomb: bool,
    ) -> Result<Vec<u8>, VimError> {
        if fileencoding.is_empty() {
            return Ok(if bomb {
                rvim_text::encode(bytes, rvim_text::Encoding::Utf8, true)
            } else {
                bytes.to_vec()
            });
        }
        if let Some(encoding) = rvim_text::Encoding::parse(fileencoding) {
            return Ok(rvim_text::encode(bytes, encoding, bomb));
        }
        let converted = rvim_platform::iconv(bytes, "utf-8", fileencoding).ok_or_else(|| {
            VimError::plain(format!(
                "E513: write error, conversion failed (encoding {fileencoding})"
            ))
        })?;
        Ok(converted)
    }

    pub(crate) fn open_path(&mut self, path: &Path) -> Result<(), VimError> {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            pfs::cwd().join(path)
        };
        let (bytes, missing) = match pfs::read_file(&path) {
            Ok(bytes) => (bytes, false),
            Err(error) if error.is_not_found() => (Vec::new(), true),
            Err(error) => return Err(VimError::new(13, error.to_string())),
        };
        let fileencodings = self.options.get_string("fileencodings");
        let (decoded, detected_encoding) = detect_file_encoding(&bytes, &fileencodings);
        self.swaps.remove(self.current_buffer);
        let buffer_id_for_modeline = rvim_types::Id::raw(self.current_buffer);
        let modeline_enabled = self.options.buffer_bool(buffer_id_for_modeline, "modeline");
        let modeline_count = self
            .options
            .buffer_number(buffer_id_for_modeline, "modelines");
        let Some(buffer) = self.current_buffer_mut() else {
            return Err(VimError::plain("E158: Invalid buffer"));
        };
        let detect_path = path.clone();
        buffer.set_name(path);
        buffer.load_bytes(&decoded.text);
        buffer.set_modified(false);
        buffer.text.meta.fileencoding = detected_encoding.clone();
        buffer.text.meta.bomb = decoded.bomb;
        buffer.text.meta.modeline = modeline_enabled;
        buffer.text.meta.modelines = modeline_count;
        let first_line = decoded
            .text
            .split(|byte| *byte == b'\n')
            .next()
            .unwrap_or(&[]);
        let filetype = rvim_runtime::detect_filetype(&detect_path, first_line).unwrap_or("");
        buffer.filetype = filetype.to_string();
        let fileformat = buffer.text.meta.fileformat.name().to_string();
        let endofline = buffer.text.meta.endofline;
        let short_name = buffer.short_name();
        let line_count = buffer.text.line_count();
        let modelines = crate::modeline::modeline_options(&buffer.text);
        let buffer_id = rvim_types::Id::raw(self.current_buffer);
        let _ = self.options.set_buffer(
            buffer_id,
            "fileformat",
            crate::options::OptionValue::String(VimString::from(fileformat)),
        );
        let _ = self.options.set_buffer(
            buffer_id,
            "endofline",
            crate::options::OptionValue::Bool(endofline),
        );
        let _ = self.options.set_buffer(
            buffer_id,
            "bomb",
            crate::options::OptionValue::Bool(decoded.bomb),
        );
        let _ = self.options.set_buffer(
            buffer_id,
            "fileencoding",
            crate::options::OptionValue::String(VimString::from(detected_encoding.as_str())),
        );
        let _ = self.options.set_buffer(
            buffer_id,
            "filetype",
            crate::options::OptionValue::String(VimString::from(filetype)),
        );
        let window_id = self.current_window_id();
        for options in modelines {
            let _ = crate::options::execute_set(
                &mut self.options,
                &options,
                true,
                buffer_id,
                window_id,
                false,
            );
        }
        // 'foldlevelstart' applies when a buffer is loaded.
        let foldlevelstart = self.options.get_number("foldlevelstart");
        if foldlevelstart >= 0 {
            let _ = self.options.set_window(
                window_id,
                "foldlevel",
                crate::options::OptionValue::Number(foldlevelstart),
            );
        }
        self.sync_window_options();
        self.set_cursor(BufPos::new(1, 0));
        self.update_window_buffer();
        let undo_buffer = self.current_buffer_id();
        self.load_undo_file(BufferId::from_raw(undo_buffer));
        self.record_jump();
        self.messages
            .info(format!("\"{}\" {}L", short_name, line_count));
        self.fire_event(if missing {
            Event::BufNewFile
        } else {
            Event::BufReadPost
        });
        if !filetype.is_empty() {
            self.syntax.set_filetype(filetype);
            self.fire_event(Event::FileType);
            self.fire_event(Event::Syntax);
            self.load_runtime_syntax();
            self.load_filetype_plugins();
        }
        self.fire_event(Event::BufEnter);
        if self.swap_enabled {
            self.create_swap_for_current();
        }
        Ok(())
    }

    pub(crate) fn write_range(
        &mut self,
        start: u32,
        end: u32,
        path: Option<&str>,
        append: bool,
    ) -> Result<(), VimError> {
        let force_write = self.force_write;
        self.force_write = false;
        let path = match path {
            Some(path) => pfs::expand_tilde(path),
            None => match self.current_buffer().and_then(|b| b.text.meta.name.clone()) {
                Some(path) => path,
                None => return Err(VimError::new(32, "No file name")),
            },
        };
        if path.exists()
            && !self.options.get_bool("writeany")
            && !force_write
            && std::fs::metadata(&path).is_ok_and(|metadata| metadata.permissions().readonly())
        {
            return Err(VimError::new(
                45,
                format!("'readonly' option is set for \"{}\"", path.display()),
            ));
        }
        self.fire_event(Event::BufWritePre);
        let buffer_id = self.current_buffer_id();
        let fileencoding = self.options.buffer_string(buffer_id, "fileencoding");
        let bomb = self.options.buffer_bool(buffer_id, "bomb");
        let bytes = self
            .current_buffer()
            .map(|buffer| buffer.text.to_file_bytes_range(start, end))
            .unwrap_or_default();
        let bytes = self.encode_with(&bytes, &fileencoding, bomb)?;
        if append {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            file.write_all(&bytes)
                .map_err(|error| VimError::new(13, error.to_string()))?;
        } else {
            pfs::write_file_atomic(&path, &bytes)
                .map_err(|error| VimError::new(13, error.to_string()))?;
        }
        self.save_undo_file(BufferId::from_raw(buffer_id));
        self.messages.info(format!(
            "\"{}\" {}L, {}B written",
            path.display(),
            end.saturating_sub(start).saturating_add(1),
            bytes.len()
        ));
        self.fire_event(Event::BufWritePost);
        Ok(())
    }

    pub(crate) fn write_buffer(
        &mut self,
        path: Option<&str>,
        append: bool,
    ) -> Result<(), VimError> {
        let force_write = self.force_write;
        self.force_write = false;
        if let Some(buffer) = self.current_buffer() {
            if !buffer.buftype.is_empty() && buffer.buftype != "acwrite" {
                return Err(VimError::new(382, "Cannot write to a special buffer"));
            }
        }
        let path = match path {
            Some(path) => pfs::expand_tilde(path),
            None => match self.current_buffer().and_then(|b| b.text.meta.name.clone()) {
                Some(path) => path,
                None => return Err(VimError::new(32, "No file name")),
            },
        };
        if path.exists()
            && !self.options.get_bool("writeany")
            && !force_write
            && std::fs::metadata(&path).is_ok_and(|metadata| metadata.permissions().readonly())
        {
            return Err(VimError::new(
                45,
                format!("'readonly' option is set for \"{}\"", path.display()),
            ));
        }
        self.fire_event(Event::BufWritePre);
        let backup = self.options.get_bool("backup");
        let writebackup = self.options.get_bool("writebackup");
        let backup_extension = self.options.get_string("backupext");
        let backup_extension = if backup_extension.is_empty() {
            "~".to_string()
        } else {
            backup_extension
        };
        let backup_path = if path.exists() && (backup || writebackup) {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "rvim".to_string());
            Some(path.with_file_name(format!("{name}{backup_extension}")))
        } else {
            None
        };
        if let Some(backup_path) = &backup_path {
            std::fs::copy(&path, backup_path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
        }
        let buffer_id = self.current_buffer_id();
        let fileformat = self.options.buffer_string(buffer_id, "fileformat");
        let fileencoding = self.options.buffer_string(buffer_id, "fileencoding");
        let endofline = self.options.buffer_bool(buffer_id, "endofline");
        let bomb = self.options.buffer_bool(buffer_id, "bomb");
        if let Some(buffer) = self.current_buffer_mut() {
            if let Some(format) = rvim_text::FileFormat::from_name(&fileformat) {
                buffer.text.meta.fileformat = format;
            }
            buffer.text.meta.endofline = endofline;
            buffer.text.meta.bomb = bomb;
            buffer.text.meta.fileencoding = fileencoding.clone();
        }
        let mut bytes = self
            .current_buffer()
            .map(|b| b.text.to_file_bytes())
            .unwrap_or_default();
        if bomb && bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            bytes.drain(..3);
        }
        let bytes = self.encode_with(&bytes, &fileencoding, bomb)?;
        if append {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| VimError::new(13, e.to_string()))?;
            file.write_all(&bytes)
                .map_err(|e| VimError::new(13, e.to_string()))?;
        } else {
            pfs::write_file_atomic(&path, &bytes).map_err(|e| VimError::new(13, e.to_string()))?;
        }
        let written_buffer = self.current_buffer_id();
        self.save_undo_file(BufferId::from_raw(written_buffer));
        let bytes_written = bytes.len();
        if let Some(backup_path) = &backup_path {
            if writebackup && !backup {
                let _ = std::fs::remove_file(backup_path);
            }
        }
        if let Some(buffer) = self.current_buffer_mut() {
            buffer.set_name(path.clone());
            buffer.set_modified(false);
        }
        self.messages.info(format!(
            "\"{}\" {}L, {}B written",
            path.display(),
            self.line_count(),
            bytes_written
        ));
        self.fire_event(Event::BufWritePost);
        Ok(())
    }

    pub(crate) fn quit_window(&mut self, force: bool) {
        if !force {
            if let Some(buffer) = self.current_buffer() {
                if buffer.modified() {
                    self.set_error("E37: No write since last change (add ! to override)");
                    return;
                }
            }
        }
        if self.view.window_ids_in_current_tab().len() > 1 {
            if let Err(error) = self.close_window_or_error() {
                self.set_error(error);
            }
            return;
        }
        if self.view.tab_count() > 1 {
            if let Err(error) = self.tab_close(force) {
                self.set_error(error);
            }
            return;
        }
        self.should_quit = true;
    }

    pub(crate) fn quit(&mut self, force: bool) {
        if !force {
            if let Some(buffer) = self.current_buffer() {
                if buffer.modified() {
                    self.set_error("E37: No write since last change (add ! to override)");
                    return;
                }
            }
        }
        self.should_quit = true;
    }

    fn write_quit(&mut self) {
        if self.write_buffer(None, false).is_ok() {
            self.quit_window(false);
        }
    }

    fn goto_first_line(&mut self) {
        self.goto_line(1);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertKind {
    Before,
    After,
    FirstNonBlank,
    EndOfLine,
    OpenBelow,
    OpenAbove,
    Replace,
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn tag_search_pattern(command: &str, name: &str) -> String {
    let trimmed = command.trim();
    if let Some(rest) = trimmed
        .strip_prefix('/')
        .or_else(|| trimmed.strip_prefix('?'))
    {
        let pattern = rest.trim_end_matches('/').trim_end_matches('?');
        if !pattern.is_empty() {
            return pattern.to_string();
        }
    }
    format!("*{name}*")
}

fn complete_path(prefix: &str) -> Vec<String> {
    let path = std::path::Path::new(prefix);
    let (directory, stem) = if prefix.ends_with('/') {
        (path, "")
    } else {
        match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => (
                parent,
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(""),
            ),
            _ => (std::path::Path::new("."), prefix),
        }
    };
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(stem) {
            let mut candidate = if prefix.ends_with('/') {
                format!("{prefix}{name}")
            } else if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                format!("{}/{name}", parent.display())
            } else {
                name.clone()
            };
            if entry.path().is_dir() {
                candidate.push('/');
            }
            out.push(candidate);
        }
    }
    out
}

fn common_prefix(values: &[String]) -> String {
    let Some(first) = values.first() else {
        return String::new();
    };
    let mut end = first.len();
    for value in &values[1..] {
        while end > 0 && !value.starts_with(&first[..end]) {
            end -= 1;
            while end > 0 && !first.is_char_boundary(end) {
                end -= 1;
            }
        }
    }
    first[..end].to_string()
}

fn digraph(first: char, second: char) -> Option<char> {
    Some(match (first, second) {
        ('a', ':') => 'ä',
        ('A', ':') => 'Ä',
        ('o', ':') => 'ö',
        ('O', ':') => 'Ö',
        ('u', ':') => 'ü',
        ('U', ':') => 'Ü',
        ('s', 's') => 'ß',
        ('e', ':') => 'ë',
        ('E', ':') => 'Ë',
        ('i', ':') => 'ï',
        ('I', ':') => 'Ï',
        ('n', '?') => 'ñ',
        ('N', '?') => 'Ñ',
        ('c', ',') => 'ç',
        ('C', ',') => 'Ç',
        ('a', 'e') => 'æ',
        ('A', 'E') => 'Æ',
        ('o', 'e') => 'œ',
        ('O', 'E') => 'Œ',
        ('a', 'a') => 'å',
        ('A', 'A') => 'Å',
        ('o', '/') => 'ø',
        ('O', '/') => 'Ø',
        ('<', '<') => '«',
        ('>', '>') => '»',
        ('C', 'o') => '©',
        ('R', 'g') => '®',
        ('E', 'u') => '€',
        ('+', '-') => '±',
        ('1', '2') => '½',
        ('1', '4') => '¼',
        ('3', '4') => '¾',
        ('!', 'I') => '¡',
        ('?', 'I') => '¿',
        _ => return None,
    })
}

fn next_char_boundary(line: &[u8], col: usize) -> usize {
    if col >= line.len() {
        return line.len();
    }
    let mut i = col + 1;
    while i < line.len() && (line[i] & 0xC0) == 0x80 {
        i += 1;
    }
    i
}

fn collect_vim_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_vim_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "vim") {
            out.push(path);
        }
    }
}

fn close_tabline_click(
    pending: &mut Option<(u16, String, u32)>,
    col: u16,
    clicks: &mut Vec<TablineClick>,
) {
    if let Some((start, function, minwid)) = pending.take() {
        if col > start {
            clicks.push(TablineClick {
                start,
                end: col,
                function,
                minwid,
            });
        }
    }
}

fn cino_amount(options: &str, key: char, shiftwidth: usize) -> Option<i32> {
    let bytes = options.as_bytes();
    let index = bytes.iter().position(|byte| *byte as char == key)?;
    let mut value = String::new();
    let mut position = index + 1;
    while position < bytes.len() {
        let ch = bytes[position] as char;
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' {
            value.push(ch);
            position += 1;
        } else if ch == 's' {
            position += 1;
            break;
        } else {
            break;
        }
    }
    if value.is_empty() {
        return None;
    }
    let mut amount = value.parse::<f64>().ok()?;
    let scaled = position > index + 1 && bytes.get(position - 1) == Some(&b's');
    if scaled {
        amount *= shiftwidth as f64;
    }
    Some(amount.round() as i32)
}

static SHELL_TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn apply_scriptencoding(source: &[u8]) -> Vec<u8> {
    let mut converted = source.to_vec();
    let mut found = false;
    for line in source.split_inclusive(|byte| *byte == b'\n').take(64) {
        let trimmed = line.trim_ascii_start();
        let Some(rest) = trimmed.strip_prefix(b"scriptencoding") else {
            continue;
        };
        if !rest.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
            continue;
        }
        let name = String::from_utf8_lossy(rest).trim().to_string();
        if name.is_empty() {
            break;
        }
        if let Some(encoding) = rvim_text::Encoding::parse(&name) {
            if !matches!(encoding, rvim_text::Encoding::Utf8) {
                converted = rvim_text::decode_with(source, encoding);
            }
            found = true;
        } else if let Some(bytes) = rvim_platform::iconv(source, &name, "utf-8") {
            converted = bytes;
            found = true;
        }
        break;
    }
    if !found {
        return source.to_vec();
    }
    // The VimL parser only accepts UTF-8 scripts, so mark the converted
    // script as UTF-8 after the editor converted it.
    let mut out = Vec::with_capacity(converted.len());
    let mut rewritten = false;
    for line in converted.split_inclusive(|byte| *byte == b'\n') {
        let trimmed = line.trim_ascii_start();
        if !rewritten
            && trimmed
                .strip_prefix(b"scriptencoding")
                .is_some_and(|rest| rest.first().is_some_and(|byte| byte.is_ascii_whitespace()))
        {
            out.extend_from_slice(b"scriptencoding utf-8\n");
            rewritten = true;
        } else {
            out.extend_from_slice(line);
        }
    }
    out
}

fn detect_file_encoding(bytes: &[u8], fileencodings: &str) -> (rvim_text::Decoded, String) {
    for name in fileencodings
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let lower = name.to_ascii_lowercase().replace('_', "-");
        match lower.as_str() {
            "ucs-bom" => {
                if bytes.starts_with(&[0xEF, 0xBB, 0xBF])
                    || bytes.starts_with(&[0xFF, 0xFE])
                    || bytes.starts_with(&[0xFE, 0xFF])
                {
                    let decoded = rvim_text::decode(bytes);
                    let name = decoded.encoding.name().to_string();
                    return (decoded, name);
                }
            }
            "utf-8" | "utf8" | "default" => {
                if std::str::from_utf8(bytes).is_ok() {
                    return (rvim_text::decode(bytes), "utf-8".to_string());
                }
            }
            other => {
                if let Some(encoding) = rvim_text::Encoding::parse(other) {
                    if !matches!(encoding, rvim_text::Encoding::Utf8) {
                        let text = rvim_text::decode_with(bytes, encoding);
                        return (
                            rvim_text::Decoded {
                                text,
                                encoding,
                                bomb: false,
                            },
                            encoding.name().to_string(),
                        );
                    }
                } else if let Some(converted) = rvim_platform::iconv(bytes, other, "utf-8") {
                    let decoded = rvim_text::decode(&converted);
                    return (decoded, other.to_string());
                }
            }
        }
    }
    let decoded = rvim_text::decode(bytes);
    let name = decoded.encoding.name().to_string();
    (decoded, name)
}

fn write_private(path: &std::path::Path, bytes: &[u8]) -> Result<(), rvim_platform::ProcessError> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options
        .open(path)
        .map_err(|error| rvim_platform::ProcessError(error.to_string()))?;
    file.write_all(bytes)
        .map_err(|error| rvim_platform::ProcessError(error.to_string()))
}

fn quote_shell_command(command: &str, shellquote: &str, xquote: &str, xescape: &str) -> String {
    // Vim's order: 'shellquote' wraps the command first (redirection stays
    // outside), then 'shellxquote' wraps the whole thing including
    // redirection.  Only "(" uses 'shellxescape' (with cmd.exe style '^'
    // escaping).
    let mut quoted = command.to_string();
    if !shellquote.is_empty() {
        quoted = format!("{shellquote}{quoted}{shellquote}");
    }
    if !xquote.is_empty() {
        let escaped = if !xescape.is_empty() && xquote == "(" {
            let mut escaped = String::new();
            for ch in quoted.chars() {
                if xescape.contains(ch) {
                    escaped.push('^');
                }
                escaped.push(ch);
            }
            escaped
        } else {
            quoted
        };
        let closing = if xquote == "(" {
            ")"
        } else if xquote == "\"(" {
            ")\""
        } else {
            xquote
        };
        quoted = format!("{xquote}{escaped}{closing}");
    }
    quoted
}

#[cfg(test)]
mod shell_quote_tests {
    use super::quote_shell_command;

    #[test]
    fn shell_quote_matrix() {
        assert_eq!(quote_shell_command("echo hi", "", "", ""), "echo hi");
        assert_eq!(quote_shell_command("echo hi", "", "\"", ""), "\"echo hi\"");
        assert_eq!(quote_shell_command("echo hi", "'", "", ""), "'echo hi'");
        assert_eq!(quote_shell_command("echo hi", "", "'", ""), "'echo hi'");
        // shellquote wraps the command, then shellxquote wraps the rest.
        assert_eq!(
            quote_shell_command("echo hi", "'", "\"", ""),
            "\"'echo hi'\""
        );
        // Only shellxquote == "(" escapes characters, with cmd.exe style '^'.
        assert_eq!(
            quote_shell_command("echo & hi", "", "(", "&"),
            "(echo ^& hi)"
        );
        assert_eq!(
            quote_shell_command("echo & hi", "", "\"", "&"),
            "\"echo & hi\""
        );
        assert_eq!(
            quote_shell_command("echo hi", "", "\"(", ""),
            "\"(echo hi)\""
        );
    }
}
