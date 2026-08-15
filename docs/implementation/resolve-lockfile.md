# LAR Resolve / Lockfile

`lar resolve` turns a root `package.toml` into a deterministic `lar.lock` by walking dependency **requirements**, selecting exact versions, and fetching missing pins from configured package sources when needed. The lockfile is the input for runtime composition and install.

## Scope (v1)

- Input: a local `package.toml` (or a directory containing it)
- Dependency source: local store, then package sources with `deps` (**main first** for fetch) — [repos.md](repos.md)
- Versions in manifests: semver **requirements** (`1.2.3`, `^1.2`, `~1.2.3`, `>=1.0, <2`); bare `*` rejected
- Lockfile: **exact** pins only
- Root package: always included from the local manifest (exact version)
- Selection: highest matching semver among store ∪ deps-index candidates (non-yanked)

## Algorithm

1. Load and validate the root manifest.
2. Walk `[dependencies]` transitively (each value is a version requirement).
3. For each `(id, req)`:
   - Same id already resolved and chosen version **matches** `req` → skip
   - Same id already resolved and chosen version **does not match** `req` → conflict error
   - Otherwise list candidates (`list_dep_versions`), filter by `req`, pick max semver
   - No candidate → unsatisfiable error
   - Ensure exact pin in store (hit + advisory warn, or fetch)
   - Load that package’s `package.toml` and enqueue its deps
4. Cycles are an error.
5. Write `lar.lock` next to the root `package.toml`.

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
dependencies = { "org.example.lib" = "^1.0" }

[[packages]]
id = "org.example.lib"
version = "1.2.0"
content_hash = "blake3:..."
dependencies = { "org.example.base" = "2.0.0" }
```

| Field | Notes |
|-------|--------|
| `format` | Lockfile format version; currently `1` |
| `[root]` | Root package id and version |
| `[[packages]]` | Sorted by `(id, version)`; includes the root; **versions are exact** |
| `content_hash` | Required for store packages; optional for an unpackaged root |
| `dependencies` | Declared requirements from that package’s manifest (may be ranges; omitted if empty) |

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
- Locked `dependencies` must match each stored package’s `[dependencies]` (including requirement strings).

This is intended for runtime composition and tooling that must refuse a stale lock.

## Related

- Package format: [package-format.md](package-format.md)
- SxS store: [sxs-store.md](sxs-store.md)
- Runtime: [runtime.md](runtime.md)
- Install: [install.md](install.md)
- Design (package sources): [architecture.md](../design/architecture.md)
- Implementation (repos): [repos.md](repos.md)
- Design (resolution): [dependency-resolution.md](../design/dependency-resolution.md)
