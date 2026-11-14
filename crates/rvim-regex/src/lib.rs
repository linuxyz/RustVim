//! Vim-compatible, byte-oriented regular expressions for RVim.
//!
//! The engine follows Vim's pattern language: magic modes, Vim's escapes,
//! leftmost-first backtracking, capture groups, back references, look-around,
//! \zs/\ze and the usual Vim character-class atoms.  Patterns and haystacks are
//! arbitrary byte slices; the engine never assumes valid UTF-8.
//!
//! # Public API
//!
//!     use rvim_regex::{compile, find_all, Magic};
//!
//!     let regex = compile(b"\\v<(foo|bar)>", Magic::Magic, false).unwrap();
//!     assert_eq!(regex.find(b"a foo!").unwrap().start, 2);
//!     assert_eq!(find_all(&regex, b"foo bar").len(), 2);
//!
//! # Matching model
//!
//! * Search is leftmost-first.  At each start offset alternatives are tried in
//!   Vim's backtracking order: earlier branches first, greedy before lazy.
//! * A haystack newline byte (0x0A) is the line break.  A dot and normal
//!   character classes do not match it; \n, \_. and \_ class variants do.
//!   Negated collections such as \[^a] match the break too, which is Vim's
//!   documented Vi-compatibility quirk.
//! * ^ and $ are line anchors in magic/nomagic mode subject to Vim's position
//!   rules, and always anchors in very magic mode.  In very nomagic mode the
//!   bare characters are literal and \^ / \$ anchor.
//! * Case folding is ASCII-only.  \c anywhere outside a collection forces
//!   case-insensitive matching for the whole pattern; \C forces case sensitive
//!   matching, but a \c wins if both are present, as in Vim.  \l, \u, \L and
//!   \U are not affected by case folding; collections and POSIX \[:lower:] /
//!   \[:upper:] classes are.
//! * Captures in Match::captures are group 1 through N (group 0 is the overall
//!   start/end), each None if it did not participate.
//! * Empty matches are allowed by find and find_at; find_all advances one byte
//!   after every empty match and can never loop.
//!
//! # Step budget
//!
//! The backtracking VM is bounded by a per-call step budget (default
//! DEFAULT_STEP_LIMIT).  find_at returns None when the budget is exhausted; use
//! Regex::find_at_result or Regex::find_all_result to distinguish exhaustion
//! from "no match".
//!
//! # Supported Vim atoms
//!
//! * Literals, ., \_., ^, $, \^, \$, \_^, \_$, \%^, \%$.
//! * \<, \>.
//! * Repetition: *, \+, \?, \=, \{n,m} (and all Vim interval forms) plus lazy
//!   \{-...}.
//! * Alternation \| (or | in very magic), capturing groups \( \) (or ()) and
//!   non-capturing groups \%( \) (or %( ).
//! * \zs and \ze.
//! * Look-ahead \@=, \@!, look-behind \@<=, \@<!, atomic \@> and the
//!   very-magic bare forms.  Look-behind accepts variable length and the
//!   optional \@123<= byte limit; it is bounded to the current and previous
//!   line like Vim.
//! * Back references \1 through \9 to already opened groups.
//! * Class atoms \d \D \x \X \o \O \w \W \h \H \a \A \l \L \u \U \s \S \i
//!   \I \k \K \f \F \p \P and their \_ line-break variants.  \i, \k, \f and
//!   \p use fixed ASCII approximations because this crate has no editor
//!   option context.
//! * Collections with ranges, negation, escaped specials, \n, numeric
//!   \d \o \x \u \U bytes, POSIX \[:alpha:]-style classes and simple one-byte
//!   equivalence/collation members such as [[=a=]] and [[.a.]].
//! * Magic mode switches \v \m \M \V, including mid-pattern.
//!
//! # Known unsupported atoms
//!
//! These are rejected with a clear RegexError instead of being silently
//! miscompiled:
//!
//! * \& conjunction (and bare & in very magic).
//! * ~ and \~ "last substitute string" (there is no substitute state here).
//! * \z(, \z1..\z9 syntax highlighting groups and \Z composing-character
//!   folding.
//! * \%[ optional sequences (and bare %[ in very magic).
//! * Contextual/numeric \% atoms such as \%#, \%V, \%'m, \%l, \%c, \%v,
//!   \%d, \%x, \%o, \%u, \%U and \%C.
//!
//! Other documented limitations:
//!
//! * Only nine capturing groups (Vim's E51 limit) and \1..\9.
//! * ASCII-only case folding; bytes >= 0x80 compare exactly.
//! * Look-around captures are merged PCRE-style: positive assertions expose
//!   captures; negative ones do not.  \zs/\ze inside an assertion do not move
//!   the outer match boundaries.
//! * Captures in repetitions use conventional last-iteration semantics.  Vim
//!   has some quirky values for empty iterations of e.g. (a*)*; those quirks
//!   are not replicated.
//! * Numeric class escapes must fit in one byte; larger values are rejected
//!   because this engine is byte-oriented.
//! * Character-class options ('isident', 'iskeyword', 'isfname', 'isprint')
//!   are fixed ASCII approximations.
//! * A backslash before an unrecognized character matches that literal byte.
//!   Inside a collection the same rule applies; Vim's legacy corner case of
//!   retaining the backslash in that position is not reproduced.
//! * Equivalence and collation collection members are reduced to their single
//!   literal byte; locale-aware collation is not attempted.
//! * An unclosed collection is treated as a literal '[' (Vim's documented
//!   behavior).  Unknown POSIX class names are rejected with E1030 rather than
//!   silently matching nothing.
//! * Intervals whose minimum is greater than their maximum are rejected with
//!   E554; Vim's old engine is more permissive here.
//! * Look-behind is bounded to the current and previous line, so a
//!   variable-length look-behind cannot inspect older lines.
//! * The step budget is per search call and covers every start offset tried by
//!   find_at.  A very large haystack can therefore need a larger configured
//!   limit; callers may use Regex::with_step_limit.

mod ast;
mod charclass;
mod compiler;
mod error;
mod parser;
mod vm;

pub use error::{MatchError, RegexError};

/// Default backtracking step budget for one search call.
pub const DEFAULT_STEP_LIMIT: u64 = 1_000_000;

/// Vim magic mode supplied to compile.
///
/// Vim's default is Magic::Magic.  The mode can also be switched inside a
/// pattern with \v, \m, \M and \V.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Magic {
    /// \v: all punctuation is special; groups and alternation are bare.
    VeryMagic,
    /// \m: Vim's default magic mode.
    Magic,
    /// \M: only ^ and $ are special without a backslash.
    Nomagic,
    /// \V: bare characters are literal; backslash enables special atoms.
    VeryNomagic,
}

/// A successful match.
///
/// start and end are byte offsets into the haystack, end exclusive.
/// captures\[i] is group i + 1; unmatched groups are None.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Byte offset where the match starts.
    pub start: usize,
    /// Byte offset just after the match.
    pub end: usize,
    /// Capture groups 1..N.
    pub captures: Vec<Option<(usize, usize)>>,
}

/// A compiled Vim regular expression.
#[derive(Debug, Clone)]
pub struct Regex {
    pub(crate) programs: Vec<compiler::Program>,
    pub(crate) groups: usize,
    /// Capture slot for each syntax group declared with `\z(`.
    pub(crate) z_groups: Vec<usize>,
    pub(crate) ignorecase: bool,
    pub(crate) step_limit: u64,
}

