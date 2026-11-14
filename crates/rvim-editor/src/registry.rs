//! Metadata-driven Vim command registry.

pub use crate::registry_generated::{NameSpec, ALL_COMMANDS, ALL_EVENTS, ALL_FUNCTIONS};

pub fn command_abbrev(name: &str) -> Option<&'static str> {
    crate::registry_abbrev::COMMAND_ABBREVIATIONS
        .iter()
        .find(|(full, _)| *full == name)
        .map(|(_, least)| *least)
}

/// Resolve a command prefix to its full name.
///
/// Vim accepts any unambiguous prefix down to each command's documented
/// minimum abbreviation and resolves ambiguity to the first match in command
/// table order.  This lets runtime files use spellings such as `sy` (syntax)
/// or `synt` (syntax) that are not listed in the dispatch match arms.
pub fn resolve_command_name(name: &str) -> Option<&'static str> {
    for spec in ALL_COMMANDS {
        let Some(least) = command_abbrev(spec.name) else {
            continue;
        };
        if name.len() >= least.len() && spec.name.starts_with(name) {
            return Some(spec.name);
        }
    }
    None
}

pub fn command_metadata(name: &str) -> Option<&'static NameSpec> {
    ALL_COMMANDS
        .iter()
        .find(|spec| spec.name == name || spec.name.starts_with(name) && name.len() >= 2)
}

pub fn function_metadata(name: &str) -> Option<&'static NameSpec> {
    ALL_FUNCTIONS.iter().find(|spec| spec.name == name)
}

pub fn event_metadata(name: &str) -> Option<&'static NameSpec> {
    ALL_EVENTS
        .iter()
        .find(|spec| spec.name.eq_ignore_ascii_case(name))
}

pub struct CommandSpec {
    pub name: &'static str,
    pub abbrev: &'static str,
    pub bang: bool,
    pub range: bool,
    pub doc: &'static str,
}

macro_rules! cmd {
    ($name:literal, $abbrev:literal, $bang:literal, $range:literal, $doc:literal) => {
        CommandSpec {
            name: $name,
            abbrev: $abbrev,
            bang: $bang,
            range: $range,
            doc: $doc,
        }
    };
}

/// Commands implemented by the editor. This table is the source for
/// command-line completion and the catalog audit; handlers remain in ex.rs.
pub static COMMANDS: &[CommandSpec] = &[
    cmd!("autocmd", "au", true, false, ":autocmd"),
    cmd!("augroup", "aug", true, false, ":augroup"),
    cmd!("badd", "ba", false, false, ":badd"),
    cmd!("bdelete", "bd", true, false, ":bdelete"),
    cmd!("bnext", "bn", false, false, ":bnext"),
    cmd!("bprevious", "bp", false, false, ":bprevious"),
    cmd!("buffer", "bu", false, false, ":buffer"),
    cmd!("buffers", "ls", false, false, ":buffers"),
    cmd!("bwipeout", "bw", true, false, ":bwipeout"),
    cmd!("cd", "cd", false, false, ":cd"),
    cmd!("close", "clo", true, false, ":close"),
    cmd!("cmap", "cmap", false, false, ":cmap"),
    cmd!("cnoremap", "cno", false, false, ":cnoremap"),
    cmd!("cunmap", "cu", false, false, ":cunmap"),
    cmd!("delete", "d", true, true, ":delete"),
    cmd!("doautocmd", "doau", false, false, ":doautocmd"),
    cmd!("echo", "ec", false, false, ":echo"),
    cmd!("fold", "fo", false, true, ":fold"),
    cmd!("foldclose", "foldc", true, true, ":foldclose"),
    cmd!("folddoopen", "foldd", false, true, ":folddoopen"),
    cmd!("folddoclosed", "folddoc", false, true, ":folddoclosed"),
    cmd!("foldopen", "foldo", true, true, ":foldopen"),
    cmd!("edit", "e", true, false, ":edit"),
    cmd!("imap", "im", false, false, ":imap"),
    cmd!("inoremap", "ino", false, false, ":inoremap"),
    cmd!("iunmap", "iu", false, false, ":iunmap"),
    cmd!("join", "j", true, true, ":join"),
    cmd!("jumps", "ju", false, false, ":jumps"),
    cmd!("map", "map", true, false, ":map"),
    cmd!("mapclear", "mapc", true, false, ":mapclear"),
    cmd!("marks", "marks", false, false, ":marks"),
    cmd!("new", "new", true, false, ":new"),
    cmd!("nmap", "nm", false, false, ":nmap"),
    cmd!("nnoremap", "nn", false, false, ":nnoremap"),
    cmd!("noremap", "no", false, false, ":noremap"),
    cmd!("normal", "norm", true, true, ":normal"),
    cmd!("nunmap", "nun", false, false, ":nunmap"),
    cmd!("only", "on", true, false, ":only"),
    cmd!("print", "p", false, true, ":print"),
    cmd!("pwd", "pwd", false, false, ":pwd"),
    cmd!("put", "pu", true, true, ":put"),
    cmd!("quit", "q", true, false, ":quit"),
    cmd!("read", "r", true, true, ":read"),
    cmd!("registers", "reg", false, false, ":registers"),
    cmd!("sbuffer", "sb", true, false, ":sbuffer"),
    cmd!("set", "se", false, false, ":set"),
    cmd!("sign", "sig", false, true, ":sign"),
    cmd!("setglobal", "setg", false, false, ":setglobal"),
    cmd!("setlocal", "setl", false, false, ":setlocal"),
    cmd!("split", "sp", true, false, ":split"),
    cmd!("tabclose", "tabc", true, false, ":tabclose"),
    cmd!("tabnew", "tabe", true, false, ":tabnew"),
    cmd!("tabnext", "tabn", false, false, ":tabnext"),
    cmd!("tabprevious", "tabp", false, false, ":tabprevious"),
    cmd!("unmap", "unm", false, false, ":unmap"),
    cmd!("version", "ve", false, false, ":version"),
    cmd!("vmap", "vm", false, false, ":vmap"),
    cmd!("vnoremap", "vn", false, false, ":vnoremap"),
    cmd!("vnew", "vne", true, false, ":vnew"),
    cmd!("vsplit", "vs", true, false, ":vsplit"),
    cmd!("vunmap", "vu", false, false, ":vunmap"),
    cmd!("wincmd", "winc", false, false, ":wincmd"),
    cmd!("write", "w", true, true, ":write"),
    cmd!("wq", "wq", true, false, ":wq"),
    cmd!("x", "x", true, false, ":x"),
    cmd!("xit", "xi", true, false, ":xit"),
    cmd!("yank", "y", false, true, ":yank"),
];

pub fn completions(prefix: &str) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = COMMANDS
        .iter()
        .filter(|command| command.name.starts_with(prefix) || command.abbrev.starts_with(prefix))
        .map(|command| command.name)
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}
