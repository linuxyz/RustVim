use std::path::{Path, PathBuf};

use rvim_types::BufferId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwapInfo {
    pub path: PathBuf,
    pub original: PathBuf,
    pub pid: u32,
    pub changedtick: u64,
}

pub fn swap_path(file: &Path, directory: Option<&str>) -> PathBuf {
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_string());
    let swap_name = format!(".{name}.swp");
    if let Some(directory) = directory {
        for entry in directory.split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let path = rvim_platform::fs::expand_tilde(entry);
            if path.is_dir() {
                return path.join(&swap_name);
            }
        }
    }
    file.parent().unwrap_or(Path::new(".")).join(swap_name)
}

pub fn write_swap(
    path: &Path,
    original: &Path,
    text: &[u8],
    changedtick: u64,
) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    if !parent.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "swap directory missing",
        ));
    }
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("rvim"),
        std::process::id()
    ));
    let mut data = Vec::new();
    data.extend_from_slice(b"RVIMSWAP1\n");
    data.extend_from_slice(format!("pid {}\n", std::process::id()).as_bytes());
    data.extend_from_slice(format!("tick {changedtick}\n").as_bytes());
    data.extend_from_slice(format!("file {}\n\n", original.display()).as_bytes());
    data.extend_from_slice(text);
    std::fs::write(&temp, data)?;
    std::fs::rename(&temp, path)
}

pub fn read_swap(path: &Path) -> std::io::Result<(PathBuf, Vec<u8>, u64)> {
    let data = std::fs::read(path)?;
    let marker = b"RVIMSWAP1\n";
    if !data.starts_with(marker) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not an RVim swap file",
        ));
    }
    let rest = &data[marker.len()..];
    let header_end = rest
        .windows(2)
        .position(|window| window == b"\n\n")
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "truncated swap header")
        })?;
    let header = String::from_utf8_lossy(&rest[..header_end]);
    let mut original = None;
    let mut tick = 0;
    for line in header.lines() {
        if let Some(value) = line.strip_prefix("file ") {
            original = Some(PathBuf::from(value));
        } else if let Some(value) = line.strip_prefix("tick ") {
            tick = value.parse().unwrap_or(0);
        }
    }
    let original = original.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "swap file has no original path",
        )
    })?;
    Ok((original, rest[header_end + 2..].to_vec(), tick))
}

pub fn remove_swap(path: &Path) {
    let _ = std::fs::remove_file(path);
}

pub struct SwapState {
    paths: std::collections::HashMap<BufferId, PathBuf>,
}

impl Default for SwapState {
    fn default() -> Self {
        Self::new()
    }
}

impl SwapState {
    pub fn new() -> Self {
        Self {
            paths: std::collections::HashMap::new(),
        }
    }

    pub fn set(&mut self, buffer: BufferId, path: PathBuf) {
        self.paths.insert(buffer, path);
    }

    pub fn get(&self, buffer: BufferId) -> Option<&PathBuf> {
        self.paths.get(&buffer)
    }

    pub fn remove(&mut self, buffer: BufferId) {
        if let Some(path) = self.paths.remove(&buffer) {
            remove_swap(&path);
        }
    }

    pub fn clear(&mut self) {
        for (_, path) in self.paths.drain() {
            remove_swap(&path);
        }
    }
}
