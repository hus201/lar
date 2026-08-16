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
- For each dependency id:
  1. Collect candidate versions that satisfy the requirement (local store ∪ configured sources; yanked index pins excluded)
  2. Select the **highest compatible** version
  3. If that exact pin exists in multiple sources, take it from the **highest-priority** source (earlier in `sources.toml`)
  4. Never merge package contents from different sources
- Peeks candidate metadata from the package index (format 2+) without downloading archives; fetches only the winning set. Legacy format 1 indexes fall back to archive inspect.
- On materialize, verifies archive `content_hash` and that manifest dependencies match the index metadata used during search
- One version per id; when a later requirement conflicts with an earlier choice, the solver **backtracks** and tries older candidates
- Hard conflicts remain when no single version satisfies all requirements; multi-candidate failures list each attempt
- Verifies signatures/hashes and emits advisory warnings on materialize (refuses yanked on new fetch)
- Writes `lar.lock` with **exact** pins only

See [architecture.md](architecture.md) and [repos.md](../implementation/repos.md).
