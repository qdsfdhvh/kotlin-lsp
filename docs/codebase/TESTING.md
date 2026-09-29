# Testing

## Organization and execution

Rust tests use `cargo test`, standard assertions, real parsers/indexers, and
`tempfile` fixtures. Unit tests live in companion `*_tests.rs` files, connected
with `#[cfg(test)] #[path = "…_tests.rs"] mod tests;`. New process tests belong
in `tests/*_tests.rs`; extend existing integration files when strengthening an
existing case. Do not put test bodies in production modules.

For CLI contracts, run `CARGO_BIN_EXE_kotlin-lsp` with real args/stdin and assert
stdout, stderr, exit code, and meaningful results—not just successful launch.
`tests/batch_query_tests.rs::Fixture` isolates workspace, cwd, HOME/USERPROFILE
and XDG cache; `tests/check_input_tests.rs::check_command` and the existing
`tests/cli_commands.rs` check cases also set both HOME and USERPROFILE on CLI
children. Windows home discovery uses USERPROFILE, not HOME. Keep Cargo/Rustup
homes inherited and unchanged; never set global environment variables for these
fixtures. Reuse `target/`; `rg` and `fd` must be on PATH. Never modify the user's
library/cache to make a test pass. Query-engine tests supplement, rather than
replace, process coverage.

Definition/reference JSON asserts exact filesystem-valued file strings and fixed
line/column positions. Normalize expected canonical fixture paths through a
standard file-URI roundtrip (`expected_file_path`), since Windows canonicalization
adds a verbatim prefix that URI conversion removes. The helper's guard verifies
canonical filesystem identity with spaces, Unicode and URI-reserved characters;
HOME/USERPROFILE and inherited toolchain homes are guarded in existing cases.
Caller/subtype file fields remain URI-valued and are compared against exact file
URIs. Do not substitute basenames, suffixes, or URI strings for filesystem paths.

A regression should fail on the defect before its narrow fix. Coverage of
already-correct behavior can be green immediately; record that honestly. Use
fixed, independently known positions and positive/negative controls. A filtered
run executing zero tests is not evidence. Permission tests report applicability
with `--nocapture`; a permissions bypass is not exercised denial coverage.

## P1/P2 reliability gate

These are bounded check/batch contracts, not compiler-grade symbol resolution.
The following focused commands passed locally on **macOS, 2026-09-12**. The
original gate executed 86 tests. The test-only portability correction reran C1,
C2, C3 and C6 (56 tests, including one new path-identity guard), plus C8/C9.
C4/C5/C7 retain the prior gate's results and were not rerun for this correction.
Counts are not repository-wide totals. Each command had a 300-second timeout
and a complete external temporary log; the implementation report records those
paths, the initial formatting failure and subsequent passing check, and the
correction-only diff. Correction commands used the explicit stable toolchain
`/Users/seiko/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo`, its bin
first on PATH, `CARGO_HOME=/Users/seiko/.cargo` and
`RUSTUP_HOME=/Users/seiko/.rustup`.

| ID | Command | Executed / result |
|---|---|---|
| C1 | `cargo test --test check_input_tests -- --nocapture` | 15 passed; directory and regular-file permission denial assertions executed |
| C2 | `cargo test --test cli_commands check_` | 3 passed |
| C3 | `cargo test --test batch_query_tests` | 33 passed (32 original + 1 path-identity guard) |
| C4 | `cargo test --bin kotlin-lsp cli::query_engine_tests::` | 19 passed |
| C5 | `cargo test --bin kotlin-lsp cli::ref_kind::tests::` | 11 passed |
| C6 | `cargo test --bin kotlin-lsp args::tests::help_` | 5 passed |
| C7 | `cargo test --bin kotlin-lsp args::tests::capabilities_manifest_matches_help` | 1 passed |
| C8 | `cargo fmt --all -- --check` | passed |
| C9 | `cargo clippy --all-targets -- -D warnings` | passed, zero warnings |

Test names below are in the indicated integration file unless prefixed with
`engine:` (`src/cli/query_engine_tests.rs`) or `args:` (`src/cli/args_tests.rs`).
Each listed test contains observable assertions; the matrix groups related
contracts instead of enumerating every language × format combination.

| Behavior / risk | Representative tests | Run |
|---|---|---|
| Check missing, mixed valid/missing, empty directory distinction | `check_input_tests`: `check_mixed_missing_empty_dir_and_valid_distinguishes_each`, `check_missing_file_text_fails`, `check_empty_directory_json_lists_empty_dirs`, `check_empty_directory_text_notes_no_sources_and_exits_zero` | C1 |
| Syntax diagnostics and exit 1 agree in text/JSON; valid input stays accepted | `cli_commands`: `check_syntax_error_exits_one`, `check_json_output`, `check_valid_file_exits_zero` | C2 |
| Read failures, partial traversal, reachable files retained, failed scans not empty | `check_input_tests`: `check_invalid_utf8_preserves_valid_input_in_text_and_json`, `check_permission_failures_preserve_partial_scan_and_are_not_empty`, `check_unreadable_directory_reports_traversal_error` | C1 |
| Empty/missing diagnose inputs, text failures, missing operands, source-extension selection and empty source file | `check_input_tests`: `check_diagnose_empty_directory_json_reports_input_error`, `check_diagnose_missing_file_json_preserves_input_error`, `check_diagnose_text_input_failures_are_actionable`, `check_without_operands_is_an_argument_error`, `check_directory_selects_supported_extensions_and_counts_empty_source` | C1 |
| Definition vs CST refs; all supported filters; literal/comment exclusions; normal smart refs parity | `batch_query_tests`: `references_are_cst_usages_with_real_filters_and_normal_refs_parity`, `other_reference_kinds_discriminate_writes_overrides_and_types`, `references_include_interpolated_reads_but_not_literal_text`, `java_and_swift_calls_are_distinct_from_declarations_and_text` | C3 |
| Import-only discovery, package exclusion, aliases, same-spelling callee/receiver/argument identity | `batch_query_tests`: `import_only_files_are_discovered_but_package_tokens_are_excluded`, `explicit_import_filter_includes_kotlin_alias_only_in_import_context`, `same_spelling_receiver_callee_and_argument_keep_occurrence_identity` | C3 |
| Declarations vs initializer/body reads; compound/member writes vs receiver/index reads | `batch_query_tests`: `references_separate_parameter_and_variable_declarations_from_reads`, `reference_writes_include_kotlin_compound_assignments`, `reference_writes_include_java_assignments`, `assignment_lhs_nested_members_and_indices_preserve_evaluation_reads`; classifier regressions | C3, C5 |
| Swift binding declarations, interpolation/initializer reads, adjacent assignment boundaries | `batch_query_tests`: `swift_declarations_exclude_initializer_and_interpolation_reads`, `swift_adjacent_assignments_select_identifier_not_statement_boundary` | C3 |
| Raw malformed/non-array input, empty batch, invalid item shapes/types/filter/depth, ordered text/JSON failures | `batch_query_tests`: `malformed_or_non_array_stdin_fails_without_results`, `empty_batch_returns_an_empty_array`, `invalid_item_shapes_and_numeric_types_preserve_following_success`, `invalid_items_fail_without_losing_success_order`, `text_batch_preserves_order_and_item_failure_exit` | C3 |
| Explicit/relative/implicit root, nested cwd, absolute operands, invalid root, no-stdlib positive/negative control | `batch_query_tests`: `explicit_root_overrides_cwd_and_json_is_compact`, `file_operands_use_explicit_root_or_nested_cwd_not_discovered_ancestor`, `missing_explicit_root_fails_on_stderr_without_results`, `no_stdlib_excludes_fake_home_library` | C3 |
| UTF-16 starts/ends, nonASCII names, repeated occurrences, empty/punctuation/surrogate/cursor-end boundaries, unavailable/ambiguous hover | `batch_query_tests`: `references_return_multiple_occurrences_and_utf16_columns`, `relative_hover_and_callers_use_root_and_utf16_identifier_start`, `bad_file_or_position_returns_ordered_errors_not_fabricated_hover`, `hover_identifier_end_empty_and_surrogate_boundaries`, `ambiguous_hover_preserves_name_without_fabricating_signature`; engine: `references_unicode_names_have_exact_utf16_start_and_end_ranges` | C3, C4 |
| Real summarize/implementations/subclasses dispatch; complete cold/warm item parity including refs and cached metadata | `batch_query_tests`: `parsed_summary_and_subtype_dispatch_are_meaningful_and_cache_stable` (asserts persisted cache and fixed success/error schemas before parity) | C3 |
| Cross-file sorting/deduplication before existing 20/50 caps | `batch_query_tests`: `callers_are_deduplicated_and_sorted_across_files_before_twenty_cap`, `subtypes_are_sorted_across_files_before_fifty_cap` | C3 |
| Compact JSON; parser/help/generated capabilities agree | `batch_query_tests`: `explicit_root_overrides_cwd_and_json_is_compact`; args: `help_batch_query_contract_and_capabilities_flags`, help guardrails, `capabilities_manifest_matches_help` | C3, C6, C7 |

