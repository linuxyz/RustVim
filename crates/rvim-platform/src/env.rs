use std::path::PathBuf;

pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

pub fn var_os(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name)
}

pub fn home() -> PathBuf {
    var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn vimruntime() -> Option<PathBuf> {
    var_os("VIMRUNTIME").map(PathBuf::from)
}

pub fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

pub fn hostname() -> String {
    var("HOSTNAME")
        .or_else(|| var("COMPUTERNAME"))
        .unwrap_or_else(|| "localhost".to_string())
}

pub fn username() -> String {
    var("USER")
        .or_else(|| var("LOGNAME"))
        .unwrap_or_else(|| "user".to_string())
}
