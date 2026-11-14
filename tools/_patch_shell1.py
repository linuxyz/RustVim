import pathlib
p = pathlib.Path("crates/rvim-platform/src/process.rs")
s = p.read_text()
old = """pub fn run_shell(command: &str, input: Option<&[u8]>) -> Result<CommandOutput, ProcessError> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let mut child = Command::new(shell)
        .arg("-c")
        .arg(command)"""
new = """pub fn run_shell(command: &str, input: Option<&[u8]>) -> Result<CommandOutput, ProcessError> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    run_shell_command(&shell, "-c", command, input)
}

/// Run a command through an explicit shell and flag.
pub fn run_shell_command(
    shell: &str,
    flag: &str,
    command: &str,
    input: Option<&[u8]>,
) -> Result<CommandOutput, ProcessError> {
    let shell = if shell.is_empty() { "/bin/sh" } else { shell };
    let mut builder = Command::new(shell);
    for part in flag.split_whitespace() {
        builder.arg(part);
    }
    let mut child = builder
        .arg(command)"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
p = pathlib.Path("crates/rvim-platform/src/lib.rs")
s = p.read_text()
s = s.replace("pub use process::{run_shell, CommandOutput, ProcessError};", "pub use process::{run_shell, run_shell_command, CommandOutput, ProcessError};")
p.write_text(s)
print("platform patched")
