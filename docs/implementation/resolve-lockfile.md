# LAR Resolve / Lockfile

`lar resolve` turns a root `package.toml` into a deterministic `lar.lock` by walking dependency **requirements**, selecting exact versions, and fetching missing pins from configured package sources when needed. The lockfile is the input for runtime composition and install.

## Scope (v1)

- Input: a local `package.toml` (or a directory containing it)
- Dependency source: local store, then configured package sources (source order = priority) — [repos.md](repos.md)
- Versions in manifests: semver **requirements** (`1.2.3`, `^1.2`, `~1.2.3`, `>=1.0, <2`); bare `*` rejected
- Lockfile: **exact** pins only
- Root package: always included from the local manifest (exact version)
- Selection: collect matching candidates → highest compatible version → if the same pin is in multiple sources, highest-priority source; never merge contents; backtrack to older versions on conflict; peek during search, fetch winners only

## Algorithm

1. Load and validate the root manifest.
2. Search the dependency graph (highest matching semver first; one version per id):
   - Same id already chosen and matches → continue
   - Same id already chosen and does not match → conflict (backtrack)
   - Otherwise try candidates from `list_dep_versions` filtered by the requirement
   - Load each candidate’s metadata via peek (store hit or download+inspect **without** `store.add`); cache in-memory
   - On conflict / unsatisfiable / missing further down, undo assignments along a trail and try the next-older candidate
   - Cycles abort that path immediately (not retried as a soft search failure)
   - If several candidates fail, the error lists each tried version and why
3. Materialize: `fetch_into_store` only for the winning pins (failed probes never enter the store).
4. Write `lar.lock` next to the root `package.toml`.

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
- Uses the same prefix rules as the store (`--system`, `LAR_USER_PREFIX`). System mode is always `/var/lib/lar`.
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
