//! The bare-`crukx` welcome screen — the same first-impression moment
//! `claude`/`gh`/`docker` give you when you type the command with nothing
//! after it. Shown once, on the no-subcommand path only; every other
//! command's output stays plain and script-safe (see colors.rs).

use crate::colors;

const ICON: &str = "◆";
const TITLE: &str = "Crukx";
const TAGLINE: &str = "Release gate for AI-written software";

/// Renders a boxed header around `TITLE`/`TAGLINE`, width computed from the
/// actual content so the border can never drift out of alignment if either
/// string changes — no hand-counted padding to get wrong.
pub fn render(version: &str, enabled: bool) -> String {
    let title_line = format!(
        "{ICON} {TITLE}  {}",
        colors::dim(&format!("v{version}"), enabled)
    );
    let title_line_visible_len = format!("{ICON} {TITLE}  v{version}").chars().count();
    let tagline_visible_len = TAGLINE.chars().count();
    let inner_width = title_line_visible_len.max(tagline_visible_len) + 2;

    let top = format!("╭{}╮", "─".repeat(inner_width));
    let bottom = format!("╰{}╯", "─".repeat(inner_width));
    let title_row = format!(
        "│ {}{} │",
        title_line,
        " ".repeat(inner_width - title_line_visible_len - 2)
    );
    let tagline_row = format!(
        "│ {}{} │",
        colors::dim(TAGLINE, enabled),
        " ".repeat(inner_width - tagline_visible_len - 2)
    );

    let border = |line: &str| colors::amber(line, enabled);
    format!(
        "{}\n{}\n{}\n{}\n",
        border(&top),
        title_row,
        tagline_row,
        border(&bottom)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_without_color_produces_aligned_plain_text_lines() {
        let banner = render("0.1.0-alpha.0", false);
        let lines: Vec<&str> = banner.lines().collect();
        assert_eq!(lines.len(), 4);
        // Every line (top border, title, tagline, bottom border) must be
        // the same character-width for the box to actually look like a box.
        let widths: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "box lines misaligned: {widths:?}"
        );
    }

    #[test]
    fn render_contains_the_title_and_tagline() {
        let banner = render("0.1.0-alpha.0", false);
        assert!(banner.contains("Crukx"));
        assert!(banner.contains("Release gate for AI-written software"));
        assert!(banner.contains("0.1.0-alpha.0"));
    }

    #[test]
    fn render_with_color_still_produces_valid_box_structure() {
        let banner = render("0.1.0-alpha.0", true);
        assert!(banner.contains("\x1b["));
        assert!(banner.contains('╭'));
        assert!(banner.contains('╰'));
    }
}
