import pathlib
p = pathlib.Path("crates/rvim-platform/src/term.rs")
s = p.read_text()
anchor = """/// Whether the terminal is expected to understand ANSI escape sequences."""
caps = """#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TermCaps {
    pub ansi: bool,
    pub colors: u16,
    pub truecolor: bool,
}

pub fn capabilities() -> TermCaps {
    let mut caps = capabilities_for(&term_name());
    if caps.ansi && colorterm_truecolor() {
        caps.truecolor = true;
        caps.colors = 256;
    }
    caps
}

/// Small built-in terminfo subset for common terminals.
pub fn capabilities_for(term: &str) -> TermCaps {
    let term = term.trim().to_ascii_lowercase();
    if !supports_ansi_for(&term) {
        return TermCaps {
            ansi: false,
            colors: 0,
            truecolor: false,
        };
    }
    let colors = if term.contains("256color") || term.contains("truecolor") {
        256
    } else if term.contains("color")
        || term.contains("xterm")
        || term.contains("screen")
        || term.contains("tmux")
        || term.contains("linux")
        || term.contains("vt100")
        || term.contains("ansi")
    {
        16
    } else {
        8
    };
    TermCaps {
        ansi: true,
        colors,
        truecolor: false,
    }
}

/// Whether the terminal is expected to understand ANSI escape sequences."""
assert anchor in s
s = s.replace(anchor, caps, 1)
# extend tests
s = s.replace("""        assert!(supports_ansi_for("xterm-256color"));
    }""", """        assert!(supports_ansi_for("xterm-256color"));
    }

    #[test]
    fn common_terminal_capabilities() {
        assert_eq!(capabilities_for("dumb").colors, 0);
        assert_eq!(capabilities_for("xterm-256color").colors, 256);
        assert_eq!(capabilities_for("xterm").colors, 16);
        assert!(capabilities_for("xterm").ansi);
    }""", 1)
p.write_text(s)
print("term patched")
p = pathlib.Path("crates/rvim-tui/src/lib.rs")
s = p.read_text()
s = s.replace("""fn detect_color_depth() -> rvim_ui_api::backend::ColorDepth {
    use rvim_ui_api::backend::ColorDepth;
    if term::term_name() == "dumb" {
        return ColorDepth::Mono;
    }
    if term::colorterm_truecolor() {
        return ColorDepth::TrueColor;
    }
    if term::term_name().contains("256color") {
        return ColorDepth::Ansi256;
    }
    ColorDepth::Ansi16
}""", """fn detect_color_depth() -> rvim_ui_api::backend::ColorDepth {
    use rvim_ui_api::backend::ColorDepth;
    let caps = term::capabilities();
    if !caps.ansi {
        ColorDepth::Mono
    } else if caps.truecolor {
        ColorDepth::TrueColor
    } else if caps.colors >= 256 {
        ColorDepth::Ansi256
    } else {
        ColorDepth::Ansi16
    }
}""", 1)
p.write_text(s)
print("tui patched")
