import pathlib
p = pathlib.Path("crates/rvim-tui/src/lib.rs")
s = p.read_text()
s = s.replace("""            caps: UiCapabilities {
                color: if term::colorterm_truecolor() {
                    rvim_ui_api::backend::ColorDepth::TrueColor
                } else {
                    rvim_ui_api::backend::ColorDepth::Ansi256
                },""", """            caps: UiCapabilities {
                color: detect_color_depth(),""", 1)
anchor = """fn sgr(attr: Attr, depth: rvim_ui_api::backend::ColorDepth) -> String {"""
helper = """fn detect_color_depth() -> rvim_ui_api::backend::ColorDepth {
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
}

fn ansi16_code(index: u8, foreground: bool) -> u8 {
    let index = index % 16;
    let base = if index < 8 { 30 } else { 82 };
    let code = base + (index % 8);
    if foreground {
        code
    } else {
        code + 10
    }
}

fn rgb_to_16(r: u8, g: u8, b: u8) -> u8 {
    let bright = r.max(g).max(b) > 191;
    let mut index = 0u8;
    if r > 127 {
        index |= 1;
    }
    if g > 127 {
        index |= 2;
    }
    if b > 127 {
        index |= 4;
    }
    if bright {
        index | 8
    } else {
        index
    }
}

fn sgr(attr: Attr, depth: rvim_ui_api::backend::ColorDepth) -> String {"""
assert anchor in s
s = s.replace(anchor, helper, 1)
old = """        Color::Indexed(index) => {
            if depth == rvim_ui_api::backend::ColorDepth::Mono {
                return;
            }
            codes.push(format!("{};5;{}", if foreground { 38 } else { 48 }, index));
        }
        Color::Rgb(r, g, b) => {
            if depth == rvim_ui_api::backend::ColorDepth::TrueColor {
                codes.push(format!(
                    "{};2;{};{};{}",
                    if foreground { 38 } else { 48 },
                    r,
                    g,
                    b
                ));
            } else {
                let index = rgb_to_256(r, g, b);
                codes.push(format!("{};5;{}", if foreground { 38 } else { 48 }, index));
            }
        }"""
new = """        Color::Indexed(index) => {
            match depth {
                rvim_ui_api::backend::ColorDepth::Mono => {}
                rvim_ui_api::backend::ColorDepth::Ansi16 => {
                    codes.push(ansi16_code(index, foreground).to_string())
                }
                _ => codes.push(format!(
                    "{};5;{}",
                    if foreground { 38 } else { 48 },
                    index
                )),
            }
        }
        Color::Rgb(r, g, b) => match depth {
            rvim_ui_api::backend::ColorDepth::Mono => {}
            rvim_ui_api::backend::ColorDepth::TrueColor => codes.push(format!(
                "{};2;{};{};{}",
                if foreground { 38 } else { 48 },
                r,
                g,
                b
            )),
            rvim_ui_api::backend::ColorDepth::Ansi256 => {
                let index = rgb_to_256(r, g, b);
                codes.push(format!(
                    "{};5;{}",
                    if foreground { 38 } else { 48 },
                    index
                ));
            }
            rvim_ui_api::backend::ColorDepth::Ansi16 => {
                codes.push(ansi16_code(rgb_to_16(r, g, b), foreground).to_string())
            }
        },"""
assert old in s
s = s.replace(old, new, 1)
# tests
s += '''

#[cfg(test)]
mod color_tests {
    use super::*;
    use rvim_ui_api::backend::ColorDepth;

    #[test]
    fn ansi16_color_fallback() {
        let attr = Attr {
            fg: Color::Indexed(1),
            ..Attr::DEFAULT
        };
        let code = sgr(attr, ColorDepth::Ansi16);
        assert!(code.contains("31"), "{code}");
        let mono = sgr(attr, ColorDepth::Mono);
        assert!(!mono.contains("38;"), "{mono}");
    }
}
'''
p.write_text(s)
print("patched")
