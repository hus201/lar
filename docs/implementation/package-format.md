# LAR Package Format

This document defines the v1 LAR package format: the `package.toml` manifest, staged directory layout, and `.lar` archive.

## Package model

LAR uses a **single package kind**. There is no required `type` or `role` field.

A package is identified by reverse-DNS `id` and semver `version`. Capabilities come from optional manifest tables and the payload:

- `[dependencies]` — exact version pins
- `[entry]` — one or more launchable binaries
- `[desktop]` — desktop integration metadata (reserved for later use)
- `files/` — immutable payload

## Staged directory layout

```text
my-app/
  package.toml
  files/
    bin/...
    lib/...
```

`lar package pack` reads this layout and produces a `.lar` archive.

### Payload file rules (v1)

Under `files/`, only **real directories** and **regular files** are allowed.

- Symlinks are rejected (not followed, not stored).
- Special files (devices, sockets, FIFOs) are rejected.
- Empty directories may exist in the staged tree but are not required in the archive inventory (only regular files are hashed).

This keeps packages reproducible and avoids packing host paths by accident. Symlink support can be revisited later if needed.

## Manifest schema (`package.toml`)

```toml
[package]
format = 1
id = "org.example.editor"
name = "Example Editor"
version = "0.1.0"
description = "Optional description"
# Set by pack:
# content_hash = "blake3:<hex>"

[dependencies]
"org.qt.qtbase" = "6.8.1"

[entry]
default = "bin/editor"
binaries = ["bin/editor"]

[desktop]
# Optional fields reserved for desktop integration
```

### `[package]` fields

| Field | Required | Notes |
|-------|----------|--------|
| `format` | yes | Package format version; currently `1` |
| `id` | yes | Reverse-DNS identifier |
| `name` | yes | Human-readable name |
| `version` | yes | Semver |
| `description` | no | Free text |
| `content_hash` | no | `blake3:<hex>` of the payload; written by `pack` |

Unknown keys under `[package]` are rejected. Do not use `type` or `role`.

Tooling built for format `1` rejects any other `format` value. Bump `format` only when making incompatible changes to the manifest or archive layout.

### `[dependencies]`

Keys are package ids (reverse-DNS). Values are exact semver versions (no ranges in v1).

### `[entry]` (optional)

| Field | Notes |
|-------|--------|
| `binaries` | Non-empty list of paths relative to `files/` |
| `default` | Optional; if set, must be one of `binaries` |

When `[entry]` is present, every listed path must exist under `files/` at validate/pack time.

### `[desktop]` (optional)

Reserved for later desktop integration. Unknown keys are rejected; fields may be empty in v1.

### Top-level tables

Only `package`, `dependencies`, `entry`, and `desktop` are allowed. Unknown top-level tables are rejected.

## Archive layout (`.lar`)

A `.lar` file is a **tar archive compressed with zstd**.

Contents:

```text
package.toml       # validated manifest, including content_hash
manifest.json      # machine index generated at pack time
files/             # payload tree
```

### `manifest.json`

Generated at pack time (not hand-edited). Example shape:

```json
{
  "format": 1,
  "id": "org.example.editor",
  "version": "0.1.0",
  "content_hash": "blake3:<hex>",
  "files": [
    {
      "path": "bin/editor",
      "blake3": "<hex>",
      "size": 12345
    }
  ]
}
```

File paths are relative to `files/`. The future SxS store can index this without re-parsing TOML for every file. `format` must match `package.format` in `package.toml`.

### Integrity (v1)

- Each payload file has a BLAKE3 digest in `manifest.json`.
- `content_hash` is a BLAKE3 digest over the canonical payload inventory (sorted paths with per-file digests).
- `lar package pack` writes `content_hash` into both the staged `package.toml` and the copy inside the `.lar` archive.
- Reading a `.lar` (via `inspect`) re-hashes every payload file and checks digests, sizes, file set, and `content_hash` against `manifest.json` / `package.toml`.
- Extracting a `.lar` verifies the archive, writes files, then re-hashes the on-disk `files/` tree before succeeding (failed verify removes the destination).
- Cryptographic package signatures are reserved for a later security milestone.

## Validation rules (v1)

- `package.format` must be `1`
- `id` matches reverse-DNS pattern
- `version` and dependency versions are valid semver
- dependency keys are valid package ids
- unknown `[package]` keys and unknown top-level tables are rejected
- if `[entry]` is present: `binaries` is non-empty; `default` (if set) is in `binaries`; each path exists under `files/`

## CLI

```bash
lar package init --id <reverse-dns> [--name ...] [--version ...] [--force] [dir]
lar package validate [package.toml|dir]
lar package pack [-o out.lar] [dir]
lar package inspect [--json] <package.lar>
```

Default pack output name: `{id}-{version}.lar`.

`lar package inspect` reads a `.lar`, verifies payload digests, and prints package metadata (or JSON with `--json`).

## Related

- Design: [Package model](../design/packages.md)
- SxS store: [sxs-store.md](sxs-store.md)
- Resolve / lockfile: [resolve-lockfile.md](resolve-lockfile.md)
