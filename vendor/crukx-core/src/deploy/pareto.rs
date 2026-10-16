use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CandidateStat {
    pub id: String,
    pub size_bytes: u64,
    pub peak_memory_mb: u64,
    #[serde(default)]
    pub tok_s: Option<f64>,
    pub quality_regression: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SelectionConstraints {
    pub max_quality_regression: f64,
    #[serde(default)]
    pub max_memory_mb: Option<u64>,
    #[serde(default)]
    pub min_tok_s: Option<f64>,
}

pub fn is_feasible(candidate: &CandidateStat, constraints: &SelectionConstraints) -> bool {
    if candidate.quality_regression > constraints.max_quality_regression {
        return false;
    }
    if constraints
        .max_memory_mb
        .is_some_and(|max| candidate.peak_memory_mb > max)
    {
        return false;
    }
    if constraints
        .min_tok_s
        .is_some_and(|min| candidate.tok_s.is_none_or(|tok| tok < min))
    {
        return false;
    }
    true
}

fn dominates(a: &CandidateStat, b: &CandidateStat) -> bool {
    let size = a.size_bytes <= b.size_bytes;
    let memory = a.peak_memory_mb <= b.peak_memory_mb;
    let quality = a.quality_regression <= b.quality_regression;
    let throughput = match (a.tok_s, b.tok_s) {
        (Some(x), Some(y)) => x >= y,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => true,
    };
    let strictly_better = a.size_bytes < b.size_bytes
        || a.peak_memory_mb < b.peak_memory_mb
        || a.quality_regression < b.quality_regression
        || matches!((a.tok_s, b.tok_s), (Some(x), Some(y)) if x > y)
        || matches!((a.tok_s, b.tok_s), (Some(_), None));
    size && memory && quality && throughput && strictly_better
}

pub fn frontier(candidates: &[CandidateStat]) -> Vec<&CandidateStat> {
    candidates
        .iter()
        .filter(|c| !candidates.iter().any(|other| dominates(other, c)))
        .collect()
}

#[allow(clippy::needless_lifetimes)]
pub fn select<'a>(
    candidates: &'a [CandidateStat],
    constraints: &SelectionConstraints,
) -> Option<&'a CandidateStat> {
    let feasible: Vec<&CandidateStat> = candidates
        .iter()
        .filter(|c| is_feasible(c, constraints))
        .collect();
    if feasible.is_empty() {
        return None;
    }
    let frontier_set: Vec<*const CandidateStat> = frontier(candidates)
        .iter()
        .map(|c| *c as *const _)
        .collect();
    feasible
        .into_iter()
        .filter(|c| frontier_set.contains(&(*c as *const _)))
        .min_by_key(|c| c.size_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_table() -> Vec<CandidateStat> {
        vec![
            CandidateStat {
                id: "BF16".to_string(),
                size_bytes: 8_200_000_000,
                peak_memory_mb: 9400,
                tok_s: Some(21.0),
                quality_regression: 0.0,
            },
            CandidateStat {
                id: "Q8".to_string(),
                size_bytes: 4_500_000_000,
                peak_memory_mb: 6000,
                tok_s: Some(31.0),
                quality_regression: 0.0003,
            },
            CandidateStat {
                id: "Q6".to_string(),
                size_bytes: 3_600_000_000,
                peak_memory_mb: 5300,
                tok_s: Some(38.0),
                quality_regression: 0.0008,
            },
            CandidateStat {
                id: "Q4".to_string(),
                size_bytes: 2_500_000_000,
                peak_memory_mb: 4700,
                tok_s: Some(51.0),
                quality_regression: 0.0019,
            },
            CandidateStat {
                id: "Q3".to_string(),
                size_bytes: 2_000_000_000,
                peak_memory_mb: 4200,
                tok_s: Some(58.0),
                quality_regression: 0.028,
            },
        ]
    }

    #[test]
    fn doc_example_selects_q4_under_quarter_percent_cap() {
        let table = doc_table();
        let selected = select(
            &table,
            &SelectionConstraints {
                max_quality_regression: 0.0025,
                max_memory_mb: None,
                min_tok_s: None,
            },
        )
        .unwrap();
        assert_eq!(selected.id, "Q4");
    }

    #[test]
    fn impossible_cap_selects_nothing() {
        let table = doc_table();
        assert_eq!(
            select(
                &table,
                &SelectionConstraints {
                    max_quality_regression: 0.0,
                    max_memory_mb: Some(1000),
                    min_tok_s: None,
                }
            ),
            None
        );
    }

    #[test]
    fn frontier_holds_tradeoffs_not_dominated_points() {
        let table = doc_table();
        let ids: Vec<&str> = frontier(&table).iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"BF16"));
        assert!(ids.contains(&"Q4"));
        let dominated = CandidateStat {
            id: "Q8-big".to_string(),
            size_bytes: 5_000_000_000,
            peak_memory_mb: 6500,
            tok_s: Some(25.0),
            quality_regression: 0.0005,
        };
        let mut with_dominated = table.clone();
        with_dominated.push(dominated);
        let ids: Vec<&str> = frontier(&with_dominated)
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert!(!ids.contains(&"Q8-big"));
    }

    #[test]
    fn memory_cap_filters_before_selection() {
        let table = doc_table();
        let selected = select(
            &table,
            &SelectionConstraints {
                max_quality_regression: 0.0025,
                max_memory_mb: Some(4600),
                min_tok_s: None,
            },
        );
        assert_eq!(selected, None);
    }

    #[test]
    fn throughput_floor_filters() {
        let table = doc_table();
        let selected = select(
            &table,
            &SelectionConstraints {
                max_quality_regression: 0.01,
                max_memory_mb: None,
                min_tok_s: Some(40.0),
            },
        )
        .unwrap();
        assert_eq!(selected.id, "Q4");
    }
}
