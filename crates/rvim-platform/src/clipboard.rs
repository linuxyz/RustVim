use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardKind {
    Unnamed,
    Selection,
    Plus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardProvider {
    MacOS,
    Wayland,
    Xclip,
    Xsel,
    None,
}

pub fn detect_provider() -> ClipboardProvider {
    #[cfg(target_os = "macos")]
    {
        if crate::fs::find_in_path("pbcopy").is_some() {
            return ClipboardProvider::MacOS;
        }
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && crate::fs::find_in_path("wl-copy").is_some()
    {
        return ClipboardProvider::Wayland;
    }
    if crate::fs::find_in_path("xclip").is_some() {
        return ClipboardProvider::Xclip;
    }
    if crate::fs::find_in_path("xsel").is_some() {
        return ClipboardProvider::Xsel;
    }
    ClipboardProvider::None
}

pub fn clipboard_get(kind: ClipboardKind) -> Result<Option<String>, String> {
    let provider = detect_provider();
    let mut command = match provider {
        ClipboardProvider::MacOS => {
            let mut c = Command::new("pbpaste");
            if kind == ClipboardKind::Selection {
                c.arg("-pboard").arg("selection");
            } else {
                c.arg("-pboard").arg("general");
            }
            c
        }
        ClipboardProvider::Wayland => {
            let mut c = Command::new("wl-paste");
            c.arg("--no-newline");
            if kind == ClipboardKind::Selection {
                c.arg("--primary");
            }
            c
        }
        ClipboardProvider::Xclip => {
            let mut c = Command::new("xclip");
            c.arg("-o")
                .arg("-selection")
                .arg(if kind == ClipboardKind::Selection {
                    "primary"
                } else {
                    "clipboard"
                });
            c
        }
        ClipboardProvider::Xsel => {
            let mut c = Command::new("xsel");
            c.arg("--output");
            if kind == ClipboardKind::Selection {
                c.arg("--primary");
            } else {
                c.arg("--clipboard");
            }
            c
        }
        ClipboardProvider::None => return Ok(None),
    };
    let output = command.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
}

pub fn clipboard_set(kind: ClipboardKind, data: &str) -> Result<(), String> {
    let provider = detect_provider();
    let mut command = match provider {
        ClipboardProvider::MacOS => {
            let mut c = Command::new("pbcopy");
            if kind == ClipboardKind::Selection {
                c.arg("-pboard").arg("selection");
            } else {
                c.arg("-pboard").arg("general");
            }
            c
        }
        ClipboardProvider::Wayland => {
            let mut c = Command::new("wl-copy");
            if kind == ClipboardKind::Selection {
                c.arg("--primary");
            }
            c
        }
        ClipboardProvider::Xclip => {
            let mut c = Command::new("xclip");
            c.arg("-i")
                .arg("-selection")
                .arg(if kind == ClipboardKind::Selection {
                    "primary"
                } else {
                    "clipboard"
                });
            c
        }
        ClipboardProvider::Xsel => {
            let mut c = Command::new("xsel");
            c.arg("--input");
            if kind == ClipboardKind::Selection {
                c.arg("--primary");
            } else {
                c.arg("--clipboard");
            }
            c
        }
        ClipboardProvider::None => return Err("clipboard provider not found".to_string()),
    };
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(data.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("clipboard provider exited with {status}"))
    }
}
