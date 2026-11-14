import pathlib
p = pathlib.Path("crates/rvim-cli/src/main.rs")
s = p.read_text()
# enum + fields
s = s.replace("""#[derive(Debug, Default)]
struct Args {""", """#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WindowLayout {
    #[default]
    Default,
    Tabs,
    Horizontal,
    Vertical,
}

#[derive(Debug, Default)]
struct Args {""", 1)
s = s.replace("""    session: Option<PathBuf>,
}""", """    session: Option<PathBuf>,
    layout: WindowLayout,
    norc: bool,
    defaults_only: bool,
}""", 1)
# parse -u NORC/DEFAULTS
s = s.replace("""                if value.eq_ignore_ascii_case("NONE") {
                    args.clean = true;
                } else {
                    args.vimrc = Some(value);
                }""", """                if value.eq_ignore_ascii_case("NONE") {
                    args.clean = true;
                } else if value.eq_ignore_ascii_case("NORC") {
                    args.norc = true;
                } else if value.eq_ignore_ascii_case("DEFAULTS") {
                    args.defaults_only = true;
                } else {
                    args.vimrc = Some(value);
                }""", 1)
# parse -p/-o/-O
s = s.replace("""            "-p" | "-o" | "-O" => {
                // The first window shows the first file; split/tab opening lands
                // with the window milestone.
            }""", """            "-p" => args.layout = WindowLayout::Tabs,
            "-o" => args.layout = WindowLayout::Horizontal,
            "-O" => args.layout = WindowLayout::Vertical,""", 1)
# make_editor: pre_commands before vimrc, handle norc/defaults
old = """fn make_editor(args: &Args, size: Size) -> Editor {
    let mut editor = Editor::new(size);
    if args.no_swap {
        editor.swap_enabled = false;
        let _ = editor
            .options
            .set_global("swapfile", rvim_editor::OptionValue::Bool(false));
    }
    if !args.clean {
        if let Some(vimrc) = &args.vimrc {
            source_ex_file(&mut editor, std::path::Path::new(vimrc));
        } else if let Some(defaults) = editor.runtime.find("defaults.vim") {
            source_ex_file(&mut editor, &defaults);
        }
    }
    if args.clean {
        let _ = editor.options.set_global(
            "viminfo",
            rvim_editor::OptionValue::String(rvim_types::VimString::new()),
        );
    }
    let _ = args.no_swap;
    editor
}"""
new = """fn make_editor(args: &Args, size: Size) -> Editor {
    let mut editor = Editor::new(size);
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
        } else if let Some(defaults) = editor.runtime.find("defaults.vim") {
            source_ex_file(&mut editor, &defaults);
        }
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
}"""
assert old in s
s = s.replace(old, new, 1)
# remove pre_commands loops and first-file open in both modes, use open_files
s = s.replace("""    for command in &args.pre_commands {
        editor.execute_ex(command);
    }
    if let Some(file) = args.files.first() {
        editor.open_file(file).map_err(|e| e.to_string())?;
    }
    if args.recover {""", """    open_files(&mut editor, &args)?;
    if args.recover {""", 1)
s = s.replace("""    for command in &args.pre_commands {
        editor.execute_ex(command);
    }
    if let Some(file) = args.files.first() {
        editor.open_file(file).map_err(|e| e.to_string())?;
    }
    if args.recover {""", """    open_files(&mut editor, &args)?;
    if args.recover {""", 1)
# startuptime actual in headless
s = s.replace("""fn run_headless(args: Args) -> Result<(), String> {
    let size = Size::new(24, 80);""", """fn run_headless(args: Args) -> Result<(), String> {
    let started = Instant::now();
    let size = Size::new(24, 80);""", 1)
s = s.replace("""        std::fs::write(path, b"rvim startup: 0.000\\n").map_err(|e| e.to_string())?;""", """        let line = format!("rvim startup: {:.3}\\n", started.elapsed().as_secs_f64() * 1000.0);
        std::fs::write(path, line).map_err(|e| e.to_string())?;""", 1)
# TUI startuptime? add started and write at end
s = s.replace("""fn run_tui(args: Args) -> Result<(), String> {
    let mut backend = TuiBackend::new();""", """fn run_tui(args: Args) -> Result<(), String> {
    let started = Instant::now();
    let mut backend = TuiBackend::new();""", 1)
# find end of run_tui; add after editor shutdown? inspect later
p.write_text(s)
print("patched")
