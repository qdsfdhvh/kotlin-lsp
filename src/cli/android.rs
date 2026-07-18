//! Android resource graph — manifest activities and @Composable analysis.

use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Serialize)]
struct ActivityInfo {
    name: String,
    exported: bool,
    intent_filters: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ComposableInfo {
    name: String,
    line: u32,
    params: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    calls: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    state_vars: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<PreviewInfo>,
}

#[derive(Debug, Serialize)]
struct PreviewInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    width_dp: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height_dp: Option<u32>,
}

pub(crate) fn run_android_activities(root: &Path, json: bool) {
    let activities = find_manifest_activities(root);
    if json {
        println!("{}", serde_json::to_string_pretty(&activities).unwrap());
    } else {
        for a in &activities {
            println!(
                "  {} ({})",
                a.name,
                if a.exported { "exported" } else { "not" }
            );
        }
    }
}

pub(crate) fn run_android_composables(
    file: &Path,
    json: bool,
    call_graph: bool,
    state: bool,
    preview: bool,
) {
    let composables = find_composables(file, call_graph, state, preview);
    if json {
        println!("{}", serde_json::to_string_pretty(&composables).unwrap());
    } else {
        for c in &composables {
            print!("  {} ({}) @ line {}", c.name, c.params.join(", "), c.line);
            if let Some(ref p) = c.preview {
                if let Some(ref n) = p.name {
                    print!(" [name={n}]");
                }
                if let Some(w) = p.width_dp {
                    print!(" [w={w}dp]");
                }
                if let Some(h) = p.height_dp {
                    print!(" [h={h}dp]");
                }
            }
            println!();
            if !c.calls.is_empty() {
                println!("    → calls: {}", c.calls.join(", "));
            }
            if !c.state_vars.is_empty() {
                println!("    → state: {}", c.state_vars.join(", "));
            }
        }
    }
}

// ── Manifest parsing ─────────────────────────────────────────────────────

fn find_manifest_activities(root: &Path) -> Vec<ActivityInfo> {
    let mut activities = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                activities.extend(find_manifest_activities(&path));
            } else if path.file_name().and_then(|n| n.to_str()) == Some("AndroidManifest.xml") {
                if let Ok(c) = std::fs::read_to_string(&path) {
                    activities.extend(parse_manifest(&c));
                }
            }
        }
    }
    activities
}

fn parse_manifest(xml: &str) -> Vec<ActivityInfo> {
    let mut acts = Vec::new();
    let mut in_act = false;
    let mut cur = String::new();
    let mut exp = false;
    let mut filters = Vec::new();
    let mut in_filt = false;
    let mut action = String::new();
    for line in xml.lines() {
        let t = line.trim();
        if t.contains("<activity ") {
            in_act = true;
            cur = extract_attr(t, "android:name");
            exp = !t.contains("android:exported=\"false\"");
        } else if in_act && (t.starts_with("</activity>") || t.contains("/>")) {
            acts.push(ActivityInfo {
                name: cur.clone(),
                exported: exp,
                intent_filters: filters.clone(),
            });
            in_act = false;
        } else if in_act && t.contains("<intent-filter") {
            in_filt = true;
        } else if in_act && t.starts_with("</intent-filter>") {
            in_filt = false;
            if !action.is_empty() {
                filters.push(action.clone());
                action.clear();
            }
        } else if in_filt && t.contains("<action ") {
            action = extract_attr(t, "android:name");
        }
    }
    acts
}

fn extract_attr(line: &str, attr: &str) -> String {
    let p = format!("{}=\"", attr);
    if let Some(s) = line.find(&p) {
        let r = &line[s + p.len()..];
        if let Some(e) = r.find('"') {
            return r[..e].to_string();
        }
    }
    String::new()
}

// ── Composable analysis — inline tree-sitter walk, no cross-fn Node refs ──

fn find_composables(
    file: &Path,
    call_graph: bool,
    state: bool,
    preview: bool,
) -> Vec<ComposableInfo> {
    let Ok(src) = std::fs::read_to_string(file) else {
        return vec![];
    };
    let mut p = tree_sitter::Parser::new();
    p.set_language(&tree_sitter_kotlin_sg::LANGUAGE.into()).ok();
    let Some(tree) = p.parse(&src, None) else {
        return vec![];
    };
    let root = tree.root_node();

    // Step 1: find all @Composable functions — collect name + node
    let mut fns: Vec<(String, tree_sitter::Node)> = Vec::new();
    {
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if n.kind() == "function_declaration" && node_text_contains(&n, &src, "@Composable") {
                let name = first_child_ident(&n, &src);
                fns.push((name, n));
            }
            let mut c = n.walk();
            for ch in n.children(&mut c) {
                stack.push(ch);
            }
        }
    }

    let names: HashSet<String> = fns.iter().map(|(n, _)| n.clone()).collect();

    // Step 2: for each function, inline-analyze its body
    let mut result = Vec::new();
    for (name, node) in fns {
        let line = node.start_position().row as u32 + 1;
        let params: Vec<String> = extract_params_text(&node, &src);
        let mut calls: Vec<String> = Vec::new();
        let mut state_vars: Vec<String> = Vec::new();
        let mut prev: Option<PreviewInfo> = None;

        // Find function body
        let mut fc = node.walk();
        let body = node
            .children(&mut fc)
            .find(|ch| ch.kind() == "function_body");

        if call_graph {
            if let Some(b) = body {
                let mut seen = HashSet::new();
                let mut stack = vec![b];
                while let Some(nn) = stack.pop() {
                    if nn.kind() == "call_expression" {
                        let callee = first_child_ident(&nn, &src);
                        if !callee.is_empty()
                            && names.contains(&callee)
                            && seen.insert(callee.clone())
                        {
                            calls.push(callee);
                        }
                    }
                    let mut c = nn.walk();
                    for ch in nn.children(&mut c) {
                        stack.push(ch);
                    }
                }
                calls.sort();
            }
        }

        if state {
            // Detect: var/val x by remember|mutableStateOf|derivedStateOf
            let fn_text = node_text(&node, &src);
            for line_text in fn_text.lines() {
                if line_text.contains("by ")
                    && (line_text.contains("mutableStateOf")
                        || line_text.contains("remember")
                        || line_text.contains("derivedStateOf"))
                {
                    // Extract var name: "var|val NAME by ..."
                    let trimmed = line_text.trim();
                    if let Some(rest) = trimmed
                        .strip_prefix("var ")
                        .or_else(|| trimmed.strip_prefix("val "))
                    {
                        if let Some(ident) = rest.split(&[':', '=', ' ']).next() {
                            if !ident.is_empty() && !ident.starts_with("by") {
                                state_vars.push(ident.to_string());
                            }
                        }
                    }
                }
            }
            state_vars.sort();
            state_vars.dedup();
        }

        if preview {
            // Check modifiers for @Preview
            for ch in node_children(&node) {
                if ch.kind() == "modifiers" || ch.kind() == "annotation" {
                    let t = node_text(&ch, &src);
                    if t.contains("@Preview") {
                        prev = Some(parse_preview_params(&src, ch.start_byte(), ch.end_byte()));
                    }
                }
            }
        }

        result.push(ComposableInfo {
            name,
            line,
            params,
            calls,
            state_vars,
            preview: prev,
        });
    }
    result
}