### Boundaries and remaining validation

- Linux/Windows execution is not claimed. The HOME/USERPROFILE and verbatim-path
  corrections address source-established Windows failure mechanisms; macOS green
  and portable assertion guards are not Windows execution or Windows RED evidence.
  Permission denial is conditional on Unix and actual access checks.
- Valid-file `check --diagnose --json` still emits check output followed by
  diagnose output. A single-document redesign is deferred; tests guarantee a
  single structured failure for empty/missing inputs only.
- The current Kotlin grammar rejects parenthesized assignment LHS in the probed
  form. Do not bless its erroneous classification with an expected-result test.
- Queries remain name-based: package/overload/source-set binding, resolving import
  aliases to targets, and recursive callers are outside this gate. Depth is 1
  or omitted. No P3 hierarchy or global cache migration is included.
- Full `cargo test`, formatter/JVM suites, and cross-platform execution belong to
  the final gate; focused results above do not imply they ran.

## P3 call hierarchy gate (macOS, 2026-09-12)

`cargo test --test call_hierarchy_tests` runs **19 process tests** with fixed
semantic JSON/text and exit/stderr assertions. Each child isolates workspace,
HOME, USERPROFILE and XDG_CACHE_HOME; RUST_LOG=error excludes timing-dependent
slow-parse logs without suppressing CLI diagnostics. The toolchain environment
is inherited. No snapshots generated from actual output are used.

| Behavior / risk | Tests in `tests/call_hierarchy_tests.rs` |
|---|---|
| Default real edges; declarations/comments/literals excluded | `entry_target_default_has_real_edges_not_declarations_or_text` |
| Default/incoming/outgoing/both, name and position forms | `directions_default_incoming_outgoing_and_both_select_edges` |
| UTF-16 call/declaration positions, explicit root vs decoy cwd, absolute/relative files | `name_and_utf16_position_equivalent_with_root_relative_and_absolute_files`, `utf16_declaration_after_emoji_matches_name_and_candidate_columns` |
| Nested cwd-relative operands; invalid roots | `implicit_root_preserves_nested_cwd_relative_files`, `invalid_root_is_reported_before_lookup` |
| Missing files/names, invalid/empty cursors, noncallable text and declaration identity | `invalid_files_cursors_and_noncallable_text_fail_honestly`, `position_never_substitutes_a_same_named_noncallable_or_unindexed_file` |
| Override method name/position equivalence; base method and same-name override-property controls | `override_method_position_matches_name_without_selecting_override_property` |
| Nested type nearest-owner key; bare/qualified/position lookup, real incoming edge, outer alias rejected | `nested_method_uses_nearest_owner_for_name_position_and_edges` |
| Same-name negative controls, exact candidate paths/UTF-16 columns, no overload body merging | `same_named_methods_are_not_merged_and_ambiguous_names_list_locations`, `same_name_files_filter_outgoing_and_unresolvable_overloads_fail` |
| Kotlin/Java/Swift basic paths; Unicode Swift names, exact text directions/errors | `kotlin_java_swift_direct_calls_have_positive_and_empty_controls`, `text_directions_and_unicode_names_are_exact_and_missing_name_fails` |
| Empty caller/callee, repeated calls, self-recursion/cycle termination, persisted cold/warm byte parity | `repeated_calls_cycles_and_actual_cold_warm_cache_are_deterministic` |
| Fake-home library enabled cold/warm and disabled positive/negative controls, warm library cursor | `no_stdlib_excludes_fake_home_callables_and_edges_with_positive_control` |
| Depth/extra operands rejected, directions scoped, help/generated capabilities | `hierarchy_rejects_depth_and_extra_operands_instead_of_ignoring_them`, `direction_flags_are_rejected_outside_hierarchy`, `help_and_generated_capabilities_advertise_real_hierarchy_boundaries` |

Focused regressions also passed: `cargo test --bin kotlin-lsp cli::reach_tests::`
(24), `cargo test --bin kotlin-lsp cli::call_diff_tests::` (30),
`cargo test --test cli_commands call_diff_` (2),
`cargo test --test batch_query_tests` (33),
`cargo test --bin kotlin-lsp args::tests::help_` (5), and
`cargo test --bin kotlin-lsp args::tests::capabilities_manifest_matches_help` (1).
These are 114 passing tests, not a full-suite or cross-platform claim.
The two review corrections each reproduced independently (one failing CLI test,
exit 101) before their hierarchy-local fix and then passed individually.
`cargo fmt --all -- --check` and all-target clippy with `-D warnings` also passed.

