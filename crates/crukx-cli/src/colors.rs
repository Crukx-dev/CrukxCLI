//! ANSI color for interactive terminal output. Raw escape codes, no crate —
//! this is a handful of colors, not worth a dependency for.
//!
//! Color is opt-out, not opt-in, matching `gh`/`cargo`/`docker`: on by
//! default in a real terminal, off when `NO_COLOR` is set (the
//! informal-but-widely-honored convention, <https://no-color.org>) or when
//! stderr isn't a tty (piped into a file, CI log, or `assert_cmd`'s
//! captured output in tests) — plain text there, so scripts and tests that
//! parse crukx's output never have to strip escape codes.

use std::io::IsTerminal;

/// Whether *this run* should emit color — checks the real environment.
/// Kept separate from `paint` so the actual formatting logic stays a pure,
/// testable function instead of depending on env/tty state itself.
pub fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
}

/// Wraps `text` in the given SGR code if `enabled`, otherwise returns it
/// unchanged. Pure — the impure "should we?" decision lives in
/// `color_enabled`, not here.
pub fn paint(code: &str, text: &str, enabled: bool) -> String {
    if enabled {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn green(text: &str, enabled: bool) -> String {
    paint("32", text, enabled)
}
pub fn red(text: &str, enabled: bool) -> String {
    paint("31", text, enabled)
}
pub fn yellow(text: &str, enabled: bool) -> String {
    paint("33", text, enabled)
}
pub fn bold(text: &str, enabled: bool) -> String {
    paint("1", text, enabled)
}
pub fn dim(text: &str, enabled: bool) -> String {
    paint("2", text, enabled)
}
/// Brand accent — approximates the amber (#E8A33D) used elsewhere in
/// Crukx's visual identity, in 256-color since not every terminal
/// negotiates truecolor.
pub fn amber(text: &str, enabled: bool) -> String {
    paint("38;5;215", text, enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_wraps_in_escape_codes_when_enabled() {
        assert_eq!(paint("32", "ok", true), "\x1b[32mok\x1b[0m");
    }

    #[test]
    fn paint_returns_plain_text_when_disabled() {
        assert_eq!(paint("32", "ok", false), "ok");
    }

    #[test]
    fn color_helpers_use_their_documented_codes() {
        assert_eq!(green("x", true), "\x1b[32mx\x1b[0m");
        assert_eq!(red("x", true), "\x1b[31mx\x1b[0m");
        assert_eq!(yellow("x", true), "\x1b[33mx\x1b[0m");
        assert_eq!(bold("x", true), "\x1b[1mx\x1b[0m");
        assert_eq!(dim("x", true), "\x1b[2mx\x1b[0m");
    }

    #[test]
    fn color_enabled_is_false_when_no_color_is_set_regardless_of_tty() {
        // SAFETY: this test crate is single-threaded per-test-binary for
        // env mutation purposes is not guaranteed by cargo test in
        // general, but NO_COLOR is only ever read/written here — no other
        // test in this crate touches it.
        std::env::set_var("NO_COLOR", "1");
        assert!(!color_enabled());
        std::env::remove_var("NO_COLOR");
    }
}
