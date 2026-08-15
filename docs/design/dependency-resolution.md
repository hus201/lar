# Dependency Resolution

**Status:** Partial — store-backed exact pins and `lar.lock` are implemented; fetching missing packages and version ranges are planned — [resolve-lockfile.md](../implementation/resolve-lockfile.md)

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
- Errors on missing packages, version conflicts, or cycles
- Writes `lar.lock` (no fetch, no ranges, no runtime composition yet)
