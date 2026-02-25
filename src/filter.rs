use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::frequency::Frequency;

pub struct FilteredApp {
    pub name: String,
    pub score: i64,
    pub match_indices: Vec<usize>,
}

pub fn filter_apps(
    apps: &[String],
    query: &str,
    frequency: &Frequency,
) -> Vec<FilteredApp> {
    let matcher = SkimMatcherV2::default();
    let normalized_query: String = query.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut results: Vec<FilteredApp> = if normalized_query.is_empty() {
        apps.iter()
            .map(|name| {
                let freq_score = frequency.get(name) as i64 * 100;
                FilteredApp {
                    name: name.clone(),
                    score: freq_score,
                    match_indices: vec![],
                }
            })
            .collect()
    } else {
        apps.iter()
            .filter_map(|name| {
                matcher.fuzzy_indices(name, &normalized_query).map(|(score, indices)| {
                    let freq_score = frequency.get(name) as i64 * 100;
                    let exact_bonus = if name == &normalized_query { 1_000_000 } else { 0 };
                    FilteredApp {
                        name: name.clone(),
                        score: score + freq_score + exact_bonus,
                        match_indices: indices,
                    }
                })
            })
            .collect()
    };
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results
}
