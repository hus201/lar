# Dependency Resolution

**Status:** Implemented — [resolve-lockfile.md](../implementation/resolve-lockfile.md), [repos.md](../implementation/repos.md)

Dependency resolution is based on application requirements.

## Target process

1. Application manifest is loaded.
2. Required dependencies are identified.
3. Available packages are checked.
4. Compatible versions are selected.
5. Missing packages are retrieved.
6. Runtime environment is created.

The goal is deterministic runtime creation.

## Current implementation

Today `lar resolve`:

- Loads a local `package.toml`
- Treats `[dependencies]` values as **semver requirements** (exact, `^`, `~`, comparisons); rejects bare `*`
- For each dependency id, tries candidates from highest matching semver downward among the local store and configured package sources (yanked index pins excluded)
- Fetches the chosen exact pin if missing (`fetch_priority`: first-win or last-win among sources)
- One version per id; when a later requirement conflicts with an earlier choice, the solver **backtracks** and tries older candidates
- Hard conflicts remain when no single version satisfies all requirements
- Verifies signatures/hashes and emits advisory warnings (refuses yanked on new fetch)
- Writes `lar.lock` with **exact** pins only

See [architecture.md](architecture.md) and [repos.md](../implementation/repos.md).
