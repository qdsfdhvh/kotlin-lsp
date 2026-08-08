# Releasing

> Routed from [`INDEX.md`](../INDEX.md). A release is a tag push: the
> tag-triggered `release.yml` workflow builds 6 platform binaries and creates
> the GitHub Release. Tag writes are governed by `git-workflow/RULE.md` § Hard
> bans — a tag must never be created twice, deleted, or force-pushed.

## Version numbering

`MAJOR.MINOR.PATCH` (semver), current train 0.30.x:

- **Patch** (0.30.x): bug fixes, refactors, doc-only changes
- **Minor** (0.x): new features, new CLI commands, behavioural changes
- **Major** (1.x): breaking API / CLI / cache-format changes

## Release steps (in order)

1. Bump `version` in `Cargo.toml` (line 6); `cargo check` syncs `Cargo.lock`.
2. Convert the `## Unreleased` CHANGELOG section to `## X.Y.Z (YYYY-MM-DD)`
   at the top of `CHANGELOG.md`.
3. Commit on a `release/vX.Y.Z` branch (pre-commit hook runs the gate),
   push, open a PR titled `chore(release): prepare vX.Y.Z`.
4. Wait for CI green on all three platforms, squash-merge, `git checkout main
   && git pull`.
5. Verify the tag does not exist (`git tag -l vX.Y.Z`), then create and push:
   `git tag vX.Y.Z && git push origin vX.Y.Z` — this triggers `release.yml`.
6. Confirm the release workflow completes and all 6 assets
   (linux/darwin/windows × x86_64/aarch64) are on the GitHub Release.

## Hard rules

- ❌ NEVER create a tag that already exists — verify first, ask the user before
  any overwrite (`git tag -f`, `git push --delete`).
- ❌ NEVER install the released binary by local compile
  (`cargo build --release && cp …`) — install from the GitHub Release asset
  (`install.sh`, or the platform tarball). Local builds bypass the release
  pipeline (CI signing, checksum verification, cross-platform testing).
- ✅ The installed binary must come from the release tag.

## CI monitoring

Watch PR CI with a pi-loop monitor, poll, or auto-merge loop (see AGENTS.md
CI section):

```
MonitorCreate(command="gh pr checks --watch <PR>", onDone="Report CI results")
```

Release workflow runs are listed with `gh run list --workflow=release.yml`.

## Related

- `git-workflow/RULE.md` — PR flow and tag hard bans
- `.github/workflows/release.yml` — the tag-triggered 6-platform build
- `docs/commands.md` — user-facing docs kept in sync with the CLI surface
