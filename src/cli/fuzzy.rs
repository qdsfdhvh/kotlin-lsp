//! Fuzzy symbol search — `find --fuzzy` enables subsequence matching.
//!
//! Splits query into space-separated tokens, scores each candidate name
//! by how well its characters contain the query tokens as subsequences.

pub(crate) fn fuzzy_score(query: &str, candidate: &str) -> f64 {
    let query_lower = query.to_lowercase();
    let cand_lower = candidate.to_lowercase();

    // Split query into tokens on whitespace
    let tokens: Vec<&str> = query_lower.split_whitespace().collect();
    if tokens.is_empty() {
        return 0.0;
    }

    let mut total_score = 0.0;
    for token in &tokens {
        if token.len() > cand_lower.len() {
            return 0.0; // token longer than candidate → no match
        }
        let score = subsequence_score(token, &cand_lower);
        if score == 0.0 {
            return 0.0; // all tokens must match
        }
        total_score += score;
    }

    // Normalize by token count and bonus for consecutive chars
    total_score / tokens.len() as f64
}

/// Score how well `pattern` appears as a subsequence in `text`.
/// Returns 0.0 if not a subsequence, 1.0 for perfect consecutive match.
fn subsequence_score(pattern: &str, text: &str) -> f64 {
    let p_chars: Vec<char> = pattern.chars().collect();
    let t_chars: Vec<char> = text.chars().collect();

    let mut p_idx = 0;
    let mut prev_t_idx: Option<usize> = None;
    let mut consecutive_bonus = 0.0;
    let mut matched = 0;

    for (t_idx, &tc) in t_chars.iter().enumerate() {
        if p_idx < p_chars.len() && tc == p_chars[p_idx] {
            matched += 1;
            if let Some(prev) = prev_t_idx {
                if t_idx == prev + 1 {
                    consecutive_bonus += 0.1; // bonus for consecutive chars
                }
            }
            prev_t_idx = Some(t_idx);
            p_idx += 1;
        }
    }

    if matched < p_chars.len() {
        return 0.0; // not all pattern chars matched
    }

    // Base score: fraction of matched vs total
    let base = matched as f64 / p_chars.len() as f64;

    // Bonus: shorter gaps mean higher score
    let gap_penalty = if let (Some(first), Some(last)) = (
        t_chars.iter().position(|&c| c == p_chars[0]),
        t_chars.iter().rposition(|&c| c == *p_chars.last().unwrap()),
    ) {
        let span = (last - first) as f64;
        let ideal = p_chars.len() as f64;
        if span > 0.0 {
            1.0 - ((span - ideal) / (span + 1.0))
        } else {
            1.0
        }
    } else {
        0.0
    };

    (base * 0.7 + gap_penalty * 0.2 + consecutive_bonus * 0.1).min(1.0)
}

pub(crate) fn fuzzy_find(
    query: &str,
    candidates: &[String],
    max_results: usize,
) -> Vec<(String, f64)> {
    let mut scored: Vec<(String, f64)> = candidates
        .iter()
        .filter_map(|name| {
            let score = fuzzy_score(query, name);
            if score > 0.0 {
                Some((name.clone(), score))
            } else {
                None
            }
        })
        .collect();

    // Sort by score descending, then by name for ties
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    scored.truncate(max_results);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        assert!(fuzzy_score("login", "login") > 0.9);
        assert!(fuzzy_score("LoginRepository", "LoginRepository") > 0.9);
    }

    #[test]
    fn subsequence_match() {
        assert!(fuzzy_score("login repo", "LoginRepository") > 0.5);
        assert!(fuzzy_score("vm login", "LoginViewModel") > 0.3);
        assert!(fuzzy_score("user repo", "UserRepository") > 0.5);
    }

    #[test]
    fn no_match() {
        assert_eq!(fuzzy_score("xyz", "LoginViewModel"), 0.0);
        assert_eq!(fuzzy_score("ab cd", "Login"), 0.0);
    }

    #[test]
    fn ranking() {
        let candidates: Vec<String> = vec![
            "LoginRepository".into(),
            "AuthRepository".into(),
            "LoginViewModel".into(),
            "UserRemoteDataSource".into(),
        ];
        let results = fuzzy_find("login repo", &candidates, 5);
        assert!(!results.is_empty());
        // LoginRepository should rank higher than AuthRepository
        let lr_pos = results.iter().position(|(n, _)| n == "LoginRepository");
        let ar_pos = results.iter().position(|(n, _)| n == "AuthRepository");
        if let (Some(lr), Some(ar)) = (lr_pos, ar_pos) {
            assert!(lr < ar, "LoginRepository should rank before AuthRepository");
        }
    }

    #[test]
    fn single_token() {
        let candidates: Vec<String> = vec!["Foo".into(), "Bar".into(), "Foobar".into()];
        let results = fuzzy_find("foo", &candidates, 5);
        assert_eq!(results.len(), 2); // Foo and Foobar
    }
}
