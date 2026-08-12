//! Semantic symbol search — `search "query"` returns TF-IDF ranked results.
//!
//! Indexes symbol names, documentation (KDoc), signatures, and return types
//! from the workspace index. Tokenizes on camelCase, snake_case, and whitespace.
//! No external ML dependencies — pure TF-IDF with BM25-inspired scoring.

/// Common English stop words to exclude from search tokens.
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "in", "on", "at", "to", "for",
    "of", "by", "with", "from", "and", "or", "not", "but", "if", "then", "else", "when", "it",
    "its", "this", "that", "these", "those", "do", "does", "did", "has", "have", "had", "can",
    "could", "will", "would", "shall", "should", "may", "might", "must", "i", "you", "he", "she",
    "we", "they", "me", "him", "her", "us", "them", "my", "your", "his", "our", "their", "what",
    "which", "who", "whom", "where", "how", "why", "all", "any", "both", "each", "few", "more",
    "most", "no", "some", "such", "only", "own", "same", "so", "than", "too", "very", "just",
    "about", "into", "over", "also", "new", "get", "got", "set",
];
/// Simple suffix-stripping stemmer for English tokens.
/// Handles common suffixes: -ed, -ing, -s, -es, -tion, -ment, -ly.
fn stem(word: &str) -> String {
    let w = word.to_lowercase();
    if w.len() <= 3 {
        return w;
    }
    // Strip double-letter + ed/ing (e.g., "running" → "run", "stopped" → "stop")
    if w.ends_with("ing") && w.len() >= 6 && w.len() <= 9 {
        let base = &w[..w.len() - 3];
        let chars: Vec<char> = base.chars().collect();
        let n = chars.len();
        if n >= 2 && chars[n - 1] == chars[n - 2] {
            return base[..n - 1].to_string();
        }
        return base.to_string();
    }
    if w.ends_with("ed") && w.len() > 4 {
        let base = &w[..w.len() - 2];
        let chars: Vec<char> = base.chars().collect();
        let n = chars.len();
        if n >= 2 && chars[n - 1] == chars[n - 2] {
            return base[..n - 1].to_string();
        }
        return base.to_string();
    }
    if w.ends_with('s') && !w.ends_with("ss") && w.len() >= 6 && w.len() <= 9 {
        let base = &w[..w.len() - 1];
        if base.ends_with("e") && base.len() > 2 {
            return base[..base.len() - 1].to_string(); // "tokens" → "token" (strip "es")
        }
        return base.to_string(); // "models" → "model"
    }
    if w.ends_with("tion") && w.len() >= 6 && w.len() <= 9 {
        return format!("{}e", &w[..w.len() - 4]); // "resolution" → "resolute"
    }
    if w.ends_with("ment") && w.len() >= 6 && w.len() <= 9 {
        return w[..w.len() - 4].to_string(); // "refreshment" → "refresh"
    }
    if w.ends_with("ly") && w.len() > 4 {
        return w[..w.len() - 2].to_string(); // "quickly" → "quick"
    }
    // "ies" → "y" (e.g., "dependencies" → "dependency")
    if w.ends_with("ies") && w.len() > 4 {
        return format!("{}y", &w[..w.len() - 3]);
    }
    w
}
use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

// ── tokenization ────────────────────────────────────────────────────────────