/// Compile a Vim pattern.
///
/// magic is the initial magic mode (\v, \m, \M or \V may still switch it
/// inside the pattern).  ignorecase is the caller's 'ignorecase' decision;
/// \c and \C override it as described in the crate documentation.  SmartCase
/// is intentionally left to the caller.
pub fn compile(pattern: &[u8], magic: Magic, ignorecase: bool) -> Result<Regex, RegexError> {
    let parsed = parser::parse(pattern, magic)?;
    let programs = compiler::compile(&parsed.ast);
    Ok(Regex {
        programs,
        groups: parsed.groups,
        z_groups: parsed.z_groups,
        ignorecase: parsed.force_ignorecase.unwrap_or(ignorecase),
        step_limit: DEFAULT_STEP_LIMIT,
    })
}

impl Regex {
    /// Search from byte offset start (inclusive).
    ///
    /// Returns the leftmost match.  If allow_empty is false, zero-width
    /// matches are skipped (searching continues one byte later).  Returns
    /// None when there is no match or when the step budget runs out; use
    /// Regex::find_at_result to tell those cases apart.
    pub fn find_at(&self, haystack: &[u8], start: usize, allow_empty: bool) -> Option<Match> {
        self.find_at_result(haystack, start, allow_empty)
            .ok()
            .flatten()
    }

    /// Like find_at, but reports step-budget exhaustion.
    pub fn find_at_result(
        &self,
        haystack: &[u8],
        start: usize,
        allow_empty: bool,
    ) -> Result<Option<Match>, MatchError> {
        vm::find_at(self, haystack, start, allow_empty)
    }

    /// Search for the leftmost match in the whole haystack.
    pub fn find(&self, haystack: &[u8]) -> Option<Match> {
        self.find_at(haystack, 0, true)
    }

    /// Number of capturing groups in the pattern.
    pub fn group_count(&self) -> usize {
        self.groups
    }

    /// Capture slots of the pattern's `\z(` groups, in declaration order.
    ///
    /// The slots index into `Match::captures`.  This is used by the syntax
    /// highlighter, where `\z1` in a later region pattern refers back to the
    /// text matched by the first `\z(` group of the start pattern.
    pub fn z_group_slots(&self) -> &[usize] {
        &self.z_groups
    }

    /// Whether this compiled pattern matches ASCII case-insensitively.
    pub fn is_ignorecase(&self) -> bool {
        self.ignorecase
    }

    /// Current per-call backtracking step limit.
    pub fn step_limit(&self) -> u64 {
        self.step_limit
    }

    /// Set the per-call backtracking step limit.  A value of zero makes every
    /// search report MatchError::StepLimitExceeded.
    pub fn set_step_limit(&mut self, limit: u64) {
        self.step_limit = limit;
    }

    /// Builder-style version of Regex::set_step_limit.
    pub fn with_step_limit(mut self, limit: u64) -> Self {
        self.set_step_limit(limit);
        self
    }
}

/// Find all non-overlapping matches, advancing one byte after empty matches.
///
/// If the step budget is exhausted, the matches found so far are returned.
/// Use find_all_result when the difference matters.
pub fn find_all(regex: &Regex, haystack: &[u8]) -> Vec<Match> {
    let mut matches = Vec::new();
    let mut start = 0usize;
    while let Some(matched) = regex.find_at(haystack, start, true) {
        let next = next_search_start(&matched, start);
        matches.push(matched);
        if next > haystack.len() {
            break;
        }
        start = next;
    }
    matches
}

/// Fallible version of find_all.
pub fn find_all_result(regex: &Regex, haystack: &[u8]) -> Result<Vec<Match>, MatchError> {
    let mut matches = Vec::new();
    let mut start = 0usize;
    while let Some(matched) = regex.find_at_result(haystack, start, true)? {
        let next = next_search_start(&matched, start);
        matches.push(matched);
        if next > haystack.len() {
            break;
        }
        start = next;
    }
    Ok(matches)
}

