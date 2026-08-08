# Releasing

> Routed from [`INDEX.md`](../INDEX.md). A release is a tag push: the
> tag-triggered `release.yml` workflow builds 5 platform binaries (darwin-x86_64
> dropped 2026-08) and creates the GitHub Release. Tag writes are governed by
> `git-workflow/RULE.md` § Hard bans — a tag must never be created twice,
> deleted, or force-pushed.

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
6. Confirm the release workflow completes and all 5 assets
   (linux × x86_64/aarch64, windows × x86_64/aarch64, darwin-aarch64) are on
   the GitHub Release. darwin-x86_64 was dropped in 2026-08 (no users); Intel
   Macs fall back to running the aarch64 build under Rosetta or building from
   source.
7. **Update the local machine's installed binary from the new Release asset**
   — download the platform tarball/zip (`install.sh`, or
   `kotlin-lsp-darwin-aarch64.tar.gz` etc.), verify `--version` shows the new
   tag, then replace the installed binary (e.g. `~/.cargo/bin/kotlin-lsp`).
   Keep a backup of the previous version until the new one smoke-tests clean.
   NEVER rebuild locally for the installed binary.

## Hard rules

- ❌ NEVER create a tag that already exists — verify first, ask the user before
  any overwrite (`git tag -f`, `git push --delete`).
- ❌ NEVER install the released binary by local compile
  (`cargo build --release && cp …`) — install from the GitHub Release asset
  (`install.sh`, or the platform tarball). Local builds bypass the release
  pipeline (CI signing, checksum verification, cross-platform testing).
- ✅ After every release, upgrade any local machine's installed `kotlin-lsp`
  from the new Release asset (step 7 above), never from a local build.
- ✅ The installed binary must come from the release tag, and `--version`
  must match the release.

## CI monitoring

Watch PR CI with a pi-loop monitor, poll, or auto-merge loop (see AGENTS.md
CI section):

```
MonitorCreate(command="gh pr checks --watch <PR>", onDone="Report CI results")
```

Release workflow runs are listed with `gh run list --workflow=release.yml`.

## Related

- `git-workflow/RULE.md` — PR flow and tag hard bans
- `.github/workflows/release.yml` — the tag-triggered platform build
- `docs/commands.md` — user-facing docs kept in sync with the CLI surface