/// Split an identifier into lowercase word segments.
///
/// Mirrors codegraph's `splitIdentifierSegments` semantics (src/search/
/// identifier-segments.ts): camelCase/PascalCase humps ("LoginViewModel" →
/// login/view/model), acronym runs ("HTMLParser" → html/parser), any
/// non-alphanumeric separates ("user_repository_impl" → user/repository/impl),
/// and digits stay glued to their word ("base64Encode" → base64/encode).
/// Segment bounds (2–32 chars, 12 segments per identifier) keep minified or
/// degenerate names from bloating the search index; digit-only segments are
/// dropped because they carry no prose signal.
fn tokenize_identifier(s: &str) -> Vec<String> {
    const MIN_SEGMENT_CHARS: usize = 2;
    const MAX_SEGMENT_CHARS: usize = 32;
    const MAX_SEGMENTS_PER_NAME: usize = 12;

    fn is_alnum(c: char) -> bool {
        c.is_alphabetic() || c.is_numeric()
    }
    fn push_segment(out: &mut Vec<String>, seg: &[char]) {
        if seg.is_empty()
            || seg.len() < MIN_SEGMENT_CHARS
            || seg.len() > MAX_SEGMENT_CHARS
            || seg.iter().all(|c| c.is_numeric())
        {
            return;
        }
        out.push(seg.iter().flat_map(|c| c.to_lowercase()).collect());
    }

    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // Non-alphanumerics separate runs (codegraph `[\p{L}\p{N}]+`).
        if !is_alnum(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && is_alnum(chars[i]) {
            i += 1;
        }
        let run = &chars[start..i];

        // Split the run on camelCase humps (lower/digit → Upper) and on the
        // last Upper of an acronym run when a lowercase follows ("HTMLParser"
        // → HTML | Parser). Both are the codegraph split points, written out
        // without lookbehind (Rust regex has none).
        let mut seg_start = 0;
        for j in 1..run.len() {
            let camel_hump =
                run[j].is_uppercase() && (run[j - 1].is_lowercase() || run[j - 1].is_numeric());
            let acronym_end = run[j - 1].is_uppercase()
                && run[j].is_uppercase()
                && j + 1 < run.len()
                && run[j + 1].is_lowercase();
            if camel_hump || acronym_end {
                push_segment(&mut out, &run[seg_start..j]);
                seg_start = j;
                if out.len() >= MAX_SEGMENTS_PER_NAME {
                    return out;
                }
            }
        }
        push_segment(&mut out, &run[seg_start..]);
        if out.len() >= MAX_SEGMENTS_PER_NAME {
            return out;
        }
    }
    out
}

/// Tokenize free text: split on whitespace, strip punctuation, lowercase.
fn tokenize_query(text: &str) -> Vec<String> {
    let stops: std::collections::HashSet<&str> = STOP_WORDS.iter().copied().collect();
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| c.is_ascii_punctuation() && c != '_')
                .to_lowercase()
        })
        .filter(|w| !w.is_empty() && !stops.contains(w.as_str()))
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

/// Identifier-segment expansion of the raw query.
///
/// Names typed as-is ("HTMLParser") yield their segments (html/parser) so they
/// match indexed name segments. `tokenize_query` lowercases first, so the same
/// input would otherwise become a single "htmlparser" token that never matches
/// the "html"/"parser" segments a `HTMLParser` symbol indexes under.
/// Non-alphanumerics separate, so trailing punctuation is stripped for free.
fn query_segments(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in raw.split_whitespace() {
        for seg in tokenize_identifier(word) {
            if !out.contains(&seg) {
                out.push(seg);
            }
        }
    }
    out
}

