//! Semantic symbol search — `search "query"` returns TF-IDF ranked results.
//!
//! Indexes symbol names, documentation (KDoc), signatures, and return types
//! from the workspace index. Tokenizes on camelCase, snake_case, and whitespace.
//! No external ML dependencies — pure TF-IDF with BM25-inspired scoring.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

// ── tokenization ────────────────────────────────────────────────────────────

/// Split an identifier into tokens on camelCase and snake_case boundaries.
fn tokenize_identifier(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;

    for ch in s.chars() {
        if ch == '_' || ch == '-' || ch == '.' || ch == '/' {
            if !current.is_empty() {
                tokens.push(current.to_lowercase());
                current.clear();
            }
            prev_lower = false;
        } else if ch.is_uppercase() && prev_lower {
            // camelCase boundary: "fooBar" → ["foo", "bar"]
            if !current.is_empty() {
                tokens.push(current.to_lowercase());
                current.clear();
            }
            current.push(ch);
            prev_lower = false;
        } else {
            current.push(ch);
            prev_lower = ch.is_lowercase();
        }
    }
    if !current.is_empty() {
        tokens.push(current.to_lowercase());
    }
    tokens
}

/// Tokenize free text: split on whitespace, strip punctuation, lowercase.
fn tokenize_query(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| c.is_ascii_punctuation() && c != '_')
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

// ── TF-IDF index ────────────────────────────────────────────────────────────

/// A single document in the search index.
struct SearchDoc {
    /// Display name (symbol name).
    name: String,
    /// Symbol kind.
    kind: String,
    /// File path (relative when possible).
    file: String,
    /// 1-based line.
    line: u32,
    /// Signature / detail.
    signature: String,
    /// KDoc summary.
    doc: Option<String>,
    /// Pre-tokenized combined text.
    tokens: Vec<String>,
}

/// TF-IDF search index over workspace symbols.
struct TfIdfIndex {
    docs: Vec<SearchDoc>,
    /// Term → (doc_id → term_frequency)
    inverted: HashMap<String, HashMap<usize, u32>>,
    /// Number of documents containing each term.
    doc_freq: HashMap<String, u32>,
}

impl TfIdfIndex {
    fn new() -> Self {
        Self {
            docs: Vec::new(),
            inverted: HashMap::new(),
            doc_freq: HashMap::new(),
        }
    }

    /// Add a document to the index.
    fn add(&mut self, doc_id: usize, tokens: &[String]) {
        for token in tokens {
            let entry = self.inverted.entry(token.clone()).or_default();
            *entry.entry(doc_id).or_insert(0) += 1;
            *self.doc_freq.entry(token.clone()).or_insert(0) = 1; // will recount after
        }
    }

    /// Recompute document frequencies (call after all docs are added).
    fn finalize(&mut self) {
        self.doc_freq.clear();
        for (term, posting) in &self.inverted {
            self.doc_freq.insert(term.clone(), posting.len() as u32);
        }
    }

    /// Search for `query_tokens`, returning top `max_results` ranked results.
    fn search(&self, query_tokens: &[String], max_results: usize) -> Vec<SearchResult> {
        let n = self.docs.len() as f64;
        if n == 0.0 {
            return vec![];
        }

        let mut scores: Vec<f64> = vec![0.0; self.docs.len()];

        // BM25-inspired scoring with TF-IDF
        let k1 = 1.2;
        let b = 0.75;

        // Average document length
        let avgdl: f64 = if self.docs.is_empty() {
            1.0
        } else {
            self.docs.iter().map(|d| d.tokens.len() as f64).sum::<f64>() / n
        };

        for qt in query_tokens {
            if let Some(posting) = self.inverted.get(qt) {
                let df = *self.doc_freq.get(qt).unwrap_or(&1) as f64;
                let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.0);

                for (&doc_id, &tf) in posting {
                    let doc_len = self.docs[doc_id].tokens.len() as f64;
                    let tf_norm = (tf as f64 * (k1 + 1.0))
                        / (tf as f64 + k1 * (1.0 - b + b * doc_len / avgdl));
                    scores[doc_id] += idf * tf_norm;
                }
            }
            // Also try partial prefix match for incomplete tokens
            for (term, posting) in &self.inverted {
                if term.starts_with(qt.as_str()) && term != qt {
                    let df = *self.doc_freq.get(term).unwrap_or(&1) as f64;
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.0) * 0.5; // reduced weight
                    for (&doc_id, &tf) in posting {
                        let doc_len = self.docs[doc_id].tokens.len() as f64;
                        let tf_norm = (tf as f64 * (k1 + 1.0))
                            / (tf as f64 + k1 * (1.0 - b + b * doc_len / avgdl));
                        scores[doc_id] += idf * tf_norm;
                    }
                }
            }
        }

        // Collect and rank
        let mut ranked: Vec<(usize, f64)> = scores
            .into_iter()
            .enumerate()
            .filter(|(_, s)| *s > 0.0)
            .collect();

        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        ranked.truncate(max_results);

        let max_score = ranked.first().map(|(_, s)| *s).unwrap_or(1.0);

        ranked
            .into_iter()
            .map(|(id, score)| {
                let doc = &self.docs[id];
                SearchResult {
                    name: doc.name.clone(),
                    kind: doc.kind.clone(),
                    file: doc.file.clone(),
                    line: doc.line,
                    signature: doc.signature.clone(),
                    doc: doc.doc.clone(),
                    score: if max_score > 0.0 {
                        (score / max_score).min(1.0)
                    } else {
                        0.0
                    },
                }
            })
            .collect()
    }
}

