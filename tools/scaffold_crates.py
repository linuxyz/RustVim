from pathlib import Path
crates = {
    "rvim-types": [],
    "rvim-ui-api": [],
    "rvim-platform": ["rvim-types"],
    "rvim-text": ["rvim-types"],
    "rvim-regex": ["rvim-types"],
    "rvim-viml": ["rvim-types"],
    "rvim-ex": ["rvim-types"],
    "rvim-edit": ["rvim-types", "rvim-text", "rvim-regex"],
    "rvim-syntax": ["rvim-types", "rvim-text", "rvim-regex"],
    "rvim-runtime": ["rvim-types", "rvim-platform"],
    "rvim-js": ["rvim-types", "rvim-viml"],
    "rvim-view": ["rvim-types", "rvim-ui-api", "rvim-text", "rvim-syntax", "rvim-regex"],
    "rvim-editor": ["rvim-types", "rvim-ui-api", "rvim-platform", "rvim-text", "rvim-regex",
                     "rvim-viml", "rvim-ex", "rvim-edit", "rvim-syntax", "rvim-runtime",
                     "rvim-js", "rvim-view"],
    "rvim-tui": ["rvim-types", "rvim-ui-api", "rvim-platform"],
    "rvim-headless": ["rvim-types", "rvim-ui-api"],
    "rvim-cli": ["rvim-types", "rvim-ui-api", "rvim-platform", "rvim-text", "rvim-regex",
                  "rvim-viml", "rvim-ex", "rvim-edit", "rvim-syntax", "rvim-runtime",
                  "rvim-js", "rvim-view", "rvim-editor", "rvim-tui", "rvim-headless"],
}
for name, deps in crates.items():
    d = Path("crates") / name
    lines = [
        "[package]",
        f'name = "{name}"',
        'version.workspace = true',
        'edition.workspace = true',
        'rust-version.workspace = true',
        'license.workspace = true',
        'description = "RVim: a Rust implementation of Vim"',
        "",
        "[dependencies]",
    ]
    for dep in deps:
        lines.append(f'{dep}.workspace = true')
    if name == "rvim-cli":
        lines += ["", "[[bin]]", 'name = "rvim"', 'path = "src/main.rs"']
    (d / "Cargo.toml").write_text("\n".join(lines) + "\n")
print("rewrote manifests")
