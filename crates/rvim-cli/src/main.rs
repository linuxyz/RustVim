use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use rvim_editor::Editor;
use rvim_headless::HeadlessBackend;
use rvim_tui::TuiBackend;
use rvim_ui_api::{Key, KeyEvent, Mods, Size, UiBackend, UiEvent, UiMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WindowLayout {
    #[default]
    Default,
    Tabs,
    Horizontal,
    Vertical,
}

#[derive(Debug, Default)]
struct Args {
    headless: bool,
    dump_frame: bool,
    version: bool,
    help: bool,
    clean: bool,
    readonly: bool,
    files: Vec<PathBuf>,
    pre_commands: Vec<String>,
    commands: Vec<String>,
    keys: String,
    startuptime: Option<PathBuf>,
    no_swap: bool,
    recover: bool,
    vimrc: Option<String>,
    session: Option<PathBuf>,
    layout: WindowLayout,
    norc: bool,
    defaults_only: bool,
    log: Option<PathBuf>,
    log_categories: String,
    verbose: i64,
    verbose_file: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rvim: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    rvim_editor::trace::install_crash_hook(args.log.clone());
    if args.help {
        print_help();
        return Ok(());
    }
    if args.version {
        let features = [
            "eval",
            "syntax",
            "quickfix",
            "linebreak",
            "statusline_click",
            "compl_func",
            "digraphs",
            "clipboard",
            "mouse",
            "timers",
            "job",
            "pty",
        ]
        .into_iter()
        .filter(|feature| rvim_types::has_feature(feature))
        .collect::<Vec<_>>()
        .join(" ");
        println!(
            "RVim {} (Vim {} target)\nFeatures: {}",
            rvim_types::RVIM_VERSION,
            rvim_types::VIM_VERSION,
            features
        );
        return Ok(());
    }
    if args.headless || args.dump_frame {
        return run_headless(args);
    }
    run_tui(args)
}

fn run_headless(args: Args) -> Result<(), String> {
    let started = Instant::now();
    let size = Size::new(24, 80);
    let mut backend = HeadlessBackend::new(size);
    let mut editor = make_editor(&args, size);
    backend.init().map_err(|e| e.to_string())?;

    open_files(&mut editor, &args)?;
    if args.recover {
        let path = args.files.first().cloned();
        if let Err(error) = editor.recover_current(path.as_deref()) {
            editor.messages.error(error.to_string());
        }
    }
    editor.set_arglist(args.files.clone());
    if let Some(session) = &args.session {
        source_ex_file(&mut editor, session);
    }
    apply_readonly(&mut editor, &args);
    for command in &args.commands {
        editor.execute_ex(command);
    }
    for event in parse_keys(&args.keys) {
        editor.handle_event(UiEvent::Key(event));
    }
    editor.flush_mappings();
    editor.fire_event(rvim_editor::autocmd::Event::VimEnter);
    let frame = editor.render();
    backend.present(&frame).map_err(|e| e.to_string())?;
    if args.dump_frame {
        print!("{}", backend.snapshot());
        if !args.keys.is_empty() {
            eprintln!("-- cursor --");
            eprint!("{}", backend.snapshot_with_cursor());
        }
    } else {
        println!("{}", backend.snapshot());
        if let Some(message) = editor.messages.last.as_deref() {
            eprintln!("{message}");
        }
    }
    if let Some(path) = args.startuptime {
        let line = format!(
            "rvim startup: {:.3}\n",
            started.elapsed().as_secs_f64() * 1000.0
        );
        std::fs::write(path, line).map_err(|e| e.to_string())?;
    }
    backend.shutdown().map_err(|e| e.to_string())?;
    editor.shutdown_swaps();
    Ok(())
}

fn run_tui(args: Args) -> Result<(), String> {
    let started = Instant::now();
    let mut backend = TuiBackend::new();
    backend.init().map_err(|e| e.to_string())?;
    let size = rvim_platform::term::terminal_size();
    let size = Size::new(size.rows, size.cols);
    let mut editor = make_editor(&args, size);
    open_files(&mut editor, &args)?;
    editor.read_default_viminfo();
    if args.recover {
        let path = args.files.first().cloned();
        if let Err(error) = editor.recover_current(path.as_deref()) {
            editor.messages.error(error.to_string());
        }
    }
    editor.set_arglist(args.files.clone());
    if let Some(session) = &args.session {
        source_ex_file(&mut editor, session);
    }
    apply_readonly(&mut editor, &args);
    for command in &args.commands {
        editor.execute_ex(command);
    }
    for event in parse_keys(&args.keys) {
        editor.handle_event(UiEvent::Key(event));
    }
    editor.flush_mappings();
    editor.fire_event(rvim_editor::autocmd::Event::VimEnter);
    editor.resize(size);
    editor.should_redraw = true;
    let mut last_size = size;
    while !editor.should_quit {
        if editor.suspend_requested {
            editor.suspend_requested = false;
            backend.shutdown().map_err(|error| error.to_string())?;
            rvim_platform::term::suspend_process();
            backend.init().map_err(|error| error.to_string())?;
            let platform_size = rvim_platform::term::terminal_size();
            let size = Size::new(platform_size.rows, platform_size.cols);
            last_size = size;
            editor.resize(size);
            editor.should_redraw = true;
        }
        if editor.should_redraw {
            let frame = editor.render();
            backend.present(&frame).map_err(|e| e.to_string())?;
            editor.should_redraw = false;
        }
        editor.on_tick(Instant::now());
        let timeout = editor
            .next_deadline()
            .map(|deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(50))
            })
            .unwrap_or_else(|| Duration::from_millis(50));
        match backend.poll_event(timeout) {
            Ok(Some(UiEvent::Resize(size))) => {
                last_size = size;
                backend.resize(size).map_err(|e| e.to_string())?;
                editor.handle_event(UiEvent::Resize(size));
            }
            Ok(Some(event)) => editor.handle_event(event),
            Ok(None) => {
                let size = rvim_platform::term::terminal_size();
                let size = Size::new(size.rows, size.cols);
                if size != last_size {
                    last_size = size;
                    editor.handle_event(UiEvent::Resize(size));
                }
            }
            Err(error) => {
                let _ = backend.shutdown();
                return Err(error.to_string());
            }
        }
    }
    editor.fire_event(rvim_editor::autocmd::Event::VimLeavePre);
    editor.write_default_viminfo();
    backend.shutdown().map_err(|e| e.to_string())?;
    editor.shutdown_swaps();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let _ = last_size;
    if let Some(path) = args.startuptime {
        let line = format!(
            "rvim startup: {:.3}
",
            started.elapsed().as_secs_f64() * 1000.0
        );
        std::fs::write(path, line).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn make_editor(args: &Args, size: Size) -> Editor {
    let mut editor = Editor::new(size);
    if let Some(path) = &args.log {
        let categories = if args.log_categories.is_empty() {
            "all"
        } else {
            &args.log_categories
        };
        if let Err(error) = editor.set_trace(path, categories) {
            editor.messages.error(error);
        }
    }
    if args.verbose > 0 {
        let _ = editor
            .options
            .set_global("verbose", rvim_editor::OptionValue::Number(args.verbose));
    }
    if args.log.is_none() {
        if let Some(path) = &args.verbose_file {
            let _ = editor.set_trace(path, "all");
        }
    }
    if args.no_swap {
        editor.swap_enabled = false;
        let _ = editor
            .options
            .set_global("swapfile", rvim_editor::OptionValue::Bool(false));
    }
    // --cmd runs before any startup file.
    for command in &args.pre_commands {
        editor.execute_ex(command);
    }
    if !args.clean && !args.norc {
        if let Some(vimrc) = &args.vimrc {
            source_ex_file(&mut editor, std::path::Path::new(vimrc));
        } else if args.defaults_only {
            if let Some(defaults) = editor.runtime.find("defaults.vim") {
                source_ex_file(&mut editor, &defaults);
            }
        } else if let Some(vimrc) = user_vimrc_path() {
            source_ex_file(&mut editor, &vimrc);
        } else if let Some(defaults) = editor.runtime.find("defaults.vim") {
            source_ex_file(&mut editor, &defaults);
        }
    }
    // Vim reads .vimrc/.exrc from the current directory when 'exrc' is set,
    // after the startup vimrc and with secure restrictions when requested.
    let exrc_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if let Err(error) = editor.source_local_exrc(&exrc_dir) {
        editor.messages.error(error.to_string());
    }
    if args.clean || args.norc {
        let _ = editor.options.set_global(
            "viminfo",
            rvim_editor::OptionValue::String(rvim_types::VimString::new()),
        );
    }
    editor
}

fn open_files(editor: &mut Editor, args: &Args) -> Result<(), String> {
    let Some(first) = args.files.first() else {
        return Ok(());
    };
    editor.open_file(first).map_err(|error| error.to_string())?;
    for file in args.files.iter().skip(1) {
        match args.layout {
            WindowLayout::Default => break,
            WindowLayout::Tabs => editor.execute_ex("tabnew"),
            WindowLayout::Horizontal => editor.execute_ex("split"),
            WindowLayout::Vertical => editor.execute_ex("vsplit"),
        }
        editor.open_file(file).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn user_vimrc_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(home) = &home {
        let vimrc = home.join(".vimrc");
        if vimrc.is_file() {
            return Some(vimrc);
        }
        let vim_vimrc = home.join(".vim/vimrc");
        if vim_vimrc.is_file() {
            return Some(vim_vimrc);
        }
    }
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".config")))?;
    let xdg_vimrc = xdg.join("vim/vimrc");
    xdg_vimrc.is_file().then_some(xdg_vimrc)
}
fn source_ex_file(editor: &mut Editor, path: &std::path::Path) {
    let Ok(bytes) = std::fs::read(path) else {
        editor
            .messages
            .error(format!("E484: Can't open file {}", path.display()));
        return;
    };
    if let Err(error) = editor.source_viml(&bytes, &path.display().to_string()) {
        editor.messages.error(error.to_string());
    }
}

