# LAR Resolve / Lockfile

`lar resolve` turns a root `package.toml` into a deterministic `lar.lock` by walking exact dependency pins against the local SxS store, fetching missing pins from configured package sources when needed. The lockfile is the input for runtime composition and install.

## Scope (v1)

- Input: a local `package.toml` (or a directory containing it)
- Dependency source: local store, then package sources with `deps` (**main first**) — [repos.md](repos.md)
- Versions: exact pins only (no ranges)
- Root package: always included from the local manifest
- Dependencies: fetched into the store if missing (signature + hash + advisory checks)

## Algorithm

1. Load and validate the root manifest.
2. Walk `[dependencies]` transitively.
3. For each `(id, version)`:
   - Same id already resolved at the same version → skip
   - Same id already resolved at a different version → conflict error
   - In the store → use it (emit advisory warnings if any)
   - Missing → fetch from `deps` sources (main first); refuse yanked; warn on advisories
   - Load that package’s `package.toml` from the store and enqueue its deps
4. Cycles are an error.
5. Write `lar.lock` next to the root `package.toml`.

(No version ranges in the current implementation.)

## Lockfile format (`lar.lock`)

```toml
format = 1

[root]
id = "org.example.editor"
version = "0.1.0"

[[packages]]
id = "org.example.editor"
version = "0.1.0"
# content_hash optional when the root is not yet packed

[[packages]]
id = "org.example.lib"
version = "1.0.0"
content_hash = "blake3:..."
dependencies = { "org.example.base" = "2.0.0" }
```

| Field | Notes |
|-------|--------|
| `format` | Lockfile format version; currently `1` |
| `[root]` | Root package id and version |
| `[[packages]]` | Sorted by `(id, version)`; includes the root |
| `content_hash` | Required for store packages; optional for an unpackaged root |
| `dependencies` | Exact pins from that package’s manifest (omitted if empty) |

## CLI

```bash
lar resolve
lar resolve path/to/package.toml
lar resolve path/to/package-dir
lar --system resolve
```

- Writes `lar.lock` beside the root manifest.
- Uses the same prefix rules as the store (`--system`, `LAR_USER_PREFIX`, `LAR_SYSTEM_PREFIX`).
- Prints the root id/version, package count, and lockfile path.

## Verify against the store

`lar_resolver::verify_lockfile` checks a loaded lockfile against the current store:

- Lock structure is valid (including required non-root `content_hash` values).
- Root without `content_hash` may be absent from the store (unpackaged root).
- Every package that has a `content_hash` must exist in the store with that exact hash.
- Locked `dependencies` must match each stored package’s `[dependencies]`.

This is intended for runtime composition and tooling that must refuse a stale lock.

## Related

- Package format: [package-format.md](package-format.md)
- SxS store: [sxs-store.md](sxs-store.md)
- Runtime: [runtime.md](runtime.md)
- Install: [install.md](install.md)
- Design (package sources): [architecture.md](../design/architecture.md)
- Implementation (repos): [repos.md](repos.md)
- Design: [Dependency resolution](../design/dependency-resolution.md)