fn parse_preview_params(src: &str, start_byte: usize, end_byte: usize) -> PreviewInfo {
    let text = &src[start_byte..end_byte];
    let mut info = PreviewInfo {
        name: None,
        width_dp: None,
        height_dp: None,
    };
    for part in text.split(&[',', '(', ')']) {
        let kv: Vec<&str> = part.splitn(2, '=').collect();
        if kv.len() != 2 {
            continue;
        }
        match kv[0].trim() {
            "name" => info.name = Some(kv[1].trim().trim_matches('"').to_string()),
            "widthDp" => info.width_dp = kv[1].trim().parse().ok(),
            "heightDp" => info.height_dp = kv[1].trim().parse().ok(),
            _ => {}
        }
    }
    info
}

// ── Tiny helpers (no Node refs across calls, only values) ─────────────────

fn node_text(node: &tree_sitter::Node, src: &str) -> String {
    src[node.start_byte()..node.end_byte()].to_string()
}

fn node_text_contains(node: &tree_sitter::Node, src: &str, s: &str) -> bool {
    src[node.start_byte()..node.end_byte()].contains(s)
}

fn first_child_ident(node: &tree_sitter::Node, src: &str) -> String {
    for ch in node_children(node) {
        if ch.kind() == "simple_identifier" {
            return ch.utf8_text(src.as_bytes()).unwrap_or("").to_string();
        }
    }
    String::new()
}

fn extract_params_text(node: &tree_sitter::Node, src: &str) -> Vec<String> {
    for ch in node_children(node) {
        if ch.kind() == "function_value_parameters" {
            return ch
                .children(&mut ch.walk())
                .filter(|c| c.kind() == "simple_identifier")
                .map(|c| c.utf8_text(src.as_bytes()).unwrap_or("").to_string())
                .collect();
        }
    }
    vec![]
}

fn node_children<'a>(node: &tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut c = node.walk();
    node.children(&mut c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parse_activity() {
        let xml = "<activity android:name=\".MainActivity\" android:exported=\"true\">\n<intent-filter>\n<action android:name=\"android.intent.action.MAIN\"/>\n</intent-filter>\n</activity>";
        let acts = parse_manifest(xml);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].name, ".MainActivity");
        assert!(acts[0].exported);
    }

    #[test]
    fn find_composables_basic() {
        let code = "@Composable\nfun Greeting(name: String) { Text(\"Hello\") }";
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.kt"), code).unwrap();
        let composables = find_composables(&dir.path().join("test.kt"), false, false, false);
        assert_eq!(composables.len(), 1);
        assert_eq!(composables[0].name, "Greeting");
    }

    #[test]
    fn call_graph_detects_internal_calls() {
        let code =
            "@Composable fun A() { B(); }\n@Composable fun B() { C(); }\n@Composable fun C() {}";
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.kt"), code).unwrap();
        let composables = find_composables(&dir.path().join("test.kt"), true, false, false);
        let a = composables.iter().find(|c| c.name == "A").unwrap();
        assert!(a.calls.contains(&"B".to_string()), "A should call B");
        let b = composables.iter().find(|c| c.name == "B").unwrap();
        assert!(b.calls.contains(&"C".to_string()), "B should call C");
    }

    #[test]
    fn state_detection() {
        let code = "import androidx.compose.runtime.*\n@Composable\nfun Counter() {\n    var count by remember { mutableStateOf(0) }\n}";
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.kt"), code).unwrap();
        let composables = find_composables(&dir.path().join("test.kt"), false, true, false);
        assert_eq!(composables.len(), 1);
        assert!(composables[0].state_vars.contains(&"count".to_string()));
    }

    #[test]
    fn preview_extraction() {
        let code = "import androidx.compose.ui.tooling.preview.Preview\n@Preview(name = \"Light\")\n@Composable fun P() {}";
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.kt"), code).unwrap();
        let composables = find_composables(&dir.path().join("test.kt"), false, false, true);
        let p = composables[0].preview.as_ref().unwrap();
        assert_eq!(p.name.as_deref(), Some("Light"));
    }
}
