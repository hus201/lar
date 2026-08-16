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
- Uses the **PubGrub** conflict-driven solver (highest matching versions preferred; one version per id)
- For each dependency id:
  1. Collect candidate versions that satisfy the requirement (local store ∪ configured sources; yanked index pins excluded — if only yanked pins match, resolve fails with an explicit yank error)
  2. Select a compatible version via PubGrub (prefers highest)
  3. If that exact pin exists in multiple sources, take it from the **highest-priority** source (earlier in `sources.toml`)
  4. Never merge package contents from different sources
- Peeks candidate metadata from the package index (deps are part of the signed pin payload) without downloading archives; fetches only the winning set.
- On materialize, verifies archive `content_hash` and that manifest dependencies match the index metadata used during search
- Dependency cycles in the selected graph are rejected
- Unsatisfiable graphs produce a PubGrub derivation report; when a package has no matching version and some sources were unavailable during discovery, the error lists each source as ✓ / ✗
- Verifies signatures/hashes and emits advisory warnings on materialize (refuses yanked on new fetch)
- Writes `lar.lock` with **exact** pins only

See [architecture.md](architecture.md) and [repos.md](../implementation/repos.md).

**Out of scope for current resolve:** host **platform requirements** (Wayland, Vulkan, D-Bus, …). Those are not LAR packages; they are enforced at install/launch — see [platform.md](platform.md#platform-requirements).