fn apply_readonly(editor: &mut Editor, args: &Args) {
    if args.readonly {
        if let Some(buffer) = editor.current_buffer_mut() {
            buffer.readonly = true;
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut iter = std::env::args().skip(1).peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--headless" => args.headless = true,
            "--dump-frame" => args.dump_frame = true,
            "--version" | "-v" => args.version = true,
            "--help" | "-h" => args.help = true,
            "--clean" => args.clean = true,
            "--noplugin" => {}
            "-n" => args.no_swap = true,
            "--no-swap" => args.no_swap = true,
            "-R" | "--readonly" => args.readonly = true,
            "-c" => {
                let command = iter
                    .next()
                    .ok_or_else(|| "-c requires a command".to_string())?;
                args.commands.push(command);
            }
            "--cmd" => {
                let command = iter
                    .next()
                    .ok_or_else(|| "--cmd requires a command".to_string())?;
                args.pre_commands.push(command);
            }
            "-u" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "-u requires a vimrc path".to_string())?;
                if value.eq_ignore_ascii_case("NONE") {
                    args.clean = true;
                } else if value.eq_ignore_ascii_case("NORC") {
                    args.norc = true;
                } else if value.eq_ignore_ascii_case("DEFAULTS") {
                    args.defaults_only = true;
                } else {
                    args.vimrc = Some(value);
                }
            }
            "-S" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "-S requires a session file".to_string())?;
                args.session = Some(PathBuf::from(value));
            }
            _ if arg.starts_with("-V") => {
                let rest = &arg[2..];
                let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
                args.verbose = if digits.is_empty() {
                    1
                } else {
                    digits.parse().unwrap_or(1)
                };
                let file = rest[digits.len()..].trim_start_matches('=');
                if !file.is_empty() {
                    args.verbose_file = Some(PathBuf::from(file));
                }
            }
            "--log" => {
                let path = iter
                    .next()
                    .ok_or_else(|| "--log requires a file".to_string())?;
                args.log = Some(PathBuf::from(path));
            }
            "--log-categories" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--log-categories requires a list".to_string())?;
                args.log_categories = value;
            }
            "--startuptime" => {
                let path = iter
                    .next()
                    .ok_or_else(|| "--startuptime requires a file".to_string())?;
                args.startuptime = Some(PathBuf::from(path));
            }
            "--stdio-channel" => {
                return Err("--stdio-channel is not implemented yet (P3 channel host)".to_string());
            }
            "-es" | "-e" | "-E" => {
                return Err("Ex mode is out of scope; use --headless instead".to_string());
            }
            "-p" => args.layout = WindowLayout::Tabs,
            "-o" => args.layout = WindowLayout::Horizontal,
            "-O" => args.layout = WindowLayout::Vertical,
            "-r" => args.recover = true,
            "--keys" => {
                args.keys = iter
                    .next()
                    .ok_or_else(|| "--keys requires a value".to_string())?;
            }
            "--" => {
                for rest in iter.by_ref() {
                    args.files.push(PathBuf::from(rest));
                }
            }
            value if value.starts_with('-') && value != "-" => {
                return Err(format!("unknown option: {value}"));
            }
            value => args.files.push(PathBuf::from(value)),
        }
    }
    Ok(args)
}

