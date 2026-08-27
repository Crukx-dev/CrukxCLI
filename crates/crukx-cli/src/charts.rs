//! Terminal charts — pure string functions, no color, no I/O. Callers
//! paint results with `colors` and print them wherever they like. Every
//! function degrades gracefully: empty input renders as an empty string,
//! never a panic, so a chart can never take down a gate run.

/// The 8-level vertical block ramp used for sparklines, dark → full.
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// One-line trend: `▁▂▃▅█`. Values are min-max normalized; flat input
/// (all equal) renders as a mid-height strip rather than pegging to the
/// top, because "no change" should look calm, not maximal.
pub fn sparkline(values: &[f64]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    if (max - min).abs() < f64::EPSILON {
        return "▄".repeat(values.len());
    }
    values
        .iter()
        .map(|v| {
            let level = (((v - min) / (max - min)) * 7.0).round() as usize;
            BLOCKS[level.clamp(0, 7)]
        })
        .collect()
}

/// Horizontal meter `██████░░░░` of `width` cells showing `value/max`
/// (both non-negative; ratio clamped into 0.0..=1.0 so over-budget
/// values render full, and the caller can color that red).
pub fn bar_meter(value: f64, max: f64, width: usize) -> String {
    if width == 0 || max <= 0.0 {
        return "░".repeat(width);
    }
    let ratio = (value / max).clamp(0.0, 1.0);
    let filled = (ratio * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// Pass/fail dot strip for repeated runs: filled `●` per PASS, hollow
/// `○` per fail. Returned as `(passed_dots, failed_dots)` so callers can
/// paint the two segments different colors — `str::split_at` indexes
/// *bytes* and would panic mid-glyph, hence no single-string variant.
pub fn pass_dots_split(passed: usize, total: usize) -> (String, String) {
    let passed = passed.min(total);
    (
        "●".repeat(passed),
        "○".repeat(total.saturating_sub(passed)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_empty_input_renders_empty() {
        assert_eq!(sparkline(&[]), "");
    }

    #[test]
    fn sparkline_single_value_renders_one_block() {
        assert_eq!(sparkline(&[42.0]).chars().count(), 1);
    }

    #[test]
    fn sparkline_maps_min_to_lowest_and_max_to_highest_block() {
        assert_eq!(sparkline(&[0.0, 100.0]), "▁█");
    }

    #[test]
    fn sparkline_flat_series_is_a_calm_mid_strip_not_pegs() {
        assert_eq!(sparkline(&[5.0, 5.0, 5.0]), "▄▄▄");
    }

    #[test]
    fn sparkline_ramps_monotonically_for_linear_input() {
        let line = sparkline(&[1.0, 2.0, 3.0, 4.0]);
        let levels: Vec<usize> = line
            .chars()
            .map(|c| BLOCKS.iter().position(|&b| b == c).unwrap())
            .collect();
        assert!(
            levels.windows(2).all(|w| w[0] <= w[1]),
            "not monotonic: {levels:?}"
        );
    }

    #[test]
    fn sparkline_survives_nan_and_infinity_without_panicking() {
        // A latency sample of NaN or +inf must degrade to *some* glyph,
        // never panic and never emit a replacement char that breaks the
        // strip's width.
        let hostile = [f64::NAN, f64::INFINITY, 1.0, f64::NEG_INFINITY];
        let line = sparkline(&hostile);
        assert_eq!(line.chars().count(), hostile.len());
        for ch in line.chars() {
            assert!(BLOCKS.contains(&ch), "glyph {ch:?} outside the ramp");
        }
    }

    #[test]
    fn sparkline_property_never_panic_or_widen_on_deterministic_noise() {
        // Dependency-free property test: an LCG over 2,000 hostile-ish
        // series (mixed magnitudes, negatives, duplicates) must always
        // render exactly one in-ramp glyph per value.
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..2_000 {
            let len = (next() % 64) as usize;
            let series: Vec<f64> = (0..len)
                .map(|_| match next() % 6 {
                    0 => f64::NAN,
                    1 => f64::INFINITY,
                    2 => -((next() % 1000) as f64),
                    3 => (next() % 100) as f64,
                    _ => ((next() % 10_000_000) as f64) / 7.0,
                })
                .collect();
            let line = sparkline(&series);
            assert_eq!(line.chars().count(), len);
            assert!(line.chars().all(|c| BLOCKS.contains(&c)));
        }
    }

    #[test]
    fn bar_meter_holds_width_under_hostile_ratios() {
        for &(value, max) in &[
            (f64::NAN, 10.0),
            (10.0, f64::NAN),
            (f64::INFINITY, 5.0),
            (-5.0, 10.0),
            (0.0, 0.0),
        ] {
            assert_eq!(bar_meter(value, max, 12).chars().count(), 12);
        }
    }

    #[test]
    fn bar_meter_zero_value_is_all_unfilled() {
        assert_eq!(bar_meter(0.0, 10.0, 10), "░░░░░░░░░░");
    }

    #[test]
    fn bar_meter_half_fills_half() {
        assert_eq!(bar_meter(5.0, 10.0, 10), "█████░░░░░");
    }

    #[test]
    fn bar_meter_full_fills_completely() {
        assert_eq!(bar_meter(10.0, 10.0, 10), "██████████");
    }

    #[test]
    fn bar_meter_over_budget_clamps_to_full_never_panics() {
        assert_eq!(bar_meter(50.0, 10.0, 4), "████");
    }

    #[test]
    fn bar_meter_degenerate_max_renders_empty_cells() {
        assert_eq!(bar_meter(5.0, 0.0, 3), "░░░");
        assert_eq!(bar_meter(5.0, -1.0, 2), "░░");
    }

    #[test]
    fn pass_dots_marks_passes_filled_and_fails_hollow() {
        let (pass, fail) = pass_dots_split(3, 5);
        assert_eq!(format!("{pass}{fail}"), "●●●○○");
        let (none, all) = pass_dots_split(0, 2);
        assert_eq!(format!("{none}{all}"), "○○");
        assert_eq!(pass_dots_split(4, 4).0, "●●●●");
    }

    #[test]
    fn pass_dots_with_passed_over_total_clamps_safely() {
        assert_eq!(pass_dots_split(9, 3).0, "●●●");
    }

    #[test]
    fn pass_dots_split_never_splits_a_glyph() {
        let (pass_part, fail_part) = pass_dots_split(3, 5);
        assert_eq!(pass_part, "●●●");
        assert_eq!(fail_part, "○○");
        let (all_pass, none) = pass_dots_split(9, 3);
        assert_eq!(all_pass, "●●●");
        assert_eq!(none, "");
    }
}
