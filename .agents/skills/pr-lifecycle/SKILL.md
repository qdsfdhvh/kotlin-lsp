# PR Lifecycle

> Routed from [`../rules/INDEX.md`](../rules/INDEX.md). Covers the full PR
> lifecycle for kotlin-lsp: create → CI → merge → **post-merge cleanup**.
> The post-merge step is the one agents routinely drop — the PR lands and the
> working tree is left on a stale feature branch with `origin/main` behind.
> This skill exists so that never happens again.

## Triggers

- "merge the PR", "it's merged", "合并完了", "check PR CI"
- Any PR push / CI monitoring / merge / branch cleanup work

## Non-negotiables (from `git-workflow/RULE.md`)

- NEVER push to `main` directly — always a PR, squash-merge only.
- NEVER merge while any of the three platform checks (ubuntu/macos/windows)
  is failing, pending, or unknown. Green = **all three** passed.
- NEVER force-push, amend a pushed commit, or delete/re-tag a release tag.

## The lifecycle

### 1. Create the PR

```bash
git checkout -b fix/…               # topic branch off main
# … make changes, run the local gate …
git push -u origin fix/…
gh pr create --title "…" --body "…\n\nCloses #N"   # reference the issue
```

### 2. Watch CI — never skip this

```bash
gh pr checks <N>                    # see current state
```

Long-running watch (wakes the agent when the run finishes):

```
MonitorCreate(command="gh pr checks <N> --watch --interval 60",
              onDone="Report CI results; merge only if all three platforms passed")
```

Merge **only** when all three of `test (ubuntu-latest)`, `test (macos-latest)`,
`test (windows-latest)` show `pass`. A red cross on any platform means fix the
code, push a new commit, and wait for the re-run — never merge on a subset.

### 3. Merge

```bash
gh pr merge <N> --squash --delete-branch   # deletes the remote branch too
```

### 4. Post-merge cleanup (the step that used to get dropped)

Immediately after the merge lands — same session, not "later":

```bash
git checkout main
git pull                                   # origin/main now has the squash commit
git branch -d fix/…                        # delete the local topic branch
                                           # (or -D if squash renamed it and -d refuses)
git log --oneline -3                       # sanity-check the merge commit is there
git status                                 # clean tree, on main, up to date
```

If any of these fail (e.g. local branch has unmerged commits), stop and report
rather than forcing `-D` blindly.

### 5. Verify

- `git branch --list` shows no leftover topic branch.
- `git log origin/main..main` is empty (local main == remote main).
- The `Closes #N` issue is auto-closed (check with `gh issue view N --json state`).

## Related

- `../rules/git-workflow/RULE.md` — PR-only flow, hard bans, local gate
- `../rules/releasing/RULE.md` — release branch/tag flow (same cleanup applies
  after the release PR merges)
