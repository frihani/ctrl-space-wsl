use crate::frequency::Frequency;

pub struct FilteredApp {
    pub name: String,
    pub score: i64,
    pub match_indices: Vec<usize>,
}

fn fuzzy_match_token(
    text: &str,
    token: &str,
    start_idx: usize,
    case_sensitive: bool,
) -> Option<(i64, Vec<usize>)> {
    let text_chars: Vec<char> = if case_sensitive {
        text.chars().collect()
    } else {
        text.chars().map(|c| c.to_ascii_lowercase()).collect()
    };
    let token_chars: Vec<char> = if case_sensitive {
        token.chars().collect()
    } else {
        token.chars().map(|c| c.to_ascii_lowercase()).collect()
    };

    if token_chars.is_empty() {
        return Some((0, vec![]));
    }

    let mut best_score: Option<i64> = None;
    let mut best_indices: Vec<usize> = vec![];

    #[allow(clippy::too_many_arguments)]
    fn search(
        text: &[char],
        token: &[char],
        text_idx: usize,
        token_idx: usize,
        indices: &mut Vec<usize>,
        score: i64,
        best_score: &mut Option<i64>,
        best_indices: &mut Vec<usize>,
        start_idx: usize,
    ) {
        if token_idx == token.len() {
            if best_score.is_none() || score > best_score.unwrap() {
                *best_score = Some(score);
                *best_indices = indices.clone();
            }
            return;
        }

        if text_idx >= text.len() {
            return;
        }

        let remaining_text = text.len() - text_idx;
        let remaining_token = token.len() - token_idx;
        if remaining_text < remaining_token {
            return;
        }

        for i in text_idx..=text.len() - remaining_token {
            if text[i] == token[token_idx] {
                let mut char_score: i64 = 1;

                if i == 0 || text[i - 1] == ' ' || text[i - 1] == '-' || text[i - 1] == '_' {
                    char_score += 10;
                }

                if token_idx > 0 && i == *indices.last().unwrap() + 1 {
                    char_score += 5;
                }

                indices.push(start_idx + i);
                search(
                    text,
                    token,
                    i + 1,
                    token_idx + 1,
                    indices,
                    score + char_score,
                    best_score,
                    best_indices,
                    start_idx,
                );
                indices.pop();
            }
        }
    }

    let mut indices = vec![];
    search(
        &text_chars,
        &token_chars,
        0,
        0,
        &mut indices,
        0,
        &mut best_score,
        &mut best_indices,
        start_idx,
    );

    best_score.map(|s| (s, best_indices))
}

fn match_fragmented(
    name: &str,
    tokens: &[&str],
    case_sensitive: bool,
) -> Option<(i64, Vec<usize>)> {
    if tokens.is_empty() {
        return Some((0, vec![]));
    }

    let mut total_score: i64 = 0;
    let mut all_indices: Vec<usize> = vec![];
    let mut search_start: usize = 0;

    for token in tokens {
        if token.is_empty() {
            continue;
        }

        let remaining = &name[search_start..];
        if let Some((score, indices)) =
            fuzzy_match_token(remaining, token, search_start, case_sensitive)
        {
            total_score += score;
            if let Some(&last_idx) = indices.last() {
                let char_end = name[..=last_idx].chars().count();
                search_start = name
                    .char_indices()
                    .nth(char_end)
                    .map(|(i, _)| i)
                    .unwrap_or(name.len());
            }
            all_indices.extend(indices);
        } else {
            return None;
        }
    }

    Some((total_score, all_indices))
}

pub fn filter_apps(apps: &[String], query: &str, frequency: &Frequency) -> Vec<FilteredApp> {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    let query_joined: String = tokens.join(" ");
    // Smart-case: case-sensitive if query has any uppercase letter
    let case_sensitive = query.chars().any(|c| c.is_ascii_uppercase());

    let mut results: Vec<FilteredApp> = if tokens.is_empty() {
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
                match_fragmented(name, &tokens, case_sensitive).map(|(score, indices)| {
                    let freq_score = frequency.get(name) as i64 * 100;

                    let exact_bonus = if case_sensitive {
                        if *name == query_joined { 1_000_000 } else { 0 }
                    } else if name.eq_ignore_ascii_case(&query_joined) {
                        1_000_000
                    } else {
                        0
                    };

                    let prefix_bonus = if case_sensitive {
                        if name.starts_with(&query_joined) { 100_000 } else { 0 }
                    } else if name
                        .to_ascii_lowercase()
                        .starts_with(&query_joined.to_ascii_lowercase())
                    {
                        100_000
                    } else {
                        0
                    };

                    FilteredApp {
                        name: name.clone(),
                        score: score + freq_score + exact_bonus + prefix_bonus,
                        match_indices: indices,
                    }
                })
            })
            .collect()
    };
    results.sort_by(|a, b| {
        b.score.cmp(&a.score).then_with(|| {
            a.name
                .bytes()
                .map(sort_byte)
                .cmp(b.name.bytes().map(sort_byte))
        })
    });
    results
}

/// Sort order: digits first, then lowercase, then uppercase (0-9 a-z A-Z).
fn sort_byte(b: u8) -> (u8, u8) {
    match b {
        b'0'..=b'9' => (0, b),
        b'a'..=b'z' => (1, b),
        b'A'..=b'Z' => (2, b),
        _           => (3, b),
    }
}
