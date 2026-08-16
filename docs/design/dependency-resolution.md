# Dependency Resolution

**Status:** Partial — exact pins, semver **requirements** in manifests, `lar.lock`, and fetch from package sources are implemented; a full backtracking solver is not — [resolve-lockfile.md](../implementation/resolve-lockfile.md), [repos.md](../implementation/repos.md)

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
- For each dependency id, selects the **highest matching** version among the local store and configured package sources (yanked index pins excluded)
- Fetches the chosen exact pin if missing (`fetch_priority`: first-win or last-win among sources)
- One version per id: a later requirement that does not match the already-chosen version is a **conflict** (no backtracking)
- Verifies signatures/hashes and emits advisory warnings (refuses yanked on new fetch)
- Writes `lar.lock` with **exact** pins only

See [architecture.md](architecture.md) and [repos.md](../implementation/repos.md).
