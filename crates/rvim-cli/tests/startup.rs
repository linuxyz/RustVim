use std::path::PathBuf;
use std::process::Command;

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("rvim-cli-{}-{name}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_rvim(home: &PathBuf, work: &PathBuf) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rvim"))
        .arg("--headless")
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("VIMRUNTIME")
        .current_dir(work)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn startup_sources_user_vimrc() {
    let home = temp_dir("home-vimrc");
    let work = temp_dir("work-vimrc");
    std::fs::write(home.join(".vimrc"), b"echo 'from-vimrc'\n").unwrap();
    let output = run_rvim(&home, &work);
    assert!(output.contains("from-vimrc"), "{output}");
}

#[test]
fn startup_sources_local_exrc_when_enabled() {
    let home = temp_dir("home-exrc");
    let work = temp_dir("work-exrc");
    std::fs::write(home.join(".vimrc"), b"set exrc\n").unwrap();
    std::fs::write(work.join(".vimrc"), b"echo 'from-exrc'\n").unwrap();
    let output = run_rvim(&home, &work);
    assert!(output.contains("from-exrc"), "{output}");
}

#[test]
fn startup_secure_exrc_rejects_shell_commands() {
    let home = temp_dir("home-secure");
    let work = temp_dir("work-secure");
    std::fs::write(home.join(".vimrc"), b"set exrc\nset secure\n").unwrap();
    std::fs::write(work.join(".vimrc"), b"!echo pwned\n").unwrap();
    let output = run_rvim(&home, &work);
    assert!(output.contains("not allowed from exrc"), "{output}");
    assert!(!output.contains("pwned"), "{output}");
}
#[test]
fn log_option_records_selected_categories() {
    let home = temp_dir("home-log");
    let work = temp_dir("work-log");
    let log = work.join("rvim.log");
    let output = Command::new(env!("CARGO_BIN_EXE_rvim"))
        .args([
            "--headless",
            "--log",
            log.to_str().unwrap(),
            "--log-categories",
            "ex",
            "-c",
            "set number",
        ])
        .env("HOME", &home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("VIMRUNTIME")
        .current_dir(&work)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("start rvim log session"), "{text}");
    assert!(text.contains("[ex] set number"), "{text}");
    assert!(!text.contains("[input]"), "{text}");
}
#[test]
fn verbose_flag_sets_option_and_log_file() {
    let home = temp_dir("home-verbose");
    let work = temp_dir("work-verbose");
    let log = work.join("verbose.log");
    let verbose_arg = format!("-V2{}", log.display());
    let output = Command::new(env!("CARGO_BIN_EXE_rvim"))
        .args(["--headless", &verbose_arg, "-c", "echo &verbose"])
        .env("HOME", &home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("VIMRUNTIME")
        .current_dir(&work)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains('2'), "{stderr}");
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("start rvim log session"), "{text}");
    assert!(text.contains("[ex] echo &verbose"), "{text}");
}
