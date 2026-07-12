//! Android resource graph — manifest activities, composable functions.
use serde::Serialize;
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

pub(crate) fn run_android_composables(file: &Path, json: bool) {
    let composables = find_composables(file);
    if json {
        println!("{}", serde_json::to_string_pretty(&composables).unwrap());
    } else {
        for c in &composables {
            println!("  {}({}) @ line {}", c.name, c.params.join(", "), c.line);
        }
    }
}

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

fn find_composables(file: &Path) -> Vec<ComposableInfo> {
    let mut c = Vec::new();
    let Ok(src) = std::fs::read_to_string(file) else {
        return c;
    };
    let mut p = tree_sitter::Parser::new();
    p.set_language(&tree_sitter_kotlin_sg::LANGUAGE.into()).ok();
    let Some(t) = p.parse(&src, None) else {
        return c;
    };
    let r = t.root_node();
    let mut s = vec![r];
    while let Some(n) = s.pop() {
        if n.kind() == "function_declaration" && has_annot(&n, &src, "Composable") {
            let nm = first_id(&n, &src);
            let ln = n.start_position().row as u32 + 1;
            c.push(ComposableInfo {
                name: nm,
                line: ln,
                params: extract_params(&n, &src),
            });
        }
        let mut cur = n.walk();
        for ch in n.children(&mut cur) {
            s.push(ch);
        }
    }
    c
}

fn has_annot(node: &tree_sitter::Node, src: &str, a: &str) -> bool {
    for ch in children(node) {
        if (ch.kind() == "modifiers" || ch.kind() == "annotation")
            && src[ch.start_byte()..ch.end_byte()].contains(a)
        {
            return true;
        }
    }
    false
}

fn first_id(node: &tree_sitter::Node, src: &str) -> String {
    for ch in children(node) {
        if ch.kind() == "simple_identifier" {
            return ch.utf8_text(src.as_bytes()).unwrap_or("").into();
        }
    }
    String::new()
}

fn extract_params(node: &tree_sitter::Node, src: &str) -> Vec<String> {
    for ch in children(node) {
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

fn children<'a>(node: &tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
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
}
