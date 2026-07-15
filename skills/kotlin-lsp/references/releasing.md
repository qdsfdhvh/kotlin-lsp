# Release Process

## Version Numbering

`MAJOR.MINOR.PATCH` (semver):
- **Patch** (0.26.x): bug fixes, internal refactors, doc-only changes
- **Minor** (0.x): new features, new CLI commands, behavioural changes
- **Major** (1.x): breaking API / CLI / cache-format changes

## Steps

```bash
# 1. Bump version in Cargo.toml
vim Cargo.toml  # version = "0.26.9"

# 2. Run cargo update to sync Cargo.lock
cargo update

# 3. Add section to top of CHANGELOG.md
vim CHANGELOG.md

# 4. Commit on a branch, create PR, wait for green CI
git checkout -b release-0.26.9
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: bump version to 0.26.9"
git push origin release-0.26.9
gh pr create --title "chore: bump version to 0.26.9"

# 5. After merge, tag and push
git checkout main && git pull
git tag v0.26.9
git push origin v0.26.9
```

## Required Files

| File | Change |
|------|--------|
| `Cargo.toml` | Bump `version` field (line 6) |
| `Cargo.lock` | Auto-updated by `cargo update` |
| `CHANGELOG.md` | New section at top with date and changes |

## Tag Safety

**NEVER delete or force-push tags without explicit confirmation.** Tags represent published releases. Deleting a tag breaks GitHub Releases, CI artifacts, and downstream consumers.

- `git tag -d` → ask first
- `git push --delete origin <tag>` → ask first
- `git tag -f` → ask first