// ── Search result ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct SearchResult {
    name: String,
    kind: String,
    file: String,
    line: u32,
    signature: String,
    doc: Option<String>,
    /// Relevance score 0.0–1.0.
    score: f64,
}

// ── Public API ──────────────────────────────────────────────────────────────

pub(crate) fn run_search(query: &str, json: bool, max_results: usize) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let index = rt.block_on(crate::cli::run::build_index(&root, false));

    let mut tfidf = TfIdfIndex::new();
    let query_tokens = tokenize_query(query);

    // Also try to tokenize query as if it were a camelCase identifier
    let mut all_query_tokens = query_tokens.clone();
    for qt in &query_tokens {
        let sub = tokenize_identifier(qt);
        for s in sub {
            if !all_query_tokens.contains(&s) {
                all_query_tokens.push(s);
            }
        }
    }

    // Build index from workspace symbols
    for file_entry in index.files.iter() {
        let file_path = file_entry.key();
        let file_data = file_entry.value();

        // Compute a display path (relative to root when possible)
        let display_path = file_path
            .strip_prefix("file://")
            .unwrap_or(file_path)
            .to_string();

        for sym in &file_data.symbols {
            let mut tokens = Vec::new();

            // Name tokens
            tokens.extend(tokenize_identifier(&sym.name));

            // Signature tokens
            if !sym.detail.is_empty() {
                tokens.extend(tokenize_identifier(&sym.detail));
            }

            // Documentation tokens
            if let Some(ref doc) = sym.documentation {
                tokens.extend(tokenize_query(doc));
            }

            // Return type tokens
            if let Some(ref rt) = sym.return_type {
                tokens.extend(tokenize_identifier(rt));
            }

            // Parameter type tokens
            for (_, ptype) in &sym.parameters {
                tokens.extend(tokenize_identifier(ptype));
            }

            let doc_id = tfidf.docs.len();
            tfidf.docs.push(SearchDoc {
                name: sym.name.clone(),
                kind: format!("{:?}", sym.kind).to_lowercase(),
                file: display_path.clone(),
                line: sym.selection_range.start.line + 1,
                signature: sym.detail.clone(),
                doc: sym.documentation.clone(),
                tokens: tokens.clone(),
            });
            tfidf.add(doc_id, &tokens);
        }
    }

    tfidf.finalize();
    let results = tfidf.search(&all_query_tokens, max_results);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&results).expect("serialize JSON")
        );
    } else {
        if results.is_empty() {
            println!("No symbols found matching '{}'", query);
        } else {
            println!("{} results for '{}':", results.len(), query);
            for r in &results {
                print!("  {:.0}%  {} {}", r.score * 100.0, r.kind, r.name);
                if !r.signature.is_empty() {
                    print!(" — {}", r.signature);
                }
                println!();
                println!("         @ {}:{}", r.file, r.line);
                if let Some(ref doc) = r.doc {
                    // Show first ~100 chars of doc
                    let preview: String = doc.chars().take(100).collect();
                    let suffix = if doc.len() > 100 { "..." } else { "" };
                    println!("         {}{}", preview, suffix);
                }
            }
        }
    }
}

