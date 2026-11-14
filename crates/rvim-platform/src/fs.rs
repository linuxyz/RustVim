use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FsError {
    pub message: String,
    pub kind: std::io::ErrorKind,
}

impl FsError {
    pub fn new(error: std::io::Error) -> Self {
        Self {
            message: error.to_string(),
            kind: error.kind(),
        }
    }

    pub fn is_not_found(&self) -> bool {
        self.kind == std::io::ErrorKind::NotFound
    }

    pub fn is_permission(&self) -> bool {
        self.kind == std::io::ErrorKind::PermissionDenied
    }
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FsError {}

impl From<std::io::Error> for FsError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileInfo {
    pub len: u64,
    pub modified: Option<SystemTime>,
    pub readonly: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
}

impl FileInfo {
    pub fn read(path: &Path) -> Result<Self, FsError> {
        let metadata = fs::symlink_metadata(path)?;
        let readonly = metadata.permissions().readonly();
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            readonly,
            is_dir: metadata.is_dir(),
            is_symlink: metadata.file_type().is_symlink(),
        })
    }
}

pub fn read_file(path: &Path) -> Result<Vec<u8>, FsError> {
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Write through a temporary file in the same directory, then rename. This
/// avoids truncating the target when the write fails halfway.
pub fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), FsError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.exists() {
        return Err(FsError {
            message: format!("directory does not exist: {}", parent.display()),
            kind: std::io::ErrorKind::NotFound,
        });
    }
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "rvim.tmp".to_string());
    let temp = parent.join(format!(".{}.rvim-{}.tmp", file_name, std::process::id()));
    {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    match fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error.into())
        }
    }
}

pub fn canonicalize(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn expand_env(path: &str) -> PathBuf {
    let mut out = String::new();
    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            let mut name = String::new();
            if chars.peek() == Some(&'{') {
                chars.next();
                for next in chars.by_ref() {
                    if next == '}' {
                        break;
                    }
                    name.push(next);
                }
            } else {
                while let Some(next) = chars.peek() {
                    if next.is_ascii_alphanumeric() || *next == '_' {
                        name.push(*next);
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            if let Ok(value) = std::env::var(&name) {
                out.push_str(&value);
            } else {
                out.push('$');
                out.push_str(&name);
            }
        } else {
            out.push(ch);
        }
    }
    expand_tilde(&out)
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(path)
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn set_cwd(path: &Path) -> Result<(), FsError> {
    std::env::set_current_dir(path)?;
    Ok(())
}

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        return path.is_file().then_some(path);
    }
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
