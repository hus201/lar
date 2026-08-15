# Dependency Resolution

**Status:** Partial — store-backed exact pins, `lar.lock`, and fetch from package sources are implemented; version ranges are planned — [resolve-lockfile.md](../implementation/resolve-lockfile.md), [repos.md](../implementation/repos.md)

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
- Walks exact `[dependencies]` pins against the local SxS store
- Fetches missing exact pins from configured package sources (**main first** among `deps` sources)
- Verifies signatures/hashes and emits advisory warnings (refuses yanked pins on new fetch)
- Errors on missing packages, version conflicts, or cycles
- Writes `lar.lock` (no version ranges yet)

See [architecture.md](architecture.md) and [repos.md](../implementation/repos.md).
