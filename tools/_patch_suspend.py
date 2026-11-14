import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("""    pub scheduler: crate::scheduler::Scheduler,""", """    pub scheduler: crate::scheduler::Scheduler,
    pub suspend_requested: bool,""", 1)
s = s.replace("""            scheduler: crate::scheduler::Scheduler::new(),""", """            scheduler: crate::scheduler::Scheduler::new(),
            suspend_requested: false,""", 1)
p.write_text(s)
print("editor field patched")
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
anchor = '''        "redraw" | "red" | "redr" | "redra" => {'''
arms = '''        "stop" | "suspend" | "sus" | "st" => {
            editor.suspend_requested = true;
        }
        "redraw" | "red" | "redr" | "redra" => {'''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("ex patched")
p = pathlib.Path("crates/rvim-editor/src/registry_generated.rs")
s = p.read_text()
for name in ['stop','suspend']:
    idx=s.find(f'name: "{name}"')
    end=s.find('implemented: false',idx)
    if idx>=0 and end>=0:
        s=s[:end]+'implemented: true'+s[end+len('implemented: false'):]
p.write_text(s)
print("registry patched")
p = pathlib.Path("crates/rvim-platform/src/term.rs")
s = p.read_text()
s += '''

/// Suspend the current process until it is resumed (Unix SIGTSTP).
#[cfg(unix)]
pub fn suspend_process() {
    use std::os::raw::c_int;
    #[cfg(target_os = "macos")]
    const SIGTSTP: c_int = 18;
    #[cfg(not(target_os = "macos"))]
    const SIGTSTP: c_int = 20;
    extern "C" {
        fn raise(signal: c_int) -> c_int;
    }
    // SAFETY: raise is called with the SIGTSTP signal constant.
    unsafe {
        raise(SIGTSTP);
    }
}

#[cfg(not(unix))]
pub fn suspend_process() {}
'''
p.write_text(s)
print("platform patched")
