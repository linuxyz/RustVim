import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub last_substitute_flags: String,
    last_change_keys:""", """    pub last_substitute_flags: String,
    pub(crate) noautocmd_depth: u32,
    pub(crate) keepjumps_depth: u32,
    pub(crate) vertical_requested: bool,
    last_change_keys:""", 1)
s = s.replace("""            last_substitute_flags: String::new(),
            last_change_keys:""", """            last_substitute_flags: String::new(),
            noautocmd_depth: 0,
            keepjumps_depth: 0,
            vertical_requested: false,
            last_change_keys:""", 1)
# MessageState silent
s = s.replace("""    pub echo_window: bool,
}

impl MessageState {""", """    pub echo_window: bool,
    pub silent: bool,
}

impl MessageState {""", 1)
s = s.replace("""        self.error = false;
        self.kind = MessageKind::Info;
        self.echo_window = false;
        self.history.push_back(message);""", """        self.error = false;
        self.kind = MessageKind::Info;
        self.echo_window = false;
        if self.silent {
            self.history.push_back(message);
            return;
        }
        self.history.push_back(message);""", 1)
# warning
s = s.replace("""    pub fn warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.last = Some(message.clone());""", """    pub fn warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        if self.silent {
            self.history.push_back(message);
            return;
        }
        self.last = Some(message.clone());""", 1)
# error
s = s.replace("""    pub fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.last = Some(message.clone());""", """    pub fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        if self.silent {
            self.errmsg = message.clone();
            self.history.push_back(message);
            return;
        }
        self.last = Some(message.clone());""", 1)
# fire_event early
s = s.replace("""    pub fn fire_event(&mut self, event: Event) {
        if self.autocmds.is_firing() {
            return;
        }""", """    pub fn fire_event(&mut self, event: Event) {
        if self.noautocmd_depth > 0 || self.autocmds.is_firing() {
            return;
        }""", 1)
# record_jump early
s = s.replace("""    fn record_jump(&mut self) {""", """    fn record_jump(&mut self) {
        if self.keepjumps_depth > 0 {
            return;
        }""", 1)
p.write_text(s)
print("editor patched")
# ex wrapper
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
s = s.replace("use rvim_ex::{AddressKind, ExCommand, RangeSpec, ResolvedRange};", "use rvim_ex::{AddressKind, ExCommand, Modifier, RangeSpec, ResolvedRange};", 1)
s = s.replace("""pub fn execute(editor: &mut Editor, line: &str) -> Result<(), VimError> {
    let command = rvim_ex::parse(line).map_err(|error| VimError::plain(error.to_string()))?;
    execute_command(editor, command)
}

fn execute_command(editor: &mut Editor, command: ExCommand) -> Result<(), VimError> {""", """pub fn execute(editor: &mut Editor, line: &str) -> Result<(), VimError> {
    let command = rvim_ex::parse(line).map_err(|error| VimError::plain(error.to_string()))?;
    execute_command(editor, command)
}

fn execute_command(editor: &mut Editor, mut command: ExCommand) -> Result<(), VimError> {
    let modifiers = std::mem::take(&mut command.modifiers);
    let mut silent = false;
    let mut silent_bang = false;
    let mut noautocmd = false;
    let mut keepjumps = false;
    let mut tab = false;
    let mut vertical = false;
    for modifier in &modifiers {
        match modifier {
            Modifier::Silent => silent = true,
            Modifier::SilentBang => {
                silent = true;
                silent_bang = true;
            }
            Modifier::Unsilent => silent = false,
            Modifier::NoAutocmd => noautocmd = true,
            Modifier::KeepJumps => keepjumps = true,
            Modifier::Tab => tab = true,
            Modifier::Vertical => vertical = true,
            _ => {}
        }
    }
    if tab {
        editor.tab_new(None)?;
    }
    let previous_silent = editor.messages.silent;
    if silent {
        editor.messages.silent = true;
    }
    if noautocmd {
        editor.noautocmd_depth += 1;
    }
    if keepjumps {
        editor.keepjumps_depth += 1;
    }
    let previous_vertical = editor.vertical_requested;
    editor.vertical_requested = vertical;
    let result = execute_command_inner(editor, command);
    editor.vertical_requested = previous_vertical;
    if keepjumps {
        editor.keepjumps_depth = editor.keepjumps_depth.saturating_sub(1);
    }
    if noautocmd {
        editor.noautocmd_depth = editor.noautocmd_depth.saturating_sub(1);
    }
    editor.messages.silent = previous_silent;
    if silent_bang && result.is_err() {
        editor.messages.error = false;
        editor.messages.last = None;
        return Ok(());
    }
    result
}

fn execute_command_inner(editor: &mut Editor, command: ExCommand) -> Result<(), VimError> {""", 1)
# split dirs
s = s.replace("""            editor.split_window(rvim_view::SplitDir::Horizontal, path.as_deref())?;""", """            let direction = if editor.vertical_requested {
                rvim_view::SplitDir::Vertical
            } else {
                rvim_view::SplitDir::Horizontal
            };
            editor.split_window(direction, path.as_deref())?;""", 1)
s = s.replace("""        "new" => {
            editor.split_window(rvim_view::SplitDir::Horizontal, None)?;""", """        "new" => {
            let direction = if editor.vertical_requested {
                rvim_view::SplitDir::Vertical
            } else {
                rvim_view::SplitDir::Horizontal
            };
            editor.split_window(direction, None)?;""", 1)
p.write_text(s)
print("ex patched")
