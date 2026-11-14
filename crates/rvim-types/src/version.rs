//! Version and feature reporting.

pub const VIM_VERSION: &str = "9.2.1338";
pub const RVIM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Dynamic feature query used by has().
///
/// Unsupported features must answer false rather than pretending. Vim9 script,
/// the terminal emulator and the image API are intentionally false.
pub fn has_feature(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "vim9script" | "terminal" | "gui" | "gui_running" | "clientserver" | "sun_workshop" => {
            false
        }
        "image" | "images" | "sixel" | "kitty" | "pixelm" => false,
        "quickjs" => false,
        "unix" => cfg!(unix),
        "win32" | "win64" => cfg!(windows),
        "mac" | "macunix" | "osx" => cfg!(target_os = "macos"),
        "linux" => cfg!(target_os = "linux"),
        "bsd" => cfg!(any(
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd"
        )),
        "multi_byte" | "multi_lang" | "mbyte" => true,
        "syntax" | "eval" | "user_commands" | "cmdline_hist" | "cmdline_info" | "viminfo"
        | "persistent_undo" => true,
        "autocmd" | "insert_expand" | "jumplist" | "modify_fname" => true,
        "filterpipe" | "quickfix" | "linebreak" | "wildmenu" | "digraphs" | "statusline_click"
        | "compl_func" => true,
        "folding" | "signs" => true,
        "spell" | "diff" | "session" | "crypt" | "vartabs" | "textprop" | "printer"
        | "postscript" | "arabic" | "rightleft" | "xim" | "menu" | "toolbar" | "wayland"
        | "tabpanel" | "termguicolors" | "mouseshape" | "browse" | "keymap" | "langmap"
        | "find_id" | "autochdir" | "autoshelldir" | "emacs_tags" | "xterm_clipboard"
        | "render_options" | "waks" | "con_dialog" | "balloon_eval" | "unsupported" => false,
        "cscope" | "netbeans_intg" => false,
        "python" | "python3" | "ruby" | "perl" | "tcl" | "lua" | "mzscheme" => false,
        "job" | "channel" | "timers" | "pty" => cfg!(unix),
        "clipboard" | "clipboard_working" => true,
        "mouse" => true,
        "title" => true,
        "iconv" => cfg!(unix),
        "nvim" => false,
        _ => false,
    }
}
