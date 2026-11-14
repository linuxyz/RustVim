//! Optional category-based diagnostics logging and crash reports.
//!
//! Logging is off unless a log file is configured (the TUI/CLI `--log`
//! option or [`crate::editor::Editor::set_trace`]).  Categories are matched against
//! `--log-categories`; `all` enables every category.  Supported categories:
//! `input`, `ex`, `viml`, `render`, `runtime`, `job` and `js` (job and js
//! are reserved for subsystems that are not part of the P0/P1 scope yet).

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Category-selective log sink.
#[derive(Debug, Default)]
pub struct Trace {
    file: Option<File>,
    categories: Vec<String>,
}

impl Trace {
    /// Opens `path` for appending and records a session marker.
    pub fn open(path: &Path, categories: &str) -> Result<Self, String> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| format!("E484: Can't open file {}: {error}", path.display()))?;
        let _ = writeln!(file, "==== start rvim log session {} ====", timestamp());
        Ok(Self {
            file: Some(file),
            categories: parse_categories(categories),
        })
    }

    pub fn is_active(&self) -> bool {
        self.file.is_some()
    }

    /// True when `category` is enabled and a log file is open.
    pub fn enabled_for(&self, category: &str) -> bool {
        self.file.is_some()
            && self
                .categories
                .iter()
                .any(|item| item == "all" || item == category)
    }

    pub fn log(&mut self, category: &str, message: &str) {
        if !self.enabled_for(category) {
            return;
        }
        if let Some(file) = self.file.as_mut() {
            let _ = writeln!(file, "{} [{category}] {message}", timestamp());
            let _ = file.flush();
        }
    }
}

fn parse_categories(categories: &str) -> Vec<String> {
    categories
        .split(',')
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Seconds and milliseconds since the Unix epoch.
pub fn timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => format!("{}.{:03}", elapsed.as_secs(), elapsed.subsec_millis()),
        Err(_) => "0.000".to_string(),
    }
}

/// Writes a crash report and returns the path it was written to.
pub fn write_crash_report(
    log: Option<&Path>,
    payload: &str,
    location: Option<&str>,
) -> Option<PathBuf> {
    let path = log.map_or_else(
        || std::env::temp_dir().join(format!("rvim-crash-{}.log", std::process::id())),
        Path::to_path_buf,
    );
    let mut text = format!(
        "RVim crash report\nversion: {}\ntarget: {}\ntime: {}\n",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_NAME"),
        timestamp()
    );
    if let Some(location) = location {
        text.push_str(&format!("location: {location}\n"));
    }
    text.push_str(&format!("message: {payload}\n"));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    file.write_all(text.as_bytes()).ok()?;
    Some(path)
}

/// Installs a panic hook that writes a crash report before the default hook
/// prints the message.  The report goes to `log` when given, otherwise to a
/// temporary file.
pub fn install_crash_hook(log: Option<PathBuf>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| (*text).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string());
        let location = info.location().map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        });
        if let Some(path) = write_crash_report(log.as_deref(), &payload, location.as_deref()) {
            eprintln!("rvim: crash report written to {}", path.display());
        }
        previous(info);
    }));
}