// ── test helpers ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_camelcase() {
        let tokens = tokenize_identifier("LoginViewModel");
        assert_eq!(tokens, vec!["login", "view", "model"]);
    }

    #[test]
    fn test_tokenize_snake_case() {
        let tokens = tokenize_identifier("user_repository_impl");
        assert_eq!(tokens, vec!["user", "repository", "impl"]);
    }

    #[test]
    fn test_tokenize_single_word() {
        let tokens = tokenize_identifier("Token");
        assert_eq!(tokens, vec!["token"]);
    }

    #[test]
    fn test_tokenize_acronym() {
        let tokens = tokenize_identifier("parseJSON");
        assert_eq!(tokens, vec!["parse", "json"]);
    }

    #[test]
    fn test_tokenize_query() {
        let tokens = tokenize_query("find where token is refreshed");
        assert_eq!(tokens, vec!["find", "where", "token", "is", "refreshed"]);
    }

    #[test]
    fn test_tokenize_query_with_punctuation() {
        let tokens = tokenize_query("find: where? token... refreshed!");
        assert_eq!(tokens, vec!["find", "where", "token", "refreshed"]);
    }

    #[test]
    fn test_tfidf_empty() {
        let tfidf = TfIdfIndex::new();
        let results = tfidf.search(&["token".to_string()], 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_tfidf_single_doc() {
        let mut tfidf = TfIdfIndex::new();
        let tokens = tokenize_identifier("TokenRefreshUseCase");
        tfidf.docs.push(SearchDoc {
            name: "TokenRefreshUseCase".into(),
            kind: "class".into(),
            file: "test.kt".into(),
            line: 10,
            signature: String::new(),
            doc: None,
            tokens: tokens.clone(),
        });
        tfidf.add(0, &tokens);
        tfidf.finalize();

        let results = tfidf.search(&["token".to_string()], 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "TokenRefreshUseCase");
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn test_tfidf_no_match() {
        let mut tfidf = TfIdfIndex::new();
        let tokens = tokenize_identifier("LoginViewModel");
        tfidf.docs.push(SearchDoc {
            name: "LoginViewModel".into(),
            kind: "class".into(),
            file: "test.kt".into(),
            line: 10,
            signature: String::new(),
            doc: None,
            tokens: tokens.clone(),
        });
        tfidf.add(0, &tokens);
        tfidf.finalize();

        let results = tfidf.search(&["xyz".to_string()], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_tfidf_prefix_match() {
        let mut tfidf = TfIdfIndex::new();
        let tokens = tokenize_identifier("TokenRefreshUseCase");
        tfidf.docs.push(SearchDoc {
            name: "TokenRefreshUseCase".into(),
            kind: "class".into(),
            file: "test.kt".into(),
            line: 10,
            signature: String::new(),
            doc: None,
            tokens: tokens.clone(),
        });
        tfidf.add(0, &tokens);
        tfidf.finalize();

        // "tok" is a prefix of "token" — should still match via prefix expansion
        let results = tfidf.search(&["tok".to_string()], 5);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_tfidf_ranking() {
        let mut tfidf = TfIdfIndex::new();

        let t1 = tokenize_identifier("TokenRefreshUseCase");
        tfidf.docs.push(SearchDoc {
            name: "TokenRefreshUseCase".into(),
            kind: "class".into(),
            file: "a.kt".into(),
            line: 1,
            signature: String::new(),
            doc: Some("Refreshes the auth token".into()),
            tokens: {
                let mut t = t1.clone();
                t.extend(tokenize_query("Refreshes the auth token"));
                t
            },
        });
        tfidf.add(0, &{
            let mut t = t1.clone();
            t.extend(tokenize_query("Refreshes the auth token"));
            t
        });

        let t2 = tokenize_identifier("LoginViewModel");
        tfidf.docs.push(SearchDoc {
            name: "LoginViewModel".into(),
            kind: "class".into(),
            file: "b.kt".into(),
            line: 5,
            signature: String::new(),
            doc: Some("Handles login screen state".into()),
            tokens: {
                let mut t = t2.clone();
                t.extend(tokenize_query("Handles login screen state"));
                t
            },
        });
        tfidf.add(1, &{
            let mut t = t2.clone();
            t.extend(tokenize_query("Handles login screen state"));
            t
        });

        tfidf.finalize();

        // "token" should rank TokenRefreshUseCase higher
        let results = tfidf.search(&["token".to_string()], 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "TokenRefreshUseCase");
    }
}
