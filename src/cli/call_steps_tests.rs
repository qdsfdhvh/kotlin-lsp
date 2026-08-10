//! Tests for the Kotlin function-body step extractor (src/cli/call_steps.rs).

use super::call_steps::{extract_functions, CallStep, FuncInfo};

fn func<'a>(functions: &'a [FuncInfo], key: &str) -> &'a FuncInfo {
    functions
        .iter()
        .find(|f| f.key == key)
        .unwrap_or_else(|| panic!("function {key} not extracted"))
}

fn call_labels(steps: &[CallStep]) -> Vec<String> {
    steps
        .iter()
        .map(|s| match s {
            CallStep::Call { key, .. } => format!("call:{key}"),
            CallStep::Branch { label, .. } => format!("branch:{label}"),
        })
        .collect()
}

// ── calls ────────────────────────────────────────────────────────────────────

#[test]
fn extracts_top_level_function_with_calls() {
    let src = "package demo\n\nfun entry(): String {\n    val x = helper()\n    return x\n}\n\nfun helper(): String = \"h\"\n";
    let functions = extract_functions(src);
    let entry = func(&functions, "entry");
    assert!(entry.exported, "top-level fun is exported");
    assert_eq!(entry.line, 3, "definition line is 1-based");
    let calls = call_labels(&entry.steps);
    assert_eq!(calls, vec!["call:helper"]);
}

#[test]
fn private_function_is_not_exported() {
    let src = "class C {\n    private fun hidden(): Int = 1\n    fun shown(): Int = hidden()\n}\n";
    let functions = extract_functions(src);
    assert!(
        !func(&functions, "C.hidden").exported,
        "private method not exported"
    );
    assert!(
        func(&functions, "C.shown").exported,
        "public method exported"
    );
}

// ── branches ─────────────────────────────────────────────────────────────────

#[test]
fn if_else_branches_with_condition_labels() {
    let src =
        "fun pick(x: Int): String {\n    if (x > 0) { return left() } else { return right() }\n}\n";
    let functions = extract_functions(src);
    let steps = &func(&functions, "pick").steps;
    assert_eq!(steps.len(), 2, "if + else are two branch steps");
    let CallStep::Branch {
        label, children, ..
    } = &steps[0]
    else {
        panic!("expected branch step");
    };
    assert_eq!(label, "if x > 0");
    assert_eq!(call_labels(children), vec!["call:left"]);
    let CallStep::Branch {
        label, children, ..
    } = &steps[1]
    else {
        panic!("expected else branch");
    };
    assert_eq!(label, "else");
    assert_eq!(call_labels(children), vec!["call:right"]);
}

#[test]
fn else_if_chain_flattens() {
    let src =
        "fun grade(n: Int): String {\n    if (n > 90) \"a\" else if (n > 80) \"b\" else \"c\"\n}\n";
    let functions = extract_functions(src);
    let labels = call_labels(&func(&functions, "grade").steps);
    assert_eq!(
        labels,
        vec!["branch:if n > 90", "branch:else if n > 80", "branch:else"]
    );
}

#[test]
fn try_catch_finally_branches() {
    let src = "fun risky(): Int {\n    try { explode() } catch (e: Exception) { recover() } finally { cleanup() }\n}\n";
    let functions = extract_functions(src);
    let labels = call_labels(&func(&functions, "risky").steps);
    assert_eq!(
        labels,
        vec!["branch:try", "branch:catch Exception", "branch:finally"]
    );
    let CallStep::Branch { children, .. } = &func(&functions, "risky").steps[0] else {
        panic!("try branch");
    };
    assert_eq!(call_labels(children), vec!["call:explode"]);
}

#[test]
fn when_entries_become_case_branches() {
    let src = "fun describe(x: Int): String {\n    return when (x) {\n        1 -> one()\n        else -> other()\n    }\n}\n";
    let functions = extract_functions(src);
    let labels = call_labels(&func(&functions, "describe").steps);
    assert_eq!(labels, vec!["branch:case 1", "branch:else"]);
}

// ── lambda attribution ───────────────────────────────────────────────────────

#[test]
fn lambda_bodies_not_attributed_to_outer() {
    // Calls inside a lambda belong to no named function (calldiff CONTRACT):
    // `helper` must NOT appear among `outer`'s steps.
    let src = "fun outer(list: List<Int>) {\n    list.forEach { item -> helper(item) }\n}\n\nfun helper(x: Int) = x\n";
    let functions = extract_functions(src);
    let outer = func(&functions, "outer");
    // forEach is a call on `list` → navigation key `list.forEach`; the lambda
    // body call to `helper` is dropped.
    let labels = call_labels(&outer.steps);
    assert_eq!(labels, vec!["call:list.forEach"]);
}

// ── class / object methods ───────────────────────────────────────────────────

#[test]
fn class_methods_get_class_qualified_keys() {
    let src = "class Service {\n    fun start() { boot() }\n    fun stop() {}\n}\n";
    let functions = extract_functions(src);
    assert!(func(&functions, "Service.start").exported);
    assert_eq!(func(&functions, "Service.start").line, 2);
    let start = func(&functions, "Service.start");
    assert_eq!(call_labels(&start.steps), vec!["call:boot"]);
    assert!(
        functions.iter().all(|f| f.key != "start"),
        "no bare method key"
    );
}

#[test]
fn companion_object_methods_use_parent_class() {
    let src = "class Api {\n    companion object {\n        fun create(): Api = Api()\n    }\n}\n";
    let functions = extract_functions(src);
    assert!(
        func(&functions, "Api.create").exported,
        "companion method uses parent class key"
    );
}

// ── navigation / receiver calls ──────────────────────────────────────────────

#[test]
fn receiver_calls_use_navigation_keys() {
    let src = "class Holder {\n    fun run() { config.load() }\n    val config = Config()\n}\n";
    let functions = extract_functions(src);
    let run = func(&functions, "Holder.run");
    let labels = call_labels(&run.steps);
    assert_eq!(
        labels,
        vec!["call:config.load"],
        "navigation expression key"
    );
}
