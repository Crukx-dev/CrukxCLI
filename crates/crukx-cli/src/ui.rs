//! Shared output vocabulary — one place for the section-header style,
//! hint lines, and mark glyphs so every command speaks the same visual
//! language (the way `gh` and `opencode` keep their subcommands looking
//! like one tool). All functions take `enabled` so call sites stay pure
//! w.r.t. the color decision made once in main.

use crate::colors;

/// `◆ Title · context` — bold glyph + title, dim context. The box-drawn
/// banner stays a bare-`crukx`-only moment; running commands get this
/// lighter header instead.
pub fn header(title: &str, context: Option<&str>, enabled: bool) -> String {
    match context {
        Some(context) => format!(
            "{} {}",
            colors::bold(&format!("◆ {title}"), enabled),
            colors::dim(&format!("· {context}"), enabled)
        ),
        None => colors::bold(&format!("◆ {title}"), enabled),
    }
}

/// Dim actionable next step under an error or failure — the difference
/// between "it broke" and "here's what to do."
pub fn hint(text: &str, enabled: bool) -> String {
    format!("hint: {}", colors::dim(text, enabled))
}

/// Success / failure / warning marks with consistent glyphs.
pub fn ok_mark(enabled: bool) -> String {
    colors::green("✓", enabled)
}
pub fn fail_mark(enabled: bool) -> String {
    colors::red("✗", enabled)
}
pub fn warn_mark(enabled: bool) -> String {
    colors::yellow("!", enabled)
}

/// Dim secondary text — metadata, counts, context lines.
pub fn dim_label(text: &str, enabled: bool) -> String {
    colors::dim(text, enabled)
}

/// `── Title ──────────` style section rule for dashboards.
pub fn section(title: &str, enabled: bool) -> String {
    format!(
        "{} {}",
        colors::bold(title, enabled),
        colors::dim(&"─".repeat(24), enabled)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_without_context_is_bold_title_only() {
        assert_eq!(header("Crukx gate", None, false), "◆ Crukx gate");
    }

    #[test]
    fn header_with_context_appends_separator() {
        assert_eq!(header("history", Some("last 5"), false), "◆ history · last 5");
    }

    #[test]
    fn hint_prefixes_the_dimmed_text() {
        assert_eq!(hint("run crukx capture", false), "hint: run crukx capture");
    }

    #[test]
    fn color_enabled_paths_emit_escapes() {
        assert!(header("gate", None, true).contains("\x1b[1m"));
        assert!(colors::dim("ctx", true).contains("\x1b[2m"));
    }
}
