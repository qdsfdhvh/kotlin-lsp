# Git Workflow

> Routed from [`INDEX.md`](../INDEX.md). Read § Hard bans before any git write —
> commit, push, branch, worktree, merge, PR. These are unrecoverable once done.

## Hard bans

- ❌ NEVER push to `main` directly — all changes go through a PR
  (repo settings enforce this: require a PR, squash-merge only).
- ❌ NEVER delete or force-push a git tag without explicit user confirmation.
  Tags represent published releases; deleting one breaks GitHub Releases, CI
  artifacts, and downstream consumers. `git tag -d`, `git push --delete`,
  `git tag -f`, force-push of a tag — all require asking first. Before
  touching a tag, verify whether it is a new tag or an existing release.
- ❌ NEVER `git checkout` a tag then edit it, `--amend` a pushed commit, or
  rewrite pushed history on shared branches.

## PR-only flow

1. Branch from `main` with a topic name (`fix/…`, `feat/…`, `docs/…`,
   `chore/…`, `release/…`).
2. Run the local gate before push (see below).
3. Push, open a PR. Reference issues with `Closes #N` to auto-close on merge.
4. Wait for CI green on ALL three platforms (ubuntu / macos / windows) before
   merging — never merge on a subset.
5. Merge with squash + delete branch.
6. **Post-merge cleanup — same session, never deferred**: `git checkout main
   && git pull && git branch -d <topic>`, then verify `git status` is clean
   and `git log origin/main..main` is empty. Full procedure:
   `.agents/skills/pr-lifecycle/SKILL.md` § 4.

## Local gate before push (rule 8)

```sh
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

The `--all-targets` on clippy matters: plain `cargo clippy` skips
`#[cfg(test)]` modules, so a test-code lint can pass locally and fail CI
(2026-08-08: `clippy::if_same_then_else` in a test slipped through to macOS
CI). The pre-commit hook (`.githooks/pre-commit`) runs the same gate using
toolchain binaries directly to avoid rustup PATH issues:

```sh
find src tests -name '*.rs' | xargs rustfmt --edition 2021 --check   # fmt proxy issue fallback
```

## Related

- `releasing/RULE.md` — tag creation (the one legitimate tag write) and release flow
- `.agents/skills/pr-lifecycle/SKILL.md` — full PR lifecycle incl. post-merge cleanup
- `.githooks/pre-commit` — the enforced gate