fn print_help() {
    println!(
        "RVim {} - a Rust Vim\n\nUsage: rvim [options] [file ...]\n\n\
Options:\n  --headless          render one frame to stdout and exit\n  --dump-frame       print the final cell frame\n  --keys KEYS        feed keys (normal notation, e.g. ihello<Esc>:wq<CR>)\n  -c COMMAND         execute an Ex command after loading\n  -p                 open files in tabs\n  -o                 open files in horizontal splits\n  -O                 open files in vertical splits\n  -u NONE|NORC|DEFAULTS  startup file mode\n  --cmd COMMAND      execute an Ex command before loading\n  --clean            skip user startup files\n  -R                 readonly\n  --startuptime FILE write startup timing\n  -V[LEVEL][FILE]     set 'verbose' level and optionally log to FILE\n  --log FILE          append diagnostics log to FILE\n  --log-categories L  comma-separated categories (input,ex,viml,render,runtime,job,js or all)\n  --version           print version\n  --help              this help\n",
        rvim_types::RVIM_VERSION
    );
}

fn parse_keys(input: &str) -> Vec<KeyEvent> {
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

fn parse_key_token(token: &str) -> Option<KeyEvent> {
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
        "nop" => None,
        _ if lower.starts_with("f") && lower[1..].parse::<u8>().is_ok() => {
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

#[allow(dead_code)]
fn _mode_name(mode: UiMode) -> &'static str {
    match mode {
        UiMode::Normal => "normal",
        UiMode::Insert => "insert",
        UiMode::Replace => "replace",
        UiMode::Visual => "visual",
        UiMode::Select => "select",
        UiMode::Command => "command",
        UiMode::More => "more",
        UiMode::Terminal => "terminal",
    }
}