pub(crate) async fn run_search(query: &str, json: bool, max_results: usize) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    let mut tfidf = TfIdfIndex::new();
    let query_tokens: Vec<String> = tokenize_query(query)
        .into_iter()
        .map(|t| stem(&t))
        .collect();

    // Query expansion: stemmed free-text tokens, their camelCase/snake_case
    // sub-segments, and segments of the raw query kept at original casing
    // (acronym names like "HTMLParser" only split before lowercasing).
    let mut all_query_tokens: Vec<String> = query_tokens.clone();
    for qt in &query_tokens {
        let sub = tokenize_identifier(qt);
        for s in sub {
            if !all_query_tokens.contains(&s) {
                all_query_tokens.push(s);
            }
        }
    }
    for seg in query_segments(query) {
        if !all_query_tokens.contains(&seg) {
            all_query_tokens.push(seg);
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

            // Stem all tokens for better matching (e.g., "refreshed" → "refresh")
            // Only stem query tokens, not document tokens.
            // Document tokens (code identifiers) are exact and should
            // match as-is; the query side stems user's free-text input.
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

    // ── acronym-run segmentation (codegraph identifier-segments parity) ──

    #[test]
    fn test_tokenize_acronym_run() {
        // "HTMLParser" must split into HTML | Parser, not stay one token.
        let tokens = tokenize_identifier("HTMLParser");
        assert_eq!(tokens, vec!["html", "parser"]);
    }

    #[test]
    fn test_tokenize_acronym_run_leading() {
        let tokens = tokenize_identifier("JSONParser");
        assert_eq!(tokens, vec!["json", "parser"]);
        let tokens = tokenize_identifier("XMLHttpRequest");
        assert_eq!(tokens, vec!["xml", "http", "request"]);
    }

    #[test]
    fn test_tokenize_digits_glued_to_word() {
        // Digits stay glued to their word: base64 | encode, foo2 | bar.
        let tokens = tokenize_identifier("base64Encode");
        assert_eq!(tokens, vec!["base64", "encode"]);
        let tokens = tokenize_identifier("Foo2Bar");
        assert_eq!(tokens, vec!["foo2", "bar"]);
    }

    #[test]
    fn test_tokenize_acronym_with_trailing_digits() {
        // Trailing digits glue to the last segment, not a separate token.
        let tokens = tokenize_identifier("HTMLParser2");
        assert_eq!(tokens, vec!["html", "parser2"]);
    }

    #[test]
    fn test_tokenize_segment_bounds() {
        // 2-char minimum: single-letter names carry no prose signal.
        let tokens = tokenize_identifier("a");
        assert_eq!(tokens, Vec::<String>::new());
        let tokens = tokenize_identifier("xy");
        assert_eq!(tokens, vec!["xy"]);
    }

    #[test]
    fn test_tokenize_digit_only_segment_dropped() {
        let tokens = tokenize_identifier("12345");
        assert_eq!(tokens, Vec::<String>::new());
    }

    #[test]
    fn test_tokenize_minified_identifier_dropped() {
        // Oversized single segments (minified names, hashes) are dropped.
        let tokens = tokenize_identifier("abcdefghijklmnopqrstuvwxyzabcdefghijklmnop");
        assert_eq!(tokens, Vec::<String>::new());
        // A hump after the oversized run still yields the short tail segment
        // (codegraph parity: "…3456789" + "ABCDEF" → ["abcdef"]).
        let tokens = tokenize_identifier("abcdefghijklmnopqrstuvwxyz0123456789ABCDEF");
        assert_eq!(tokens, vec!["abcdef"]);
    }

    #[test]
    fn test_tokenize_signature_punctuation() {
        // Non-alphanumerics separate: a signature tokenizes into words.
        let tokens =
            tokenize_identifier("fun addBiometryToPowerAuth(isAllowedForActiveOp: Boolean)");
        assert_eq!(
            tokens,
            vec![
                "fun", "add", "biometry", "to", "power", "auth", "is", "allowed", "for", "active",
                "op", "boolean"
            ]
        );
    }

    #[test]
    fn test_query_segments_acronym() {
        // Raw-casing expansion: "HTMLParser" must match html/parser segments.
        let segs = query_segments("HTMLParser");
        assert_eq!(segs, vec!["html", "parser"]);
    }

    #[test]
    fn test_query_segments_snake_and_punctuation() {
        let segs = query_segments("user_repository");
        assert_eq!(segs, vec!["user", "repository"]);
        // Trailing punctuation separates for free.
        let segs = query_segments("HTMLParser,");
        assert_eq!(segs, vec!["html", "parser"]);
    }

    #[test]
    fn test_tokenize_query() {
        let tokens = tokenize_query("find token refreshed");
        assert_eq!(tokens, vec!["find", "token", "refreshed"]);
    }

    #[test]
    fn test_tokenize_query_with_punctuation() {
        let tokens = tokenize_query("find: where? token... refreshed");
        assert_eq!(tokens, vec!["find", "token", "refreshed"]);
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
