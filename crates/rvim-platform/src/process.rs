use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessError(pub String);

impl std::fmt::Display for ProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProcessError {}

pub fn run_shell(command: &str, input: Option<&[u8]>) -> Result<CommandOutput, ProcessError> {
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
        .arg(command)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ProcessError(error.to_string()))?;
    if let Some(bytes) = input {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(bytes)
                .map_err(|error| ProcessError(error.to_string()))?;
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| ProcessError(error.to_string()))?;
    Ok(CommandOutput {
        status: output.status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

pub fn shell_command(command: &str) -> Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let mut cmd = Command::new(shell);
    cmd.arg("-c").arg(command);
    cmd
}

/// Converts bytes between encodings using the system iconv(1) as Vim's
/// +iconv fallback would use libiconv.  Returns None when iconv is missing or
/// reports an error.
pub fn iconv(input: &[u8], from: &str, to: &str) -> Option<Vec<u8>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("iconv")
        .args(["-f", from, "-t", to])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(input).ok()?;
    let output = child.wait_with_output().ok()?;
    if output.status.success() {
        Some(output.stdout)
    } else {
        None
    }
}