fn next_search_start(matched: &Match, previous_start: usize) -> usize {
    if matched.start == matched.end {
        matched.end.saturating_add(1)
    } else if matched.end > previous_start {
        matched.end
    } else {
        previous_start.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_re(pattern: &str) -> Regex {
        match compile(pattern.as_bytes(), Magic::Magic, false) {
            Ok(regex) => regex,
            Err(error) => panic!("pattern {pattern:?} failed to compile: {error}"),
        }
    }

    fn find(pattern: &str, haystack: &str) -> Option<(usize, usize)> {
        compile_re(pattern)
            .find(haystack.as_bytes())
            .map(|matched| (matched.start, matched.end))
    }

    fn captures(pattern: &str, haystack: &str) -> Vec<Option<(usize, usize)>> {
        compile_re(pattern)
            .find(haystack.as_bytes())
            .expect("expected a match")
            .captures
    }

    fn all(pattern: &str, haystack: &str) -> Vec<(usize, usize)> {
        find_all(&compile_re(pattern), haystack.as_bytes())
            .into_iter()
            .map(|matched| (matched.start, matched.end))
            .collect()
    }

    fn find_magic(pattern: &str, haystack: &str, magic: Magic) -> Option<(usize, usize)> {
        compile(pattern.as_bytes(), magic, false)
            .unwrap()
            .find(haystack.as_bytes())
            .map(|matched| (matched.start, matched.end))
    }

    #[test]
    fn escaped_control_atoms() {
        assert_eq!(find("\\t", "a\tb"), Some((1, 2)));
        assert_eq!(find("\\r", "a\rb"), Some((1, 2)));
        assert_eq!(find("\\e", "a\u{1b}b"), Some((1, 2)));
        assert_eq!(find("\\b", "a\u{08}b"), Some((1, 2)));
        assert_eq!(find("\\n", "a\nb"), Some((1, 2)));
        assert_eq!(find("\\b", "a b"), None);
    }

    #[test]
    fn anchors_follow_magic_position_rules() {
        assert_eq!(find("a^b", "a^b"), Some((0, 3)));
        assert_eq!(find("a$b", "a$b"), Some((0, 3)));
        assert_eq!(find("^(a)$", "(a)"), Some((0, 3)));
        assert_eq!(find("\\(^a\\)", "a"), Some((0, 1)));
        assert_eq!(find("a\\n^b", "a\nb"), Some((0, 3)));
        assert_eq!(find("a$\\nb", "a\nb"), Some((0, 3)));
        assert_eq!(find("a$\\|b", "a"), Some((0, 1)));
        assert_eq!(find("x\\|^a", "a"), Some((0, 1)));
        assert_eq!(find("\\(a$\\)", "a"), Some((0, 1)));
        assert_eq!(find_magic("a$b", "a$b", Magic::VeryMagic), None);
        assert_eq!(find_magic("a$b", "a$b", Magic::VeryNomagic), Some((0, 3)));
        assert_eq!(find_magic("a\\$", "a", Magic::VeryNomagic), Some((0, 1)));
        assert_eq!(find_magic("a^b", "a^b", Magic::VeryMagic), None);
        assert_eq!(find_magic("a^b", "a^b", Magic::VeryNomagic), Some((0, 3)));
        assert_eq!(find_magic("a\\^b", "a^b", Magic::VeryNomagic), None);
    }

    #[test]
    fn star_literal_contexts() {
        assert_eq!(find("^*", "*"), Some((0, 1)));
        assert_eq!(find("a^*", "a^^^"), Some((0, 4)));
        assert_eq!(find_magic("^*", "*", Magic::Nomagic), Some((0, 1)));
        assert_eq!(find("a.b", "axb"), Some((0, 3)));
        assert_eq!(find_magic("*", "*", Magic::Magic), Some((0, 1)));
    }

    #[test]
    fn class_newline_rules() {
        assert_eq!(find("[\\n]", "\n"), Some((0, 1)));
        assert_eq!(find("[\\x0a]", "\n"), Some((0, 1)));
        assert_eq!(find("[[:space:]]", "\n"), None);
        assert_eq!(find("\\_[[:space:]]", "\n"), Some((0, 1)));
        assert_eq!(find("[^\\n]", "\n"), Some((0, 1)));
        assert_eq!(find("[a]", "\n"), None);
        assert_eq!(find("\\_s\\+", "a\n\nb"), Some((1, 3)));
        assert_eq!(find("\\_[ab]", "\n"), Some((0, 1)));
        assert_eq!(find("\\_[ab]", "c"), None);
    }

    #[test]
    fn unclosed_collection_is_literal() {
        assert_eq!(find("[abc", "[abc"), Some((0, 4)));
        assert_eq!(find("[", "["), Some((0, 1)));
        assert_eq!(find("[]", "[]"), Some((0, 2)));
        assert_eq!(find("[^abc", "[^abc"), Some((0, 5)));
    }

    #[test]
    fn class_first_character_and_negation() {
        assert_eq!(find("[^]a]", "b"), Some((0, 1)));
        assert_eq!(find("[^]a]", "]"), None);
        assert_eq!(find("[^]]", "a"), Some((0, 1)));
        assert_eq!(find("[^]]", "]"), None);
        assert_eq!(find("[a\\]b]", "]"), Some((0, 1)));
        assert_eq!(find("[\\^a]", "^"), Some((0, 1)));
        assert_eq!(find("[\\-]", "-"), Some((0, 1)));
    }

    #[test]
    fn numeric_class_escapes_and_limits() {
        assert_eq!(find("[\\x41]", "A"), Some((0, 1)));
        assert_eq!(find("[\\u0041]", "A"), Some((0, 1)));
        assert_eq!(find("[\\U00000041]", "A"), Some((0, 1)));
        assert_eq!(find("[\\e]", "\u{1b}"), Some((0, 1)));
        // Multi-byte code points in a collection are accepted; the byte engine
        // approximates them by their UTF-8 bytes (see the crate docs).
        assert!(compile(b"[\\u0100]", Magic::Magic, false).is_ok());
    }

    #[test]
    fn equivalence_and_collation_classes() {
        assert_eq!(find("[[=a=]]", "a"), Some((0, 1)));
        assert_eq!(find("[[.a.]]", "a"), Some((0, 1)));
        assert_eq!(find("[[===]]", "="), Some((0, 1)));
    }

    #[test]
    fn identifier_and_filename_class_atoms() {
        assert_eq!(find("\\i\\+", "_ab12"), Some((0, 5)));
        assert_eq!(find("\\I\\+", "_ab12"), Some((0, 3)));
        assert_eq!(find("\\k\\+", "_ab12"), Some((0, 5)));
        assert_eq!(find("\\K\\+", "_ab12"), Some((0, 3)));
        assert_eq!(find("\\f\\+", "a/b-c"), Some((0, 5)));
        assert_eq!(find("\\F\\+", "a/b1"), Some((0, 3)));
        assert_eq!(find("\\p\\+", "ab cd"), Some((0, 5)));
        assert_eq!(find("\\P\\+", "ab12"), Some((0, 2)));
    }

    #[test]
    fn very_magic_escaped_punctuation_is_literal() {
        let very = |pattern: &str, text: &str| {
            compile(pattern.as_bytes(), Magic::VeryMagic, false)
                .unwrap()
                .find(text.as_bytes())
                .map(|matched| (matched.start, matched.end))
        };
        assert_eq!(very("\\%", "%"), Some((0, 1)));
        assert_eq!(very("\\&", "&"), Some((0, 1)));
        assert_eq!(very("\\~", "~"), Some((0, 1)));
        assert_eq!(very("\\@", "@"), Some((0, 1)));
        assert_eq!(very("\\{", "{"), Some((0, 1)));
        assert_eq!(very("\\*", "*"), Some((0, 1)));
        assert_eq!(very("\\(", "("), Some((0, 1)));
        assert_eq!(very("\\|", "|"), Some((0, 1)));
    }

    #[test]
    fn tilde_and_ampersand_mode_rules() {
        assert_eq!(find("\\~", "~"), Some((0, 1)));
        assert!(compile(b"~", Magic::Magic, false).is_err());
        assert_eq!(find_magic("~", "~", Magic::Nomagic), Some((0, 1)));
        assert_eq!(find_magic("~", "~", Magic::VeryNomagic), Some((0, 1)));
        assert!(compile(b"\\~", Magic::Nomagic, false).is_err());
        // \& is supported as a conjunction; in very nomagic it is literal.
        assert!(compile(b"\\&", Magic::Nomagic, false).is_ok());
        assert!(compile(b"\\&", Magic::VeryNomagic, false).is_ok());
        assert_eq!(find_magic("&", "&", Magic::Magic), Some((0, 1)));
    }

    #[test]
    fn magic_switch_inside_groups() {
        let regex = compile(b"\\v(a)\\m\\(b\\)", Magic::Magic, false).unwrap();
        let matched = regex.find(b"ab").unwrap();
        assert_eq!((matched.start, matched.end), (0, 2));
        assert_eq!(matched.captures, vec![Some((0, 1)), Some((1, 2))]);
        let regex = compile(b"\\V\\(a\\)\\v(b)", Magic::Magic, false).unwrap();
        let matched = regex.find(b"ab").unwrap();
        assert_eq!((matched.start, matched.end), (0, 2));
        assert_eq!(matched.captures, vec![Some((0, 1)), Some((1, 2))]);
    }

    #[test]
    fn very_nomagic_quantifiers_and_groups() {
        assert_eq!(find_magic("a*", "a*", Magic::VeryNomagic), Some((0, 2)));
        assert_eq!(find_magic("a\\*", "aaa", Magic::VeryNomagic), Some((0, 3)));
        assert_eq!(
            find_magic("a\\{2}", "aaa", Magic::VeryNomagic),
            Some((0, 2))
        );
        assert_eq!(
            find_magic("\\(a\\|b\\)\\+", "ab", Magic::VeryNomagic),
            Some((0, 2))
        );
    }

    #[test]
    fn very_magic_lookaround_and_atomic() {
        let very = |pattern: &str, text: &str| {
            compile(pattern.as_bytes(), Magic::VeryMagic, false)
                .unwrap()
                .find(text.as_bytes())
        };
        let matched = very("(a*)@>b", "aab").unwrap();
        assert_eq!((matched.start, matched.end), (0, 3));
        assert_eq!(matched.captures[0], Some((0, 2)));
        let matched = very("(a*)@=a", "aaa").unwrap();
        assert_eq!((matched.start, matched.end), (0, 1));
        assert_eq!(matched.captures[0], Some((0, 3)));
        assert_eq!(
            very("(ab)@2<=c", "abc").map(|matched| (matched.start, matched.end)),
            Some((2, 3))
        );
        assert_eq!(
            very("(ab)@1<=c", "abc").map(|matched| (matched.start, matched.end)),
            None
        );
    }

    #[test]
    fn empty_negative_lookahead_find_all() {
        assert_eq!(
            all("\\(a\\)\\@!", "bbb"),
            vec![(0, 0), (1, 1), (2, 2), (3, 3)]
        );
        assert_eq!(all("\\ze", "abc"), vec![(0, 0), (1, 1), (2, 2), (3, 3)]);
    }

    #[test]
    fn find_all_is_non_overlapping() {
        assert_eq!(all("aa", "aaaa"), vec![(0, 2), (2, 4)]);
        assert_eq!(all("a*", "aaa"), vec![(0, 3), (3, 3)]);
    }

    #[test]
    fn find_all_result_reports_limit() {
        let mut regex = compile_re("a");
        regex.set_step_limit(1);
        assert!(find_all(&regex, b"a").is_empty());
        assert_eq!(
            find_all_result(&regex, b"a"),
            Err(MatchError::StepLimitExceeded { limit: 1 })
        );
    }

    #[test]
    fn find_at_honours_start_offset() {
        let regex = compile_re("a");
        assert_eq!(
            regex.find_at(b"aba", 1, true).map(|m| (m.start, m.end)),
            Some((2, 3))
        );
        assert_eq!(
            regex.find_at(b"aba", 2, true).map(|m| (m.start, m.end)),
            Some((2, 3))
        );
        assert_eq!(regex.find_at(b"aba", 3, true), None);
    }

    #[test]
    fn atomic_inside_repetition_preserves_frames() {
        let regex = compile_re("\\(\\(a\\)\\@>\\)*");
        let matched = regex.find(b"aa").unwrap();
        assert_eq!((matched.start, matched.end), (0, 2));
        assert_eq!(matched.captures, vec![Some((1, 2)), Some((1, 2))]);
    }

    #[test]
    fn backreference_to_unmatched_group_fails() {
        assert_eq!(find("\\(a\\)\\|\\(b\\)\\2", "b"), None);
        assert_eq!(find("\\(a\\)\\|\\(b\\)\\2", "bb"), Some((0, 2)));
        assert_eq!(find("\\(a\\)\\|\\1", "b"), None);
        assert_eq!(find("\\(a\\)\\|\\1", "a"), Some((0, 1)));
    }

    #[test]
    fn alternation_priority_and_greedy_backoff() {
        assert_eq!(find("a\\|ab", "ab"), Some((0, 1)));
        assert_eq!(find("ab\\|a", "ab"), Some((0, 2)));
        assert_eq!(find("a*ab", "aaab"), Some((0, 4)));
    }

    #[test]
    fn c_flag_inside_and_after_collections() {
        let regex = compile_re("[\\c]");
        assert!(regex.find(b"c").is_some());
        assert!(regex.find(b"C").is_none());
        let regex = compile_re("[a]\\c");
        assert!(regex.find(b"A").is_some());
        let regex = compile(b"[a]\\C", Magic::Magic, true).unwrap();
        assert!(regex.find(b"A").is_none());
    }

    #[test]
    fn non_ascii_bytes_are_not_case_folded() {
        let regex = compile(&[0xc0], Magic::Magic, true).unwrap();
        assert!(regex.find(&[0xc0]).is_some());
        assert!(regex.find(&[0xe0]).is_none());
    }

    #[test]
    fn accessors_and_error_display() {
        let regex = compile(b"\\c\\(a\\)\\(b\\)", Magic::Magic, false).unwrap();
        assert_eq!(regex.group_count(), 2);
        assert!(regex.is_ignorecase());
        assert_eq!(
            MatchError::StepLimitExceeded { limit: 5 }.to_string(),
            "regex step limit of 5 exceeded"
        );
        let error = compile(b"\\(", Magic::Magic, false).unwrap_err();
        assert!(error.to_string().contains("byte 0"));
        assert!(error.to_string().contains("E54"));
    }

    #[test]
    fn too_many_capturing_groups() {
        let pattern = "\\(a\\)".repeat(10);
        let error = compile(pattern.as_bytes(), Magic::Magic, false).unwrap_err();
        assert!(error.message.contains("E51"));
        assert_eq!(error.offset, 45);
    }

    #[test]
    fn nested_multis_are_rejected() {
        for pattern in ["a**", "a\\+\\+", "a\\?\\?", "a\\@=*", "a*\\@="] {
            assert!(
                compile(pattern.as_bytes(), Magic::Magic, false).is_err(),
                "expected {pattern:?} to be rejected"
            );
        }
        for (pattern, magic) in [
            ("\\+", Magic::Magic),
            ("\\*", Magic::Nomagic),
            ("\\*", Magic::VeryNomagic),
            ("\\{2}", Magic::Magic),
            ("\\@=", Magic::Magic),
        ] {
            assert!(
                compile(pattern.as_bytes(), magic, false).is_err(),
                "expected {pattern:?} / {magic:?} to be rejected"
            );
        }
    }

    #[test]
    fn very_magic_requires_escaping_invalid_percent_and_at() {
        assert!(compile(b"%", Magic::VeryMagic, false).is_err());
        assert!(compile(b"@", Magic::VeryMagic, false).is_err());
        assert!(compile(b"@=", Magic::VeryMagic, false).is_err());
        assert!(compile(b"~", Magic::VeryMagic, false).is_err());
        assert!(compile(b"\\%", Magic::VeryMagic, false).is_ok());
        assert!(compile(b"\\@", Magic::VeryMagic, false).is_ok());
        assert!(compile(b"\\~", Magic::VeryMagic, false).is_ok());
    }

    #[test]
    fn empty_repetitions_terminate() {
        let regex = compile_re("\\(a*\\)*");
        assert!(regex.find(b"aaaaaaaaaaaaaaaaaaaaaaaa").is_some());
        let regex = compile_re("\\(\\)*");
        assert_eq!(regex.find(b"abc").map(|m| (m.start, m.end)), Some((0, 0)));
        let regex = compile_re("\\(a*\\)\\{2,4}");
        assert!(regex.find(b"aaaa").is_some());
    }

    #[test]
    fn lookbehind_is_bounded_to_two_lines() {
        assert_eq!(find("\\(a\\)\\@<=b", "a\nb"), None);
        assert_eq!(find("\\(a\\)\\@<=b", "ab"), Some((1, 2)));
    }

    #[test]
    fn step_limit_stops_catastrophic_backtracking() {
        let mut regex = compile_re("\\(a\\|aa\\)*b");
        regex.set_step_limit(5_000);
        let haystack = vec![b'a'; 40];
        assert_eq!(
            regex.find_at_result(&haystack, 0, true),
            Err(MatchError::StepLimitExceeded { limit: 5_000 })
        );
        regex.set_step_limit(0);
        assert_eq!(
            regex.find_at_result(b"a", 0, true),
            Err(MatchError::StepLimitExceeded { limit: 0 })
        );
    }

    #[test]
    fn malformed_lookaround_syntax_errors() {
        for pattern in ["\\@", "\\@x", "\\@<", "\\@123<", "\\@1"] {
            let error = compile(pattern.as_bytes(), Magic::Magic, false).unwrap_err();
            assert!(
                error.message.contains("E59"),
                "pattern {pattern:?}: {error}"
            );
        }
        let error = compile(b"a\\{2,1}", Magic::Magic, false).unwrap_err();
        assert!(error.message.contains("E554"));
    }

    #[test]
    fn multiline_underscore_dot() {
        assert_eq!(find("a\\_.*b", "a\nx\nb"), Some((0, 5)));
        assert_eq!(find("a\\_.x", "a\nx"), Some((0, 3)));
        assert_eq!(find("\\_.", "\n"), Some((0, 1)));
        assert_eq!(find("\\_.*", "a\nb"), Some((0, 3)));
    }

    #[test]
    fn empty_haystack_behavior() {
        assert_eq!(find("", ""), Some((0, 0)));
        assert_eq!(all("", ""), vec![(0, 0)]);
        assert!(find("a", "").is_none());
        assert!(all("a", "").is_empty());
        let regex = compile_re("a");
        assert_eq!(find_all_result(&regex, b"aba").unwrap().len(), 2);
    }

    #[test]
    fn zero_width_lookahead_over_empty_haystack() {
        assert_eq!(find("\\(a\\)\\@!", ""), Some((0, 0)));
        assert_eq!(find("\\(a*\\)\\@!", "a"), None);
    }

    #[test]
    fn nul_bytes_are_literal() {
        let regex = compile(b"a\x00b", Magic::Magic, false).unwrap();
        let matched = regex.find(b"xa\x00b").unwrap();
        assert_eq!((matched.start, matched.end), (1, 4));
        let regex = compile(&[b'[', 0x00, b']'], Magic::Magic, false).unwrap();
        assert!(regex.find(&[0x00]).is_some());
    }

    #[test]
    fn empty_group_captures() {
        assert_eq!(captures("\\(\\)", "x"), vec![Some((0, 0))]);
        assert_eq!(captures("\\(a*\\)", "b"), vec![Some((0, 0))]);
        assert_eq!(captures("\\(a*\\)*", "b"), vec![Some((0, 0))]);
    }

    #[test]
    fn vim_parity_span_table() {
        let cases = &[
            ("abc", "xxabcxx", Some((2, 5))),
            ("a*", "baa", Some((0, 0))),
            ("a\\+", "baaa", Some((1, 4))),
            ("a\\?", "b", Some((0, 0))),
            ("ab*c", "abbbc", Some((0, 5))),
            ("a\\{2,3}b", "aaaab", Some((1, 5))),
            ("a\\{-2,3}b", "aaaab", Some((1, 5))),
            ("\\(foo\\|bar\\)", "xxbar", Some((2, 5))),
            ("\\(ab\\)\\{2}", "abab", Some((0, 4))),
            ("\\(a\\)\\1", "abcaa", Some((3, 5))),
            ("foo\\(bar\\)\\@=", "foobar", Some((0, 3))),
            ("foo\\(bar\\)\\@!", "foobaz", Some((0, 3))),
            ("\\(foo\\)\\@<=bar", "foobar", Some((3, 6))),
            ("\\(a*\\)\\@<=X", "aaaX", Some((3, 4))),
            ("\\(a*\\)\\@>b", "aab", Some((0, 3))),
            ("\\<foo\\>", "a foo b", Some((2, 5))),
            ("\\s*\\zsif", "   if x", Some((3, 5))),
            ("\\ze..\\zs", "abc", Some((2, 2))),
            ("[a-c0-9]", "zz5", Some((2, 3))),
            ("[^abc]", "abcx", Some((3, 4))),
            ("[[:alpha:]]\\+", "123abc", Some((3, 6))),
            ("\\d\\+", "abc123", Some((3, 6))),
            ("\\w\\+", "!abc_12", Some((1, 7))),
            ("\\x\\+", "zz1aF", Some((2, 5))),
            ("a\\cb\\C", "AB", Some((0, 2))),
            ("\\v(a|b)+", "ab", Some((0, 2))),
            ("\\va{-}b", "aaab", Some((0, 4))),
            ("\\v(a)@<=a", "aa", Some((1, 2))),
            ("\\Va.b", "a.b", Some((0, 3))),
            ("\\Va\\.b", "axb", Some((0, 3))),
            ("\\_^b", "a\nb", Some((2, 3))),
            ("a\\_$", "a\nb", Some((0, 1))),
            ("\\%^a", "abc", Some((0, 1))),
            ("c\\%$", "abc", Some((2, 3))),
            ("\\p\\+", "ab cd", Some((0, 5))),
            ("\\I\\+", "_ab12", Some((0, 3))),
        ];
        for (pattern, haystack, expected) in cases {
            assert_eq!(
                find(pattern, haystack),
                *expected,
                "pattern {pattern:?} on {haystack:?}"
            );
        }
    }

    #[test]
    fn literal_matching() {
        assert_eq!(find("abc", "xxabcxx"), Some((2, 5)));
        assert_eq!(find("abc", "ab"), None);
        assert_eq!(find("", "abc"), Some((0, 0)));
    }

    #[test]
    fn invalid_utf8_is_literal_bytes() {
        let regex = compile(&[0xff, 0xfe], Magic::Magic, false).unwrap();
        let matched = regex.find(&[0x00, 0xff, 0xfe]).unwrap();
        assert_eq!((matched.start, matched.end), (1, 3));
    }

    #[test]
    fn dot_excludes_newline_and_underscore_dot_includes_it() {
        assert_eq!(find("a.b", "a\nb"), None);
        assert_eq!(find("a.b", "axb"), Some((0, 3)));
        assert_eq!(find("a\\_.b", "a\nb"), Some((0, 3)));
        assert_eq!(find("a\\nb", "a\nb"), Some((0, 3)));
    }

    #[test]
    fn line_anchors() {
        assert_eq!(find("^b", "a\nb"), Some((2, 3)));
        assert_eq!(find("b$", "b\na"), Some((0, 1)));
        assert_eq!(find("\\_^b", "a\nb"), Some((2, 3)));
        assert_eq!(find("a\\_$", "a\nb"), Some((0, 1)));
        assert_eq!(find("\\%^a", "abc"), Some((0, 1)));
        assert_eq!(find("c\\%$", "abc"), Some((2, 3)));
    }

    #[test]
    fn word_boundaries() {
        assert_eq!(find("\\<foo\\>", "a foo b"), Some((2, 5)));
        assert_eq!(find("\\<foo\\>", "foobar"), None);
        assert_eq!(find("\\<foo", "xfoo"), None);
        assert_eq!(find("foo\\>", "foo_"), None);
        assert_eq!(find("\\>", "foo"), Some((3, 3)));
    }

    #[test]
    fn zero_width_set_start_and_end() {
        assert_eq!(find("\\s*\\zsif", "   if x"), Some((3, 5)));
        assert_eq!(find("a\\zs", "abc"), Some((1, 1)));
        assert_eq!(find("a\\ze", "abc"), Some((0, 1)));
        assert_eq!(find("\\ze..\\zs", "abc"), Some((2, 2)));
    }

    #[test]
    fn alternation_and_groups() {
        assert_eq!(find("\\(foo\\|bar\\)", "xxbar"), Some((2, 5)));
        assert_eq!(find("a\\|b", "b"), Some((0, 1)));
        assert_eq!(captures("\\(a\\|b\\)c", "bc"), vec![Some((0, 1))]);
        assert_eq!(captures("a\\(b\\)c", "abc"), vec![Some((1, 2))]);
        assert_eq!(captures("a\\(b\\)\\(c\\)", "abc").len(), 2);
    }

    #[test]
    fn non_capturing_group() {
        let regex = compile_re("\\%(ab\\|cd\\)\\(e\\)");
        let matched = regex.find(b"cde").unwrap();
        assert_eq!(matched.captures.len(), 1);
        assert_eq!(matched.captures[0], Some((2, 3)));
    }

    #[test]
    fn unmatched_groups_are_none() {
        assert_eq!(
            captures("\\(a\\)\\(b\\)\\?\\(c\\)", "ac"),
            vec![Some((0, 1)), None, Some((1, 2))]
        );
    }

    #[test]
    fn backreferences() {
        assert_eq!(find("\\([a-z]\\)\\1", "abcaa"), Some((3, 5)));
        assert_eq!(find("\\(a\\)\\1", "aA"), None);
        assert_eq!(captures("\\(ab\\)\\1", "abab"), vec![Some((0, 2))]);
        let error = compile(b"\\1\\(a\\)", Magic::Magic, false).unwrap_err();
        assert_eq!(error.offset, 0);
        assert!(error.message.contains("E65"));
    }

    #[test]
    fn lookahead() {
        let matched = compile_re("foo\\(bar\\)\\@=").find(b"foobar").unwrap();
        assert_eq!((matched.start, matched.end), (0, 3));
        assert_eq!(matched.captures[0], Some((3, 6)));
        assert_eq!(find("foo\\(bar\\)\\@=", "foobaz"), None);
        assert_eq!(find("foo\\(bar\\)\\@!", "foobaz"), Some((0, 3)));
        assert_eq!(find("foo\\(bar\\)\\@!", "foobar"), None);
        assert_eq!(captures("foo\\(bar\\)\\@!", "foobaz"), vec![None]);
    }

    #[test]
    fn lookbehind() {
        let matched = compile_re("\\(foo\\)\\@<=bar").find(b"foobar").unwrap();
        assert_eq!((matched.start, matched.end), (3, 6));
        assert_eq!(matched.captures[0], Some((0, 3)));
        assert_eq!(find("\\(foo\\)\\@<!bar", "foobar"), None);
        assert_eq!(find("\\(foo\\)\\@<!bar", "bazbar"), Some((3, 6)));
        assert_eq!(find("\\(a*\\)\\@<=X", "aaaX"), Some((3, 4)));
        assert_eq!(captures("\\(a*\\)\\@<=X", "aaaX"), vec![Some((3, 3))]);
        assert_eq!(captures("\\(a\\+\\)\\@<=X", "aaaX"), vec![Some((2, 3))]);
    }

    #[test]
    fn limited_lookbehind() {
        assert_eq!(find("\\(ab\\)\\@2<=c", "abc"), Some((2, 3)));
        assert_eq!(find("\\(ab\\)\\@1<=c", "abc"), None);
        assert_eq!(find("\\(ab\\)\\@2<!c", "abc"), None);
        assert_eq!(find("\\(ab\\)\\@2<!c", "xbc"), Some((2, 3)));
    }

    #[test]
    fn atomic_group() {
        assert_eq!(find("\\(a*\\)\\@>a", "aa"), None);
        let matched = compile_re("\\(a*\\)\\@>b").find(b"aab").unwrap();
        assert_eq!((matched.start, matched.end), (0, 3));
        assert_eq!(matched.captures[0], Some((0, 2)));
    }

    #[test]
    fn quantifiers() {
        assert_eq!(find("ab*c", "ac"), Some((0, 2)));
        assert_eq!(find("ab*c", "abbbc"), Some((0, 5)));
        assert_eq!(find("ab\\+c", "ac"), None);
        assert_eq!(find("ab\\+c", "abc"), Some((0, 3)));
        assert_eq!(find("ab\\?c", "ac"), Some((0, 2)));
        assert_eq!(find("ab\\=c", "abc"), Some((0, 3)));
        assert_eq!(find("a\\{2}b", "aab"), Some((0, 3)));
        assert_eq!(find("a\\{2,3}b", "aaaab"), Some((1, 5)));
        assert_eq!(find("a\\{2,}b", "aaaab"), Some((0, 5)));
        assert_eq!(find("a\\{,2}b", "aaab"), Some((1, 4)));
        assert_eq!(find("a\\{}b", "aaab"), Some((0, 4)));
    }

    #[test]
    fn lazy_quantifiers() {
        assert_eq!(find("a\\{-}b", "xaaab"), Some((1, 5)));
        assert_eq!(find("a\\{-2,3}b", "aaaab"), Some((1, 5)));
        assert_eq!(find("a\\{-}b", "aaa"), None);
        assert_eq!(find("a[bc]\\{-}[cd]", "abcd"), Some((0, 3)));
        assert_eq!(find("a[bc]*[cd]", "abcd"), Some((0, 4)));
    }

    #[test]
    fn groups_in_repetitions_keep_last_iteration() {
        assert_eq!(captures("\\(ab\\)\\{2}", "abab"), vec![Some((2, 4))]);
        assert_eq!(captures("\\(a\\)*b", "b"), vec![None]);
        assert_eq!(captures("\\(a*\\)*", "aaa"), vec![Some((3, 3))]);
    }

    #[test]
    fn character_collections() {
        assert_eq!(find("[abc]", "xby"), Some((1, 2)));
        assert_eq!(find("[^abc]", "abcx"), Some((3, 4)));
        assert_eq!(find("[a-c0-9]", "zz5"), Some((2, 3)));
        assert_eq!(find("[]]", "]"), Some((0, 1)));
        assert_eq!(find("[a\\]b]", "]"), Some((0, 1)));
        assert_eq!(find("[a-]", "-"), Some((0, 1)));
        assert_eq!(find("[-a]", "-"), Some((0, 1)));
        assert_eq!(find("[^-a]", "-"), None);
        assert_eq!(find("[\\x41]", "A"), Some((0, 1)));
        assert_eq!(find("[\\o101]", "A"), Some((0, 1)));
        assert_eq!(find("[\\d65]", "A"), Some((0, 1)));
    }

    #[test]
    fn posix_collections() {
        assert_eq!(find("[[:alpha:]]\\+", "123abc"), Some((3, 6)));
        assert_eq!(find("[[:digit:]]\\+", "ab12"), Some((2, 4)));
        assert_eq!(find("[[:space:]]", "a b"), Some((1, 2)));
        assert_eq!(find("[[:xdigit:]]\\+", "zzA9"), Some((2, 4)));
        assert_eq!(find("[[:punct:]]", "a!"), Some((1, 2)));
        let error = compile(b"[[:nope:]]", Magic::Magic, false).unwrap_err();
        assert!(error.message.contains("Unknown character class"));
    }

    #[test]
    fn class_atoms() {
        assert_eq!(find("\\d\\+", "abc123"), Some((3, 6)));
        assert_eq!(find("\\D\\+", "123abc"), Some((3, 6)));
        assert_eq!(find("\\w\\+", "!abc_12"), Some((1, 7)));
        assert_eq!(find("\\W", "abc!"), Some((3, 4)));
        assert_eq!(find("\\s", "a\tb"), Some((1, 2)));
        assert_eq!(find("\\S", " \ta"), Some((2, 3)));
        assert_eq!(find("\\l\\+", "12abc"), Some((2, 5)));
        assert_eq!(find("\\u\\+", "12ABC"), Some((2, 5)));
        assert_eq!(find("\\x\\+", "zz1aF"), Some((2, 5)));
        assert_eq!(find("\\o\\+", "zz17"), Some((2, 4)));
        assert_eq!(find("\\h\\+", "1abc"), Some((1, 4)));
        assert_eq!(find("\\a\\+", "1abc"), Some((1, 4)));
    }

    #[test]
    fn negated_collection_matches_newline() {
        assert_eq!(find("[^a]", "a\n"), Some((1, 2)));
        assert_eq!(find("[^\\n]", "\n"), Some((0, 1)));
        assert_eq!(find("[a]", "\n"), None);
        assert_eq!(find("\\_s", "\n"), Some((0, 1)));
        assert_eq!(find("\\S", "\n"), None);
        assert_eq!(find("\\_S", "\n"), Some((0, 1)));
    }

    #[test]
    fn case_sensitivity_and_c_flags() {
        let sensitive = compile(b"abc", Magic::Magic, false).unwrap();
        assert!(sensitive.find(b"ABC").is_none());
        let insensitive = compile(b"abc", Magic::Magic, true).unwrap();
        assert_eq!(insensitive.find(b"ABC").unwrap().start, 0);
        let forced = compile(b"\\cabc", Magic::Magic, false).unwrap();
        assert_eq!(forced.find(b"ABC").unwrap().start, 0);
        let forced_off = compile(b"\\Cabc", Magic::Magic, true).unwrap();
        assert!(forced_off.find(b"ABC").is_none());
        let both = compile(b"a\\cb\\C", Magic::Magic, false).unwrap();
        assert_eq!(both.find(b"AB").unwrap().start, 0);
        let both2 = compile(b"a\\Cb\\c", Magic::Magic, false).unwrap();
        assert_eq!(both2.find(b"AB").unwrap().start, 0);
    }

    #[test]
    fn case_folding_in_collections() {
        let regex = compile(b"[a]", Magic::Magic, true).unwrap();
        assert!(regex.find(b"A").is_some());
        let regex = compile(b"[^a]", Magic::Magic, true).unwrap();
        assert!(regex.find(b"A").is_none());
        let regex = compile(b"[[:lower:]]", Magic::Magic, true).unwrap();
        assert!(regex.find(b"A").is_some());
        let regex = compile(b"[[:upper:]]", Magic::Magic, true).unwrap();
        assert!(regex.find(b"a").is_some());
        let regex = compile(b"\\l", Magic::Magic, true).unwrap();
        assert!(regex.find(b"A").is_none());
        let regex = compile(b"\\u", Magic::Magic, true).unwrap();
        assert!(regex.find(b"a").is_none());
        let regex = compile(b"\\L", Magic::Magic, true).unwrap();
        assert!(regex.find(b"A").is_some());
        let regex = compile(b"\\U", Magic::Magic, true).unwrap();
        assert!(regex.find(b"a").is_some());
    }

    #[test]
    fn case_folded_backreference() {
        let regex = compile(b"\\(a\\)\\1", Magic::Magic, true).unwrap();
        assert!(regex.find(b"aA").is_some());
    }

    #[test]
    fn magic_mode_magic() {
        assert_eq!(find("a.b", "axb"), Some((0, 3)));
        assert_eq!(find("a\\.b", "a.b"), Some((0, 3)));
        assert_eq!(find("a\\*b", "a*b"), Some((0, 3)));
        assert_eq!(find("a*b", "aaab"), Some((0, 4)));
        assert_eq!(find("a\\+b", "aaab"), Some((0, 4)));
        assert_eq!(find("a\\?b", "ab"), Some((0, 2)));
        assert_eq!(find("a\\|b", "b"), Some((0, 1)));
        assert_eq!(find("\\(ab\\)", "ab"), Some((0, 2)));
        assert_eq!(find("^*", "*"), Some((0, 1)));
        assert_eq!(find("a^*", "a^^^"), Some((0, 4)));
    }

    #[test]
    fn magic_mode_nomagic() {
        let nomagic = |pattern: &str, text: &str| {
            compile(pattern.as_bytes(), Magic::Nomagic, false)
                .unwrap()
                .find(text.as_bytes())
                .map(|matched| (matched.start, matched.end))
        };
        assert_eq!(nomagic("a.b", "a.b"), Some((0, 3)));
        assert_eq!(nomagic("a.b", "axb"), None);
        assert_eq!(nomagic("a\\.b", "axb"), Some((0, 3)));
        assert_eq!(nomagic("a*", "a*"), Some((0, 2)));
        assert_eq!(nomagic("a\\*", "aaa"), Some((0, 3)));
        assert_eq!(nomagic("\\[ab]", "b"), Some((0, 1)));
        assert_eq!(nomagic("[ab]", "[ab]"), Some((0, 4)));
        assert_eq!(nomagic("^a$", "a"), Some((0, 1)));
        assert_eq!(nomagic("a^b", "a^b"), Some((0, 3)));
    }

    #[test]
    fn magic_mode_very_magic() {
        let very = |pattern: &str, text: &str| {
            compile(pattern.as_bytes(), Magic::VeryMagic, false)
                .unwrap()
                .find(text.as_bytes())
                .map(|matched| (matched.start, matched.end))
        };
        assert_eq!(very("(a|b)+", "ab"), Some((0, 2)));
        assert_eq!(very("a{2}", "aa"), Some((0, 2)));
        assert_eq!(very("a{2,3}", "aaa"), Some((0, 3)));
        assert_eq!(very("a{-}b", "aaab"), Some((0, 4)));
        assert_eq!(very("a\\.b", "a.b"), Some((0, 3)));
        assert_eq!(very("a\\{2}", "a{2}"), Some((0, 4)));
        assert_eq!(very("<foo>", "a foo "), Some((2, 5)));
        assert_eq!(very("a@=a", "a"), Some((0, 1)));
        assert_eq!(very("(a)@!b", "ac"), None);
        assert_eq!(very("(a)@<=a", "aa"), Some((1, 2)));
        assert_eq!(very("%(a|b)+", "ab"), Some((0, 2)));
        assert_eq!(very("%^a", "a"), Some((0, 1)));
        assert_eq!(very("a%$", "a"), Some((0, 1)));
        assert_eq!(very("\\<", "<"), Some((0, 1)));
        assert_eq!(very("*", "*"), Some((0, 1)));
    }

    #[test]
    fn magic_mode_very_nomagic() {
        let very = |pattern: &str, text: &str| {
            compile(pattern.as_bytes(), Magic::VeryNomagic, false)
                .unwrap()
                .find(text.as_bytes())
                .map(|matched| (matched.start, matched.end))
        };
        assert_eq!(very("a.b", "a.b"), Some((0, 3)));
        assert_eq!(very("a.b", "axb"), None);
        assert_eq!(very("a\\.b", "axb"), Some((0, 3)));
        assert_eq!(very("a\\*", "aaa"), Some((0, 3)));
        assert_eq!(very("\\[ab]", "b"), Some((0, 1)));
        assert_eq!(very("[ab]", "[ab]"), Some((0, 4)));
        assert_eq!(very("\\^a", "a"), Some((0, 1)));
        assert_eq!(very("a\\$", "a"), Some((0, 1)));
    }

    #[test]
    fn magic_switches_mid_pattern() {
        assert_eq!(find("\\ma\\.b", "a.b"), Some((0, 3)));
        assert_eq!(find("\\ma\\V.b", "a.b"), Some((0, 3)));
        assert_eq!(find("\\m(a|b)", "(a|b)"), Some((0, 5)));
        assert_eq!(find("\\V.a\\v.b", ".a.b"), Some((0, 4)));
    }

    #[test]
    fn find_at_and_allow_empty() {
        let regex = compile_re("a*");
        assert_eq!(
            regex.find_at(b"baa", 0, true).map(|m| (m.start, m.end)),
            Some((0, 0))
        );
        assert_eq!(
            regex.find_at(b"baa", 0, false).map(|m| (m.start, m.end)),
            Some((1, 3))
        );
        assert_eq!(
            regex.find_at(b"baa", 3, true).map(|m| (m.start, m.end)),
            Some((3, 3))
        );
        assert_eq!(regex.find_at(b"baa", 4, true), None);
        let empty = compile_re("");
        assert!(empty.find_at(b"abc", 0, false).is_none());
        assert_eq!(
            empty.find_at(b"abc", 2, true).map(|m| (m.start, m.end)),
            Some((2, 2))
        );
    }

    #[test]
    fn find_all_advances_after_empty_matches() {
        assert_eq!(all("", "abc"), vec![(0, 0), (1, 1), (2, 2), (3, 3)]);
        assert_eq!(all("a*", "baa"), vec![(0, 0), (1, 3), (3, 3)]);
        assert_eq!(all("^", "a\nb"), vec![(0, 0), (2, 2)]);
        assert_eq!(all("$", "a\nb"), vec![(1, 1), (3, 3)]);
    }

    #[test]
    fn find_all_result_reports_step_limit() {
        let mut regex = compile_re("a*");
        regex.set_step_limit(1);
        assert_eq!(
            regex.find_at_result(b"aaa", 0, true),
            Err(MatchError::StepLimitExceeded { limit: 1 })
        );
        assert!(regex.find(b"aaa").is_none());
    }

    #[test]
    fn step_limit_is_configurable() {
        let mut regex = compile_re("a");
        assert_eq!(regex.step_limit(), DEFAULT_STEP_LIMIT);
        assert_eq!(regex.find(b"a").map(|m| m.start), Some(0));
        regex.set_step_limit(1);
        assert_eq!(regex.step_limit(), 1);
        assert!(regex.find(b"a").is_none());
        let regex = compile_re("a").with_step_limit(50);
        assert_eq!(regex.find(b"a").map(|m| m.start), Some(0));
    }

    #[test]
    fn errors_have_offsets() {
        let error = compile(b"\\(a", Magic::Magic, false).unwrap_err();
        assert_eq!(error.offset, 0);
        assert!(error.message.contains("E54"));
        let error = compile(b"a\\)", Magic::Magic, false).unwrap_err();
        assert_eq!(error.offset, 1);
        assert!(error.message.contains("E55"));
        let error = compile(b"\\{2}", Magic::Magic, false).unwrap_err();
        assert_eq!(error.offset, 0);
        assert!(error.message.contains("E64"));
        let error = compile(b"a\\{x}", Magic::Magic, false).unwrap_err();
        assert!(error.message.contains("E554"));
        let error = compile(b"a\\zs*", Magic::Magic, false).unwrap_err();
        assert!(error.message.contains("E888"));
        // Reverse ranges are treated as literal characters instead of E944,
        // matching Vim's acceptance of runtime syntax patterns.
        assert!(compile(b"[z-a]", Magic::Magic, false).is_ok());
        let error = compile(b"abc\\", Magic::Magic, false).unwrap_err();
        assert!(error.message.contains("E14"));
    }

    #[test]
    fn unsupported_atoms_fail_loudly() {
        let result = compile(b"~", Magic::Magic, false);
        assert!(result.is_err(), "expected ~ to be rejected");
        assert!(compile(b"\\@>", Magic::Magic, false).is_err());
    }

    #[test]
    fn syntax_atoms_compile_and_match() {
        // \z( groups are ordinary captures exposed through z_group_slots.
        let regex = compile(b"\\z(a\\)b", Magic::Magic, false).unwrap();
        assert_eq!(regex.z_group_slots(), &[1]);
        let matched = regex.find(b"ab").unwrap();
        assert_eq!(matched.captures[0], Some((0, 1)));

        // \%[abc] matches the longest possible prefix.
        let regex = compile(b"x\\%[abc]", Magic::Magic, false).unwrap();
        assert_eq!(regex.find(b"xabc").map(|m| (m.start, m.end)), Some((0, 4)));
        assert_eq!(regex.find(b"xa").map(|m| (m.start, m.end)), Some((0, 2)));

        // \& requires both branches at the same position; the last branch is
        // the one that is matched.
        let regex = compile(b"foo\\&..", Magic::Magic, false).unwrap();
        assert_eq!(
            regex.find(b"foobar").map(|m| (m.start, m.end)),
            Some((0, 2))
        );

        // \%d / \%x character escapes.
        assert_eq!(
            compile(b"\\%d65", Magic::Magic, false)
                .unwrap()
                .find(b"xAy")
                .map(|m| (m.start, m.end)),
            Some((1, 2))
        );

        // Engine selector and contextual atoms are accepted; the latter never
        // match because the regex crate has no editor context.
        assert!(compile(b"\\%#=1abc", Magic::Magic, false).is_ok());
        assert!(compile(b"a\\%#b", Magic::Magic, false)
            .unwrap()
            .find(b"ab")
            .is_none());
    }

    #[test]
    fn capture_offsets_are_absolute() {
        let matched = compile_re("\\(b\\)\\(c\\)").find(b"abc").unwrap();
        assert_eq!(matched.captures, vec![Some((1, 2)), Some((2, 3))]);
    }

    #[test]
    fn captures_inside_lookahead_are_visible() {
        let matched = compile_re("\\(a\\)\\(b\\)\\@=").find(b"ab").unwrap();
        assert_eq!(matched.captures, vec![Some((0, 1)), Some((1, 2))]);
        let matched = compile_re("x\\(a\\)\\@!").find(b"xb").unwrap();
        assert_eq!(matched.captures, vec![None]);
    }

    #[test]
    fn atomic_and_lookaround_compile() {
        assert!(compile(b"\\(a\\)\\@>", Magic::Magic, false).is_ok());
        assert!(compile(b"\\(a\\)\\@123<=", Magic::Magic, false).is_ok());
    }

    #[test]
    fn invalid_utf8_collection() {
        let regex = compile(&[b'[', 0x80, b'-', 0xff, b']'], Magic::Magic, false).unwrap();
        assert_eq!(regex.find(&[0x80]).unwrap().start, 0);
        assert_eq!(regex.find(&[0xfe]).unwrap().start, 0);
        assert!(regex.find(b"abc").is_none());
    }
}
