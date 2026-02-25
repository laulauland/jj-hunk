# AGENTS

## Commits

Use conventional commits. The changelog is generated from these prefixes:
- `feat:` / `fix:` / `refactor:` / `perf:` / `docs:` / `chore:` / `style:`
- Scoped prefixes are fine: `feat(select): add range syntax`
- `chore(release):` and `release:` commits are excluded from the changelog

## Releasing

Binary: `jj-hunk`. Version lives in `Cargo.toml`.

To cut a release:
1. Bump version in `Cargo.toml`, commit: `chore(release): vX.Y.Z`
2. Push to main, then tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`
3. CI builds cross-platform bottles, generates changelog, creates GitHub release, and updates the Homebrew formula in `laulauland/homebrew-tap`

Requires `TAP_GITHUB_TOKEN` repo secret (PAT with write access to `laulauland/homebrew-tap`).