Hierarchy is direct/name-based and returns string graph keys, not locations or
compiler-resolved identities; see [the output contract](../commands.md#direct-call-hierarchy-call-hierarchy).
The same-file overload boundary fails explicitly. The bundled Kotlin grammar
rejects `fun 目标()` (confirmed by tree/check); Unicode-name coverage uses valid
Swift instead, while Kotlin UTF-16 positions around emoji have positive coverage.
Warm library restoration is hierarchy-local: existing cache data is decoded once,
only candidate files/relevant edges are expanded, and only candidate source lines
are loaded for byte-to-UTF-16 conversion. Shared cache schema and batch behavior
are unchanged. Full suite, ktlint/JVM and Windows/Linux runs remain final-gate work.

## P4a semantic search output gate (macOS, 2026-09-12)

All new tests use the real CLI with isolated workspace, HOME, USERPROFILE,
XDG_CACHE_HOME and Gradle home; each child has a 15-second timeout. They assert
stdout/stderr/exit and fixed fixture results, not private search mocks. Timing
noise alone is excluded with RUST_LOG=error. Search keeps its existing URI-tail
`file` representation; identity assertions round-trip escaping and canonical paths.

| Run | Command | Passed |
|---|---|---:|
| S1 | `cargo test --test search_output_tests` | 15 |
| E1 | `cargo test --test extract_sources_options_tests` | 3 |
| R1 | `cargo test --bin kotlin-lsp cli::search::` (includes corpus/ranking) | 38 |
| R2 | `cargo test --bin kotlin-lsp cli::extract_sources::tests::` | 5 |
| R3 | `cargo test --test check_input_tests --test batch_query_tests --test call_hierarchy_tests` | 15 + 33 + 19 |
| R4 | `cargo test --test cli_commands check_` (not the full target) | 3 |
| R5 | `cargo test --bin kotlin-lsp args::tests::` (also matches indexer argument tests) | 92 |
| R6 | `cargo test --bin kotlin-lsp args::tests::help_` | 5 |

| Behavior / risk | Tests (S1 unless noted) | Run |
|---|---|---|
| Exactly three scored/filter-only matches plus excluded class/function controls; omitted/0/1/3/4/MAX limits, no-hit at every limit, exact envelope keys/boolean, compact array equality and fixed fields/scores; shorthand/explicit and flag orders | `scored_limits_preserve_array_fields_scores_and_truthful_envelopes`, `filter_only_limits_preserve_array_fields_scores_and_truthful_envelopes`, `semantic_envelope_no_hit_is_compact_and_not_truncated` | S1 |
| Default cap remains 20 with 21 real matches, in both branches | `omitted_limit_remains_twenty_with_actual_overflow_in_both_branches` | S1 |
| Reject missing JSON, every nonsemantic search member (including cached summarize), other command families, negative/noninteger/overflow limits; no command output/index/edit work | `envelope_requires_json_for_both_semantic_spellings_before_indexing`, `envelope_rejects_every_nonsemantic_search_member_and_other_commands_before_work`, `invalid_search_limits_fail_before_indexing_in_array_and_envelope_modes` | S1 |
| Repeated fresh processes and actual cold/warm workspace caches; same-name cross-file ties; distinct-line same-file overload signatures | `scored_ties_have_fixed_cross_file_order_on_repeated_cold_and_warm_runs`, `same_file_distinct_signatures_and_cross_file_filter_ties_keep_fixed_order` | S1 |
| Multi-prefix floating sums with unequal term frequencies; non-top scores preserved under limits/envelopes; fixed rank plus byte parity | `matching_prefix_terms_have_stable_scores_across_processes_and_cache_states` | S1 |
| Real implementation before generated stub, generated-only symbol and path-filtered stub positive controls | `generated_real_before_stub_and_generated_only_remains_searchable` | S1 |
| Kotlin/Java/Swift meaningful language-filtered results, root B vs cwd A positive/negative controls, full escaped path identity | `kotlin_java_swift_search_filters_return_meaningful_results_with_decoys`, `semantic_explicit_root_selects_workspace_b_not_decoy_cwd_a`, `search_file_field_preserves_full_identity_and_existing_uri_escaping` | S1 |
| Real help/generated capability flag scope and invocable envelope example | `help_and_generated_capabilities_describe_semantic_envelope_and_extraction_options` | S1, R5, R6 |
| Extraction rejects root before announcements/writes; isolated real Gradle JAR, filtered no-hit and dry-run hit, actual source-byte extraction/non-source exclusion | `extract_sources_rejects_root_before_scanning_or_writing_with_actionable_options`, `extract_sources_isolated_dry_run_discovers_filtered_jar_without_writes`, `extract_sources_valid_options_extract_only_source_bytes_to_explicit_output` | E1, R2 |

Independent one-behavior RED→GREEN evidence was captured for the missing envelope,
cross-file ordering, prefix-sum nondeterminism, extraction root rejection and help
advertisement. `cargo fmt --all -- --check` and all-target clippy with denied
warnings pass. These are focused results, not full-suite or cross-platform claims.

**Known boundary:** an additional same-line overload probe
`fun matchToken(value: String) {}; fun matchToken(value: Int) {}` exposed an
existing parser detail limitation: both signatures report `String` because
`extract_detail` takes whole source lines and cuts at the first body. The parent
approved deferring it outside P4a; the exploratory failing test/source/log are
preserved in handoff evidence, not converted to an assertion blessing wrong
output. `src/parser.rs` is byte-identical to the accepted pre-P4a baseline.
Distinct-line overload assertions remain enforced; this is not exhaustive
signature or compiler-binding coverage. P4b query options and P4c workspace
forwarding, shared caches, broad library/no-stdlib coverage, full suite, ktlint/JVM,
and Windows/Linux execution remain separate gates.

## P4b compatible-cache prerequisite (macOS, 2026-09-13)

Historical cache/consumer recovery gate, **not by itself completed P4b option coverage**.
The overall P4b matrix and current focused results are recorded below.
All 17 original `cli_options_tests` cases remain, with complete enabled-cold vs
persisted enabled-warm JSON comparisons added to their independent assertions.
They initially ran 10 passed / 7 failed. The real accessor regression first
failed at native-key lookup, then independently at empty first-return lines.
Each consumer correction was checked separately. Stronger parity exposed an
additional indexed-find missing `kind`; its metadata read now uses `get_file`.
Review correction reproduced three qualified-hover false positives independently:
safe navigation, a call receiver, and whitespace after the dot each returned an
unrelated global declaration (3 failed / 3 passed before correction). A CLI-local
`navigation_suffix` role guard rejects only the selected member, preserving an
unqualified argument in `receiver.consume(Target())` (6 passed after correction).
The accessor fixture now compares canonical filesystem identity and uses
URI-decoded native persisted keys, matching the writer even when Windows
canonicalization adds verbatim prefixes. This is source-verified portability,
not a claimed Windows execution.

| Behavior / risk | Source-named tests |
|---|---|
| Real source-path index → nonempty full/compact caches → new lazy index; URI-decoded native keys vs escaped URIs (space/%/#/Unicode), full canonical filesystem identity (including Windows verbatim spelling), exact first-access lines/symbols, repeated access, unrelated file unmaterialized/unfilled, same-basename workspace retained, invalid URI and missing source graceful, filled lines preserved | `indexer::lazy_library_tests::get_file_native_cache_first_access_lines_and_identity` |
| Original root B vs cwd A controls; home library disabled-cold / enabled-cold / enabled-warm / disabled-after-warm; full output parity, exact source identity, workspace retained | All original 17 in `tests/cli_options_tests.rs` (context, impact, find-test, inspect, summaries, indexed find/refs/hover, hierarchy and expect-actual controls) |
| Find class/function/property kinds, escaped full native path/URI identity, home exclusion vs retained workspace.json nonhome external sources | `indexed_find_native_library_kind_parity_and_nonhome_source_selection` |
| Cached summary discovers all requested-name workspace/library files, preserves signature/doc and private exclusion; unknown ordinary summary fails | `cached_summary_discovers_all_requested_files_without_private_symbols` |
| Warm library ambiguity participates even when workspace already has a target; disabled mode remains positive | `indexed_hover_ambiguity_includes_warm_library_candidates` |
| Workspace reference equals declaration signature/doc after an emoji (UTF-16), repeated fresh processes | `tests/hover_reference_tests.rs::indexed_hover_workspace_reference_matches_declaration_signature_doc_and_utf16` |
| Ambiguous/no-match/qualified references rejected, unrelated global not substituted for receiver, comments/literals/punctuation/empty/out-of-range cursor rejected, unique reference and declaration positive controls | `tests/hover_reference_tests.rs::indexed_hover_reference_rejects_ambiguity_qualifiers_and_non_identifiers` |
| Safe-navigation, call-receiver, and spaced-dot member selections reject unrelated global declarations; unqualified argument in a member call remains positive, repeated fresh processes | `tests/hover_reference_tests.rs::indexed_hover_reference_rejects_{safe_navigation,call_receiver,spaced_member}_global_decoy` and `indexed_hover_reference_keeps_unqualified_argument_in_member_call` |

Focused gate commands after review correction (262 distinct passing tests):

- `cargo test --test cli_options_tests --test hover_reference_tests --test check_input_tests --test batch_query_tests --test call_hierarchy_tests --test search_output_tests --test extract_sources_options_tests`: **111** (20 + 6 + 15 + 33 + 19 + 15 + 3).
- `cargo test --bin kotlin-lsp -- indexer::lazy_library_tests:: indexer::cache::tests:: indexer::symbol_index::tests:: cli::summarize::tests:: cli::query_engine_tests:: cli::integration_tests::enrich_result_kinds_ cli::search:: cli::reach_tests:: cli::call_diff_tests:: cli::args::tests::help_ cli::args::tests::capabilities_manifest_matches_help`: **139**. Discovery confirmed each listed prefix has actual cases.
- `cargo test --bin kotlin-lsp -- backend::format::tests::hover_ backend::format::tests::contextual_hover_ indexer::tests::hover_`: **7**.
- `cargo test --test cli_commands -- check_ call_diff_`: **5**, not the whole target.
- `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`: pass.

Runs use isolated HOME/USERPROFILE/XDG/workspaces, stable cargo with the original
CARGO_HOME/RUSTUP_HOME, RUST_LOG=error for timing noise only, and a 240-second
outer process bound with full logs. Cache version/serialized keys, source
selection, cwd-relative file operands, P2/P3 exceptions and snapshot policy are
unchanged. No full-library source hydration/reparse was added. Hover's new
fallback parses only the requested cursor file if there is no live tree;
qualified/ambiguous reference fallback remains unresolved, and declaration
behavior is retained (not compiler-grade binding).

At this historical prerequisite gate, remaining work included the broader P4b
matrix/docs (now recorded below), P4c workspace forwarding, independent review,
full suite, ktlint/JVM, Windows/Linux execution,
and large-library performance. The previously recorded same-line parser
signature limitation remains out of scope. These focused results do not close
those gates.

## P4b Java hover safety correction (macOS, 2026-09-13)

The earlier 262-test gate missed Java selected-member roles. The bounded
correction adds **25 public CLI tests** to `tests/hover_reference_tests.rs`
(**31 total**, retaining all six Kotlin controls). Before the fix, 12 Java
negative cases returned the unrelated `class Target {}` with exit 0 instead of
no result; 19 controls passed. After the CLI-local CST guard, all 31 pass.
This closes that safety regression, **not the remaining P4b phase**.

Each new fixture runs public `check --json` and asserts one valid source file,
zero errors, then indexes real files into a nonempty persisted workspace cache.
Repeated hover processes assert exact exit 0 / compact signature JSON / empty
stderr for positives, and exact exit 1 / empty stdout / cursor-specific diagnostic
for negatives. Every negative also checks the actual global decoy declaration.
The Java/Swift snippets are accepted by the bundled grammars; this is not a
claim that undeclared receiver fixtures compile with a Java/Swift compiler.
No unsupported grammar form is converted into an expected wrong hover result.

| Language / CST role | Source-named tests in `hover_reference_tests.rs` |
|---|---|
| Java selected method with call receiver, whitespace-dot or explicit type arguments | `java_hover_rejects_call_receiver_method_global_decoy`, `java_hover_rejects_spaced_method_global_decoy`, `java_hover_rejects_generic_method_global_decoy` |
| Java selected field with call receiver or whitespace-dot | `java_hover_rejects_call_receiver_field_global_decoy`, `java_hover_rejects_spaced_field_global_decoy` |
| Java scoped type (whitespace/annotation) and scoped annotation name | `java_hover_rejects_spaced_qualified_type_global_decoy`, `java_hover_rejects_annotated_qualified_type_global_decoy`, `java_hover_rejects_scoped_annotation_global_decoy` |
| Java selected method reference, including type arguments | `java_hover_rejects_method_reference_global_decoy`, `java_hover_rejects_generic_method_reference_global_decoy` |
| Java qualified creation selects an inner type, including a generic base name | `java_hover_rejects_qualified_creation_type_global_decoy`, `java_hover_rejects_qualified_generic_creation_type_global_decoy` |
| Java unqualified argument/type and annotated creation stay eligible | `java_hover_keeps_unqualified_argument_type_in_member_call`, `java_hover_keeps_unqualified_type`, `java_hover_keeps_unqualified_annotated_creation_type`, `java_hover_keeps_qualified_creation_argument_type` |
| Java receiver identities are not selected children; constructor reference `Target::new` is a type use | `java_hover_keeps_method_receiver_identifier`, `java_hover_keeps_field_receiver_identifier`, `java_hover_keeps_method_reference_receiver_identifier`, `java_hover_keeps_qualified_type_receiver_identifier`, `java_hover_keeps_constructor_reference_type` |
| Java nested type arguments are not the selected member | `java_hover_keeps_generic_member_type_argument`, `java_hover_keeps_method_reference_type_argument` |
| Kotlin retained safe-navigation/call-receiver/spaced-dot negatives and nested argument positive; ambiguity, invalid cursor, doc and UTF-16 controls | The six existing `indexed_hover_*` tests above; member negatives now assert the exact exit and diagnostic |
| Swift actual ordinary/call-receiver selected-member negatives vs global decoy; unqualified nested argument and declaration positives | `swift_hover_rejects_selected_member_global_decoy`, `swift_hover_keeps_unqualified_argument_and_declaration` |

The defensive guard uses selected `name`/`field` identity plus `object` presence
where Java supplies fields. Fieldless scoped types/method references use the
final child; qualified construction checks only its `type` (with one generic-base
wrapper), not its arguments or arbitrary ancestors. Shared resolution, cache,
parser, schema, declaration output and option routing are unchanged. Existing
help/command/skill wording already promises conservative qualified rejection;
this correction restores that contract without changing its surface.

The same focused commands listed in the prerequisite gate were rerun with a
240-second bound and full logs: **136 integration + 139 units + 7 hover units +
5 legacy-filter tests = 287 distinct passes**. The original 17 CLI option cases
and previous three additions remain unchanged (20 passed). Fmt and all-target
clippy with denied warnings pass. This is macOS execution only; independent
review, broader P4b coverage/docs, full-suite/JVM, Windows/Linux and large-library
performance remain separate gates.

## Broader validation

Run `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`
with focused suites. Run the full suite at the final acceptance gate. Coverage
inspection is available through `./coverage.sh [FILE|--all]`; a behavior matrix
is not a claim of exhaustive grammar coverage or a coverage percentage.

Other source-named suites cover parsing, resolution, indexing, and search;
`src/cli/search_corpus_tests.rs` uses a fixed OkHttp/Okio-shaped corpus through
the production path. Prefer real fixtures and fresh indexers over injected maps
when the contract involves parsing or CLI dispatch.

CI is defined in [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml):
Linux/macOS/Windows jobs run rustfmt, all-target/all-feature clippy with denied
warnings, and all-feature tests. Release packaging is separate in
[`.github/workflows/release.yml`](../../.github/workflows/release.yml). Check
those workflows for current triggers and platforms rather than relying on
historical test counts or release target totals.

## P4b overall query-option matrix (macOS, 2026-09-13)

The accepted Java/cache prerequisite is retained, not reimplemented. The current
options target has **29 real CLI tests**: all original 17, the three accepted
prerequisite additions, and nine completion tests. The table maps commands to
observable behavior, not parser acceptance or a capability group's flag union.
Every name below is in `tests/cli_options_tests.rs` unless otherwise identified.

| Command | Behavior → source-named tests | Gate |
|---|---|---|
| `context` | Index B vs cwd A/implicit A: `context_root_selects_b_not_cwd_a`; home inclusion/exclusion and full cold/warm parity: `context_no_stdlib_cold_warm` | Q1 |
| `impact` | Root: `impact_root_selects_b_not_cwd_a`; home cursor enabled/disabled with workspace-scoped callers: `impact_no_stdlib_cold_warm` | Q1 |
| `search find-test` | Root: `find_test_root_selects_b_not_cwd_a`; home symbol cursor vs workspace test discovery: `find_test_no_stdlib_cold_warm` | Q1 |
| `search summarize` | Root: `summarize_root_selects_b_not_cwd_a`; home enabled/disabled and complete output parity: `summarize_no_stdlib_cold_warm` | Q1 |
| `search summarize --cached` | Root: `summarize_cached_root_selects_b_not_cwd_a`; home cycles: `summarize_cached_no_stdlib_cold_warm`; all requested-name files/signature/doc, private/no-match exclusions: `cached_summary_discovers_all_requested_files_without_private_symbols` | Q1 |
| `search expect-actual` | Root: `expect_actual_root_selects_b_not_cwd_a`; workspace expect/actual identities, home-cache exclusion/inclusion and intentionally workspace-scoped home-name no-hit: `expect_actual_no_stdlib_preserves_workspace_rg_scope_and_skips_home_cache` | Q1 |
| `type hierarchy` | Root: `type_hierarchy_root_selects_b_not_cwd_a`; unchanged home exclusion both flags/cold/warm, independently positive indexed home definition: `type_hierarchy_preserves_home_library_exclusion_even_after_enabled_index` | Q1 |
| Indexed `find` | Home cycles: `indexed_find_no_stdlib_cold_warm`; class/function/property kind parity, escaped full identities and configured nonhome retention: `indexed_find_native_library_kind_parity_and_nonhome_source_selection` | Q1 |
| Indexed `refs` | `indexed_refs_no_stdlib_cold_warm`: workspace rg call locations preserved; home-cache creation excluded when disabled (not a claim of library-only reference enumeration) | Q1 |
| Indexed `hover` | Home use-site target cycles: `indexed_hover_no_stdlib_cold_warm`; ambiguity includes warm candidates: `indexed_hover_ambiguity_includes_warm_library_candidates`; all 31 accepted `hover_reference_tests` retain UTF-16, declaration/reference, Kotlin/Java/Swift source-role and false-global controls | Q1, Q2 |
| `tool inspect` | Home file symbols vs disabled empty symbol list, full parity: `inspect_no_stdlib_cold_warm` | Q1 |
| File-oriented context/impact/find-test/hover/inspect | `alternate_index_root_does_not_rebase_same_named_relative_file_operands`: relative A operand is never substituted with B's same-named file; absolute B positive identities. Outside-index context/impact/find-test fail, inspect has no indexed symbols; hover can index the actual A file on demand | Q1 |
| Rootless file/name queries | `rootless_queries_discover_ancestor_from_nested_cwd_without_rebasing_files`: nested cwd relative file, ancestor definitions/callers/tests and name-query summary/cached/expect-actual/hierarchy identities | Q1 |
| Selected P4b queries | `selected_queries_reject_missing_and_file_roots_before_results`: all eleven entry paths reject both root errors, empty stdout and actionable stderr; `selected_file_queries_reject_unreadable_operands_without_panic_or_results`: relative/absolute missing files, directory and invalid UTF-8 for the five file consumers | Q1 |
| Selected cursor and required-argument paths | `selected_cursor_queries_reject_invalid_positions_and_arguments`: zero/noninteger positions, out-of-range lines/columns, empty file and missing operands/names; actual errors and no panic/false results | Q1 |
| Fast/indexed find/refs/hover | `find_refs_hover_modes_respect_index_precondition_and_fast_workspace_scope`: no-index smart errors, automatic/fast B identities, fast home negatives, indexed library positive, precise call refs, declaration hover, explicit fast hover error | Q1 |
| Representative language/UTF-16 paths | `query_options_preserve_kotlin_java_swift_files_and_utf16_positions`: valid grammar check, exact declaration identities after an emoji for context, summary/inspect values and full cold/warm parity in all three languages; Kotlin impact/find-test UTF-16 positives | Q1 |
| Already-wired semantic search/docs/imports/annotated/module packages/sealed/reach/complete | `already_wired_queries_keep_representative_root_and_no_stdlib_controls`: exact B results with A negatives where applicable, package dependency, two-node reach, local completion positive/home negative; no home cache. This is a bounded representative control, not expanded command semantics or the whole completion suite | Q1 |
| Help/parser/generated metadata | `cli::args::tests::help_query_option_scope_and_generated_flags_match_parser`, existing `help_*` and `capabilities_manifest_matches_help`; manual real `--help`/`capabilities --json` plus changed-command smoke | Q3 |

Final focused gates (**297 distinct tests passed**, excluding repeated development
runs; no ignored cases):

- **Q1**: `cargo test --test cli_options_tests` — **29 passed** (included in combined Q1/Q2 below).
- **Q1/Q2**: `cargo test --test cli_options_tests --test hover_reference_tests --test check_input_tests --test batch_query_tests --test call_hierarchy_tests --test search_output_tests --test extract_sources_options_tests -- --nocapture` — **145 passed** (29 + 31 + 15 + 33 + 19 + 15 + 3).
- **Q3**: `cargo test --bin kotlin-lsp -- indexer::lazy_library_tests:: indexer::cache::tests:: indexer::symbol_index::tests:: cli::summarize::tests:: cli::query_engine_tests:: cli::integration_tests::enrich_result_kinds_ cli::search:: cli::reach_tests:: cli::call_diff_tests:: cli::args::tests::help_ cli::args::tests::capabilities_manifest_matches_help` — **140 passed**; every selected prefix matched actual tests.
- **Q4**: `cargo test --bin kotlin-lsp -- backend::format::tests::hover_ backend::format::tests::contextual_hover_ indexer::tests::hover_` — **7 passed**.
- **Q5**: `cargo test --test cli_commands -- check_ call_diff_` — **5 passed**, not the full target.
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings` — pass.

Completion exposed three narrow selected-command validation defects, each
public CLI RED→GREEN: invalid roots lacked root-specific errors; a missing
relative operand panicked at URI conversion; oversized context columns falsely
returned the line's last symbol. Dispatch now rejects these inputs before index
work. File-base/source defaults, shared cache/resolver/parser/schema, accepted
hover guards and all original 20 option assertions remain unchanged. Metadata
coverage also went RED→GREEN for missing `--no-stdlib` advertisements. Existing
hover error wording is retained. No source-less diagnostic accuracy or compiler
binding promise is added; summaries and related JSON still pretty-print.

Processes use temporary HOME/USERPROFILE/XDG/workspaces; Cargo/Rustup homes stay
explicitly preserved with the absolute stable Cargo binary and 240-second bounds.
Manual smoke verifies 14 real invocations with exact results/errors and generated
flags. Full logs, before hashes including accepted untracked files, completion-only
diff and the per-test gate mapping are retained in the local recovery evidence.
Fresh independent overall P4b review is still required. P4c forwarding, full-suite/
ktlint/JVM, Windows/Linux runs, large-library performance and the known same-line
signature limitation remain outside this focused result.

## P4c workspace options and remaining audit disposition (macOS, 2026-09-13)

This is the bounded original P4 audit's workspace slice, not a new global audit.
`tests/workspace_options_tests.rs` runs isolated real children with independent
A/B projects, identical module names, distinct dependencies/symbols/manifests,
full filesystem identities (including spaces, literal `%`/`#` and Unicode), and
HOME + USERPROFILE + XDG_CACHE_HOME per child. URI fields are decoded separately;
snapshot's legacy escaped file field is checked as such, not silently redefined.
Legacy unordered arrays are compared semantically, not globally sorted by the CLI.

**W1:** `cargo test --test workspace_options_tests` — **21 passed**, zero ignored.
Every row below ran in W1 (one test per listed name); the total is 21, not the
number of child invocations. Module discovery retains Kotlin/Java file extensions;
parse-backed graph/snapshot have Kotlin/Java/Swift controls. Workspace overview
retains its existing lightweight line-based extraction. No new grammar semantics.

| Behavior / risk | Test name in `tests/workspace_options_tests.rs` | Command / result |
|---|---|---|
| Module list: B modules, full paths, dependencies, source sets/counts; explicit A and rootless A controls | `module_list_root_selects_all_b_metadata_not_cwd_a` | W1 / 21 pass |
| Module deps: exact B dependencies/dependents, A negative/control | `module_deps_root_selects_b_edges_not_a` | W1 / 21 pass |
| Module files: complete B paths, A negative/control, existing extension scope | `module_files_root_selects_b_full_paths_not_a` | W1 / 21 pass |
| Graph: exact symbols/calls/inheritance and nested modules, A controls, cold/warm | `tool_graph_root_selects_b_symbols_edges_and_modules_not_a` | W1 / 21 pass |
| Workspace: project root, modules/counts, symbol files, entry points, A controls | `tool_workspace_root_selects_b_project_modules_symbols_and_entry_points` | W1 / 21 pass |
| Snapshot: project/modules, all symbol identities, calls/extends/imports/overrides and entry points, A controls, cold/warm | `tool_snapshot_root_selects_b_project_modules_symbols_relationships` | W1 / 21 pass |
| Activities: exact B manifest activity/export/filter fields, A controls | `android_activities_root_selects_b_manifest_not_a` | W1 / 21 pass |
| All seven missing/file roots, JSON and text: failure before false output | `all_workspace_commands_reject_missing_and_file_roots_before_results` | W1 / 21 pass |
| Absolute/relative roots, nested cwd: B throughout all seven outputs | `explicit_relative_roots_from_nested_cwd_govern_all_seven_operations` | W1 / 21 pass |
| Rootless Gradle-only ancestor module discovery and existing nested .git discovery | `rootless_nested_discovery_preserves_gradle_ancestors_and_git_workspace_defaults` | W1 / 21 pass |
| Explicit empty root does not fall back to A or Gradle ancestor; exact JSON shapes | `explicit_empty_directory_never_falls_back_to_cwd_or_gradle_ancestor` | W1 / 21 pass |
| Snapshot default/default-warm/include-cold/include-warm/default-after-include: library metadata and default workspace relationships; persisted cache controls | `snapshot_default_and_include_libraries_cold_warm_and_default_after_include` | W1 / 21 pass |
| Snapshot configured external survives default home exclusion, selected B config, cold/warm cycles | `snapshot_configured_external_included_but_home_excluded_by_default_cold_warm` | W1 / 21 pass |
| Library function signature, parameters, return type, KDoc, fq_name/line; relationships default vs omitted; final home-negative controls | `snapshot_exclude_relationships_preserves_selected_symbol_metadata_cold_warm` | W1 / 21 pass |
| Graph/snapshot Kotlin/Java/Swift symbols and file identities; module file scope unchanged | `selected_workspace_graph_and_snapshot_keep_kotlin_java_swift_symbols` | W1 / 21 pass |
| Seven text-mode smokes with semantic assertions; snapshot still always JSON | `workspace_text_shapes_and_seven_command_smokes_are_semantic` | W1 / 21 pass |
| Tree/composables/ordinary check/plain insert: cwd-relative same-named file; unrelated missing root accepted; preview bytes and zero writes | `file_only_tree_composables_check_and_insert_keep_cwd_operands` | W1 / 21 pass |
| Format cwd-only expansion with B-only operand; failure before external tools (empty child PATH), no ktlint/JVM | `format_input_expansion_remains_cwd_relative_without_launching_ktlint` | W1 / 21 pass |
| Isolated help/capabilities/embedded skill: root applicability, utility boundaries, format has no advertised root | `help_capabilities_and_skills_truthfully_bound_workspace_options_without_workspace` | W1 / 21 pass |
| Home calls/extends/overrides/imports excluded with library metadata still present cold/warm; similarly prefixed nonhome workspace edges remain; default-after-include and exclude-relationships | `snapshot_home_relationships_are_excluded_for_all_edge_kinds_cold_and_warm` | W1 / 21 pass |
| Graph retains existing cold configured/nonconfigured-home inclusion; root B uses B config not A; no eager library expansion added | `graph_keeps_existing_cold_source_inclusion_and_uses_selected_root_configuration` | W1 / 21 pass |

Each of the seven root commands has separate public CLI RED→GREEN evidence.
Invalid roots and surface documentation also went RED→GREEN. Snapshot's first
include succeeded cold, but the second include omitted the library symbol warm:
the consumer now discovers only its already-selected compact library URIs and
uses the accepted `get_file` accessor. Default never requests home sources.
A subsequent real library-call fixture proved cold-only home-edge leakage because
`is_library_path` compared file URLs as native paths. The parent explicitly
approved a second snapshot-local correction: URL decoding plus symmetric
filesystem identity normalization in that helper. Public CLI RED→GREEN covers
all four edge kinds (calls/extends/overrides/imports), retained workspace edges
under the similarly prefixed `sources-neighbor % # 库` directory, library metadata,
include cold/warm and subsequent default/exclude controls. The companion
`src/cli/snapshot_tests.rs::is_library_path_accepts_uri_and_native_inputs_without_prefix_overmatch`
retains native/URL inputs with platform-derived paths and no real-home mutation.
No shared index/cache/resolver/schema/version changes. Development logs also
retain fixture corrections (filesystem vs URI identity, annotation-start line,
plain insert's existing extra preview newline); these were not product fixes or
weakened accepted assertions. Other controls were green from first execution.

### Retained focused gates

All commands use `/Users/seiko/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo`,
that toolchain's bin first on PATH, preserved CARGO_HOME `/Users/seiko/.cargo` and
RUSTUP_HOME `/Users/seiko/.rustup`, **240-second bounds**, complete logs and exits.
These are local macOS results, not Windows/Linux/full-suite/performance claims.

| Gate | Exact Cargo arguments | Actual result |
|---|---|---|
| W2 | `test --test workspace_options_tests --test cli_options_tests --test hover_reference_tests --test check_input_tests --test batch_query_tests --test call_hierarchy_tests --test search_output_tests --test extract_sources_options_tests -- --nocapture` | 166 pass: 21 + retained 29/31/15/33/19/15/3 |
| W3 | `test --bin kotlin-lsp -- indexer::lazy_library_tests:: indexer::cache::tests:: indexer::symbol_index::tests:: cli::summarize::tests:: cli::query_engine_tests:: cli::integration_tests::enrich_result_kinds_ cli::search:: cli::reach_tests:: cli::call_diff_tests:: cli::args::tests::help_ cli::args::tests::capabilities_manifest_matches_help cli::modules::tests:: cli::workspace::tests:: cli::snapshot::tests:: cli::android::tests:: cli::run::tests::` | 180 pass; every prefix matched (module 6/workspace 2/snapshot 15/android 5/run 11) |
| W4 | `test --bin kotlin-lsp -- backend::format::tests::hover_ backend::format::tests::contextual_hover_ indexer::tests::hover_` | 7 pass |
| W5 | `test --test cli_commands -- check_ call_diff_` | 5 pass; not full cli_commands |
| W6 | `test --bin kotlin-lsp args::tests::` | 94 pass: 63 CLI args + 31 incidental indexer args; overlaps W3 |
| W7 | `fmt --all -- --check` and `clippy --all-targets -- -D warnings` | pass, zero warnings |

W2–W5 cover **358 distinct tests**; W6 adds 86 unique tests (444 distinct overall,
not including repeated W1/development runs). W1 and W2/W3 were rerun after the parent-approved home-relationship correction;
the isolated reproducer also changed from failed to passed. Permission-denial
coverage is retained in P1 with truthful platform guards; this slice adds no
permission-policy semantics. Full-suite/cli_complete/JVM/ktlint and large-library
performance remain final-gate work. Independent read-only review is still required.

### Review correction: selected nonhome external relationships

The initial 444-test gate did not prove external relationship completeness: the
configured-external fixture above contained only an edge-free class. Read-only
review and a separate parent CLI reproducer found all four external edge kinds
present cold but absent warm, despite complete symbols. The authorized single
correction supplements snapshot relationships from **already-materialized,
already-selected nonhome** library FileData, using cold-index identities and the
existing deduplication sets. It does not load additional sources or modify shared
index/cache/resolver/parser/schema code. Home relationships remain excluded even
with `--include-libraries`; its symbol-inclusion policy is unchanged. No CLI flags,
help, capability metadata, output shapes, or operand policies changed in this
correction, so the five existing surface sources remain unchanged.

| Behavior / risk | Test in `tests/workspace_options_tests.rs` | Exact Cargo arguments / actual result |
|---|---|---|
| Configured external calls/extends/overrides/imports cold and warm; repeated-call deduplication; full external metadata, B workspace and home-positive/negative controls; include cold/warm/default-after/include-exclude cycles | `snapshot_selected_external_relationships_survive_warm_cache_with_workspace_and_home_controls` | `test --test workspace_options_tests snapshot_selected_external_relationships_survive_warm_cache_with_workspace_and_home_controls -- --exact --nocapture` — RED: 1 failed (iteration 1/warm, missing external edges, symbols retained); GREEN: 1 passed |
| Selected Java/Swift external call and supertype identities, full symbol file identities and workspace controls cold/warm | `snapshot_selected_java_swift_external_calls_and_supertypes_are_cache_stable` | `test --test workspace_options_tests snapshot_selected_java_swift_external_calls_and_supertypes_are_cache_stable -- --exact --nocapture` — 1 passed |
| All original 21 workspace controls plus both correction cases, including isolated help/capabilities and all seven semantic command smokes | both tests above + all W1 rows | `test --test workspace_options_tests` — 23 passed, zero ignored |
| Retained P1–P4 integration controls | W2 exact arguments above | 168 passed: 23 workspace + retained 29/31/15/33/19/15/3 |
| Module/workspace/snapshot/android and retained selected units, help/capabilities | W3 exact arguments above | 180 passed |
| Retained hover and legacy check/call-diff controls | W4 and W5 exact arguments above | 7 and 5 passed |
| Args/help/capabilities, including incidental indexer args | W6 exact arguments above | 94 passed (8 overlap W3) |
| Formatting and warnings | W7 exact arguments above | fmt and all-target clippy passed, zero warnings |

These correction regates cover **446 distinct tests** (168 + 180 + 7 + 5 + 94 − 8),
not repeated development runs. All used the same direct stable toolchain and
240-second bounds described above. The correction artifact's `ledger.jsonl`
retains every RED/GREEN/gate command, full logs, exits, and real test counts;
`correction-only.diff` and before/after hashes separate these three changed files
from all accepted prerequisite work. Fresh parent-owned read-only recheck remains
required. macOS only; no full-suite, Windows/Linux, JVM/ktlint, or performance claim.

### Original P4 audit disposition

| Original audit family | Bounded disposition |
|---|---|
| Blanket all-command root / every-index no-stdlib promises; group flag unions | P4a corrected blanket claims; P4b query table and P4c workspace/file-only table delimit real support. Format's root metadata removed, not rejected by parser. No new flags/aliases. |
| Search compact array, envelope, limits and deterministic ranking | P4a implemented and retained 15 process controls; same-line overload signature limitation remains outside this slice. |
| extract-sources root advertisement | P4a rejects root with gradle-home/output guidance; retained 3 process controls. |
| context/impact/summarize/cached/find-test/expect-actual/type hierarchy ignored root | P4b implemented; retained 29 options + 31 hover controls. |
| Indexed find/refs/hover/inspect no-stdlib omissions | P4b implemented; no-stdlib excludes canonical home sources, not configured nonhome external sources. |
| module list/deps/files; graph/workspace/snapshot; activities root omissions | P4c wired through nested metadata and validated only these seven explicit roots; W1. |
| Snapshot inclusion ambiguity | Existing include-libraries policy preserved independently of no-stdlib; selected library metadata warm restoration, home-URI edge exclusion and review correction of selected nonhome external relationships are snapshot-local, W1 plus correction regates. No mixed-flag precedence invented. |
| Already-wired tool query / call hierarchy | P2/P3 accepted controls retained (33/19 tests); explicit root-relative operand exceptions unchanged. |
| Semantic/docs/imports/annotated/cache-stats/module packages/sealed/reach/complete | Existing wiring preserved; representative P4b controls and relevant query/reach units rerun. This is not every combination of group flags. |
| index/gradle-deps/index-jars/sources/cache/doctor/bench/call diff | Existing source-proven policy retained: positional index-jars root precedence; global cache list vs workspace cache operations; Git-specific call diff. No new universal discovery. |
| tokens --resolve/check --diagnose/code-action/bench workspace-only defaults | Existing home-excluding index policy retained. Root affects only existing indexing/containment, not ordinary file expansion. |
| tree/composables/format/ordinary check and file-only edits | Cwd-relative file operands retained; representative non-JVM/preview controls in W1. Edit inject/other edit indexing and containment unchanged; edit safety redesign remains P5. |
| capabilities/tool skills | No workspace operation; isolated W1 controls, no new root rejection. |

Evidence is the P4c-only diff, before/after hashes (including accepted untracked
prerequisites), full RED/GREEN/gate ledger and machine-readable behavior matrix in
the implementation artifact. Planning and final acceptance remain parent-owned.

## P5 shared-engine edit safety (macOS, 2026-09-13)

Tests cross `apply_file_edits`/preview and real argv→stdout/stderr/exit→file bytes.
The private generic interleaving seam changes **real files** at prepare/commit and
pre-replacement boundaries; it neither mocks the writer/parser nor introduces
production test flags. The five old inline edit tests moved intact to
`src/cli/edit_tests.rs` (bare unwraps replaced with reasons). This slice adds
**42 tests**: 28 edit API controls, two semantic-insert controls, 12 public CLI
controls. Full selected gates below run **535 distinct tests**, not a full suite.

### Behavior → test → command/result matrix

`E1` is `cargo test --offline --bin kotlin-lsp -- cli::edit::tests:: cli::insert::tests:: cli::integration_tests::`
(**80 pass**: 33 edit + 17 insert + 30 integration). API names below are in
`src/cli/edit_tests.rs` unless another file is specified.
`E2` is `cargo test --offline --test edit_safety_tests` (**12 pass**, also in E3).

| Behavior/risk | Source-named tests | Gate / actual result |
|---|---|---|
| Every-file preflight: missing second file without/with root; invalid UTF-8, directory/read failure; invalid root/outside target | `preflight_missing_second_file_never_writes_first`, `invalid_utf8_and_directory_second_file_preflight_no_writes`, `outside_root_and_invalid_root_preflight_no_writes`, `permissions_preserved_and_unreadable_second_file_preflights` | E1 / 80 pass |
| Reversed/overflow/out-of-range lines/columns, overlap, surrogate-interior; preview errors agree | `invalid_ranges_preflight_entire_batch`, `overlaps_and_surrogate_interiors_are_errors_in_preview_and_apply` | E1 / 80 pass |
| UTF-16 non-ASCII/emoji in Kotlin/Java/Swift files; untouched LF/CRLF/mixed/lone CR, verbatim multiline replacement | `utf16_replacement_preserves_untouched_bytes_and_verbatim_new_text`, `adjacent_replacements_equal_inserts_and_multiline_order` | E1 / 80 pass |
| Empty/EOF inserts, requested removal/addition of final newline; adjacent edits and equal insert order | `empty_eof_insert_and_final_newline_are_requested_only`, `adjacent_replacements_equal_inserts_and_multiline_order` | E1 / 80 pass |
| Duplicate canonical targets and symlink aliases rejected before writes | `duplicate_canonical_targets_rejected_before_writes`, `duplicate_symlink_alias_preflight_rejects_without_editing_target` | E1 / 80 pass |
| Positive three-file batch; byte-exact preview parity; dry-run prospective counts, zero temp/content/permission/mtime/identity changes; noops | `valid_batch_preview_apply_and_dry_run_counts_match_bytes`, `noop_and_dry_run_preserve_identity_permissions_and_no_temporaries` | E1 / 80 pass |
| Content conflicts before first write, including second-file change; between writes; after temp creation; accurate partial/unattempted report | `content_conflict_before_first_commit_is_not_overwritten`, `second_file_conflict_before_first_commit_is_globally_rechecked`, `content_conflict_between_files_reports_partial_and_unattempted`, `content_change_after_temporary_write_is_rechecked_and_cleaned` | E1 / 80 pass |
| Atomic replacement rather than truncation; same-byte new identity rejected; original permissions retained | `atomic_replace_does_not_truncate_existing_open_identity`, `same_bytes_target_replacement_detects_new_identity`, `permissions_preserved_and_unreadable_second_file_preflights` | E1 / 80 pass |
| In-root symlink keeps link inode; outside blocked; target, link, parent and root retarget detection | `symlink_inside_target_preserves_link_and_outside_is_blocked`, `symlink_retarget_and_same_target_link_replacement_are_detected`, `parent_and_root_symlink_retargets_before_commit_are_detected` | E1 / 80 pass |
| Unique temps, occupied-name create-new rejection, sentinel preservation and ordinary cleanup | `unique_temporary_names_preserve_preexisting_sentinels`, `replaced_temporary_path_never_commits_foreign_sentinel`, `content_change_after_temporary_write_is_rechecked_and_cleaned` | E1 / 80 pass |
| Late directory move and temp→symlink substitution: no foreign overwrite/delete, explicit possible retained own temp | `late_directory_move_never_deletes_foreign_sentinel_reports_retained_temp`, `temporary_symlink_to_moved_own_file_is_not_committed_or_deleted` | E1 / 80 pass |
| Real temp creation denial after a first successful write; real atomic-replace denial; cleanup failure reported | `temporary_creation_failure_after_first_write_reports_partial`, `replace_failure_reports_zero_writes_and_cleanup_failure_honestly` | E1 / 80 pass |
| Semantic dispatcher no whole-file newline normalization or invalid whole-file EOF range | `src/cli/insert_tests.rs::semantic_insert_preview_apply_preserves_crlf_and_no_final_newline`, `semantic_insertion_empty_and_unterminated_eof_are_real_ranges` | E1 / 80 pass |
| CLI rename Unicode length, multi-file positive apply, Swift dry-run bytes/permissions, readonly failure summary/exit, missing operand no panic | `tests/edit_safety_tests.rs::rename_unicode_identifier_uses_utf16_and_preserves_mixed_bytes`, `rename_two_files_valid_batch_and_invalid_utf8_operand_fail_honestly`, `rename_dry_run_prospective_counts_no_bytes_or_permission_change`, `rename_readonly_reports_failure_exit_and_untouched_bytes`, `rename_missing_operand_fails_without_panic` | E2 / 12 pass |
| CLI Kotlin multiple equal imports and Java Unicode preview/apply; explicit CRLF retention, readonly failure and invalid UTF-8 | `imports_equal_position_preview_matches_apply_and_retains_endings`, `imports_java_preview_and_apply_are_unicode_safe`, `imports_readonly_failure_exit_and_json_preview_dry_run`, `rename_two_files_valid_batch_and_invalid_utf8_operand_fail_honestly` in `tests/edit_safety_tests.rs` | E2 / 12 pass |
| CLI code-action uses 1-based cursor, byte-preserving apply, readonly failure report, invalid/missing position and root escape rejection | `code_action_apply_uses_one_based_cursor_and_preserves_crlf`, `code_action_readonly_failure_retains_summary_and_untouched_bytes`, `code_action_invalid_position_and_missing_operand_fail_without_panic`, `code_action_outside_root_fails_without_writing` in `tests/edit_safety_tests.rs` | E2 / 12 pass |

### Retained gates and evidence

All use direct `/Users/seiko/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo`,
that bin first on PATH, CARGO_HOME `/Users/seiko/.cargo`, RUSTUP_HOME
`/Users/seiko/.rustup`, **240-second bounds** and complete stdout/stderr/exit logs.
Final gate processes and CLI children use disposable HOME **and** USERPROFILE,
XDG cache/config/data and workspace directories. `RUST_LOG=error` suppresses only
logging noise. No tool/dependency installation, full suite, full cli_commands,
cli_complete, ktlint/JVM or release binary installation was run.

| Gate | Exact Cargo arguments | Actual result |
|---|---|---|
| E1 | as above | 80 pass |
| E3 | `test --offline --test edit_safety_tests --test workspace_options_tests --test cli_options_tests --test hover_reference_tests --test check_input_tests --test batch_query_tests --test call_hierarchy_tests --test search_output_tests --test extract_sources_options_tests` | 180 pass: 12 + retained 23/29/31/15/33/19/15/3 |
| E4 | P4c W3 exact arguments above with `--offline` | 180 pass; 3 overlap E1 |
| E5 | P4c W4 arguments above with `--offline` | 7 pass |
| E6 | `test --offline --test cli_commands -- check_ call_diff_` | 5 pass |
| E7 | `test --offline --bin kotlin-lsp args::tests::` | 94 pass; 8 overlap E4 |
| E8 | `fmt --all -- --check`; `clippy --offline --all-targets -- -D warnings` | pass, zero warnings |

Independent failing assertions were recorded before fixes for preflight, Unicode
byte slicing, duplicate targets, atomicity, content conflict, temp replacement,
temp symlink substitution, semantic-insert EOF/newlines, rename UTF-16 length,
failed-write exit status, code-action cursor off-by-one, and missing rename panic.
The initial pure-CJK Kotlin rename probe instead failed in **reference discovery**
(`no references found`), outside the edit engine; it is retained as an unresolved
probe, not counted as green or silently treated as supported. Accented `café`
provides the reachable CLI UTF-16 regression. API tests cover all three languages.

Unix symlink/mode tests are honestly gated. Directory denial tests verify the
runner actually enforces permissions (privileged runners report the condition
inapplicable); this macOS run exercised the denial branches. Windows portable
identity assertions compare filesystem identities rather than verbatim path
spellings. Windows/Linux have **not** run. Locked `same-file`/`tempfile` were reused;
parent-approved checksum-verified read-only winapi-util source inspection confirms
OpenOptions retains FILE_SHARE_DELETE. This is not Windows runtime validation;
ReFS/file-ID limitations remain. No deterministic disk-full `write_all` injection
or crash-durability/ACL claim; actual create-temp/replace failures are covered.

The engine deliberately may retain a temporary file if directory/temp identity
changes or cleanup is denied; reports say so. Check-to-rename/unlink races remain.
Only shared-engine routes inherit this contract (see commands/architecture docs).
Semantic-insert flat parser branches remain unregistered; plain grouped insert
and unrelated batch/format writers are not migrated. Planning/acceptance and
fresh read-only review remain parent-owned. P5-only diff, prerequisite hashes,
case matrix, exact RED/GREEN/gate logs and manual byte smokes accompany the report.

## Cross-platform repair validation (PR #335)

Keep local verification isolated: set temporary `HOME`, `USERPROFILE`, and
`XDG_CACHE_HOME`/`XDG_CONFIG_HOME`/`XDG_DATA_HOME`, retain the real `CARGO_HOME`
and `RUSTUP_HOME`, and clear inherited `GIT_*` variables. Capture the actual
Cargo exit code (not a `grep`/`head` pipeline's status), save complete logs, and
bound the entire test process group. A moved checkout may require rebuilding
package artifacts because integration binaries embed `CARGO_BIN_EXE` paths.
Do not move the user's real library sources to make tests pass.

The initial CI failures are tracked separately from the historical P5/P6
macOS receipts below. The repair coverage is:

- `rg::tests::split_fields_*`: native, verbatim drive, UNC and Unix rg output;
  drive colons must not discard references.
- `cli::query_engine_tests::references_resolve_mem_only_uris_without_native_paths`:
  references remain available from in-memory content without a native path.
- `path_identity_tests::alternate_root_spellings_keep_cold_and_warm_query_identity`:
  successive raw/canonical/dotted roots must retain exactly two symbols, one
  call edge and a working cursor lookup, including escaped filenames.
- Existing `cli_options_tests`, `workspace_options_tests`, `search_output_tests`
  and P6 fixtures retain exact results; URI-tail fields are not silently changed
  into native paths. Search's percent-encoding assertion is a compatibility gate.
- Existing `cli::edit::tests` and `edit_safety_tests` cover replacement, identity
  conflicts, partial failures and byte preservation. Destination identity handles
  stay alive through the final comparison, then are released before Windows
  replacement; directory/temp handles remain for guarded cleanup. The final
  check-to-rename race is still not eliminated.
- `batch_query_tests::file_explicit_root_fails_before_indexing_in_text_and_json`:
  existing-file roots fail before indexing/results in both output modes; the
  missing-root control remains separate.
- `install_script_tests`: hermetic GNU tar fixtures include `gzip` on PATH.

Run `cargo fmt --all -- --check`, `cargo test --all-features --no-fail-fast` and
`cargo clippy --all-targets --all-features -- -D warnings` under that isolation.
Only exact-head Ubuntu/macOS/Windows CI establishes cross-platform acceptance;
local macOS results and platform source inspection alone do not.

## P6 baseline and identity gate

macOS verification of this gate, 2026-09-17, development build. P6 makes
README/installer GitHub-Releases-only, aligns command aliases with the real
parser, replaces the old Rust benchmark harness with real-fixture CLI
baselines, and documents the call-graph identity boundary in
[GRAPH_IDENTITY.md](GRAPH_IDENTITY.md) — known lossy reach/snapshot behavior
stays documented there, never asserted as correct. Windows/Linux have **not**
run locally. The first push CI run (35167953203, head 043d097) failed exactly
where this gate's fixtures were representation-bound, and the fixes are fixture
scope only: Ubuntu GNU tar could not exec `gzip` from the closed fixture PATH
(2 installer tests), and Windows compared CLI-reported path spellings (`/C:/…`
URI tails, 8.3 `RUNNER~1` short names, percent-encoding) against native
spellings in the alias semantic test, the benchmark decoy/find controls and the
unique reach/graph/snapshot control. Assertions now decode every reported
representation to canonical filesystem identity while retaining URI-shape
checks, exact non-path fields, exact key sets, complete result sets and
wrong-workspace negatives. The same run's Windows production failures (empty
refs, missing cursor lookup, duplicate graph symbols/calls, edit atomic replace
`Access is denied`) are **not** test-only, stay outside P6, and remain a
separate parent-owned prerequisite; they are not claimed fixed here.

### Behavior → test → command/result matrix (this gate's macOS run)

| Behavior | Test | Command (offline) | Result |
|---|---|---|---|
| Release-only installer: exact-PREFIX verify, never cargo or stale PATH binary | `install_script_tests::release_only_installer_verifies_exact_destination_not_stale_path` | `cargo test --test install_script_tests` | 3 passed, exit 0 |
| Darwin x86_64 stops before download; no Intel→arm64/Rosetta falsehood | `install_script_tests::unsupported_darwin_x86_environment_stops_before_download` | same run | included above |
| Pinned version uses pipeline asset name | `install_script_tests::pinned_linux_release_uses_pipeline_asset_name` | same run | included above |
| Installer shell syntax | — | `bash -n scripts/install.sh` | exit 0 |
| 36 removed flat aliases refuse before execution, no JSON envelope | `alias_contract_tests::removed_flat_aliases_fail_before_execution_even_with_json` | `cargo test --test alias_contract_tests` | 3 passed, exit 0 |
| `docs`/`search` shorthands live with real semantic results | `alias_contract_tests::docs_and_search_shorthands_are_live_with_nonempty_semantic_results` | same run | included above |
| `tool bench` real nonzero fixture counts + help/capabilities advertisement | `alias_contract_tests::tool_bench_current_command_reports_real_nonzero_fixture_counts` | same run | included above |
| Benchmarks: real Kotlin/Java/Swift fixtures, per-invocation status+semantics, cache-byte/mtime invariance, decoys, empty/wrong-workspace negatives | `benches_tests::benchmark_fixture_semantics_cold_warm_batch_and_decoys`, `benchmark_contract_rejects_empty_success_and_wrong_workspace` | `cargo test --test benches` | 2 passed, 1 ignored (timing receipt), exit 0 |
| Graph identity: package/class-name/overload/same-line/source-set boundaries, ambiguity refusal with exact candidates, cold+warm × positional+name | `graph_identity_tests` (6 `identity_case` tests) | `cargo test --test graph_identity_tests` | 7 passed, exit 0 |
| Unique control: exact reach/graph/snapshot edges cold+warm, no cwd decoy | `graph_identity_tests::unique_control_has_exact_reach_graph_snapshot_edges_cold_and_warm` | same run | included above |
| Help guardrails not weakened | `args::tests::help_*` | `cargo test --bin kotlin-lsp args::tests::help_` | 7 passed, exit 0 |

### Explicit timing receipt (not a performance claim)

Benchmark timings execute explicitly; they are debug-profile, small synthetic
fixture receipts with no release, large-library or threshold claim:
`cargo test --offline --test benches -- --ignored --nocapture --test-threads=1`
→ exit 0, 1 passed, 21 per-invocation receipts (3 iterations × fresh check,
cold single query, 4 warm single queries, warm four-query batch) plus 3
aggregate `startup-amortization-samples-not-speedup-claim` summaries (24 total
records). This run: check-fresh
344–1061ms, cold single query 224–226ms, warm single 14–32ms, warm batch
15–16ms; cold/warm/batch stay clearly distinguished. `cargo fmt --all --
--check` and `cargo clippy --offline --all-targets -- -D warnings` pass with
zero warnings for this gate's files.
