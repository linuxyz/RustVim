use rvim_regex::{compile, Magic};
use rvim_syntax::SyntaxEngine;

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn random_text(state: &mut u64, length: usize) -> String {
    const ALPHABET: &[u8] = b"abcXYZ0123 /\\\\^$.*+?()[]{}|\\t-_ ";
    let mut text = String::new();
    for _ in 0..length {
        let index = (next(state) as usize) % ALPHABET.len();
        text.push(ALPHABET[index] as char);
    }
    text
}

#[test]
fn randomized_regex_and_syntax_inputs_do_not_panic() {
    let mut state = 0x1234_5678_9abc_def0;
    for _ in 0..80 {
        let pattern = random_text(&mut state, 20);
        let line = random_text(&mut state, 24);
        let _ = compile(pattern.as_bytes(), Magic::Magic, false)
            .map(|regex| regex.find(line.as_bytes()));
    }

    let mut engine = SyntaxEngine::new();
    engine.set_enabled(true);
    for _ in 0..40 {
        let word = random_text(&mut state, 6);
        let _ = engine.define(&format!("keyword Group {word}"));
        let _ = engine.define(&format!("match Match /{}/", random_text(&mut state, 6)));
        let _ = engine.define(&format!(
            "region Region start=/{}/ end=/{}/ contains=Group",
            random_text(&mut state, 4),
            random_text(&mut state, 4)
        ));
        let line = random_text(&mut state, 20);
        let _ = engine.highlight_line(line.as_bytes());
    }
}
