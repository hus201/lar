# Package sources (repos)

**Status:** Implemented

Configured package sources distribute `.lar` archives into the local SxS store. Fetch requires Ed25519 signatures over `content_hash` and a trusted publisher key. Repos may also publish **signed** vulnerability advisories; LAR **warns** (and refuses new fetch of **yanked** pins) but never auto-deletes store packages.

Design overview: [architecture.md](../design/architecture.md).

## Layout

Under the LAR prefix:

```text
{prefix}/config/sources.toml
{prefix}/config/trust.toml
```

Published source layout (local path or HTTP(S) base):

```text
{base}/index.toml
{base}/advisories.toml          # optional; absent → empty; present but invalid → error
{base}/packages/org.example.lib-1.0.0.lar
```

## Package index (`index.toml`)

```toml
format = 2

[[packages]]
id = "org.example.lib"
version = "1.0.0"
content_hash = "blake3:…"
file = "packages/org.example.lib-1.0.0.lar"
key_id = "ed25519:…"
signature = "base64:…"
dependencies = { "org.example.base" = "^1.0" }
```

Format **2** embeds each pin’s `[dependencies]` and **signs** them with the pin (together with id/version/hash/file) so resolve can search without downloading `.lar` files. Format **1** indexes remain readable; resolve falls back to inspecting the archive for those pins. `lar repo publish` / `index` always write format 2.

## Source config

```toml
format = 1

[[sources]]
name = "upstream"
uri = "/path/to/repo"           # or file://, http://, https://

[[sources]]
name = "overlay"
uri = "/path/to/overlay"
```

Source **order is priority** (earlier = higher). When resolving or fetching a pin:

1. Collect candidates that satisfy the version requirement
2. Select the highest compatible version
3. If the same `(id, version)` exists in multiple sources, take it from the highest-priority source
4. Never merge package contents from different sources

Local store always wins if the pin is already present. Whether a package is installable as an app is determined by its manifest (`[entry]`), not by the source.

Legacy `fetch_priority` keys in older `sources.toml` files are ignored.

## Signatures and trust

- Algorithm: Ed25519
- **Index format 2+ package pins:** signature covers a canonical message including `id`, `version`, `content_hash`, `file`, and `dependencies` (so resolve can trust index metadata without downloading `.lar` files)
- **Index format 1 (legacy):** signature covers UTF-8 `content_hash` only
- Advisory signatures: top-level `advisories.toml` fields (`content_hash`, `key_id`, `signature`) — Ed25519 over the UTF-8 `content_hash` string
- Trust store: `{prefix}/config/trust.toml`

```toml
format = 1

[[keys]]
id = "ed25519:…"
public_key = "base64:…"
comment = "Example vendor"
```

Repo fetch requires a trusted `key_id` and a valid signature, then verifies the archive hash matches the index. Direct `lar store add file.lar` remains unsigned-OK.

## Advisories

Optional `advisories.toml`. Absent → empty (no warning). Present → must include `content_hash`, `key_id`, and `signature`: BLAKE3 over the canonical `format` + `[[advisories]]` payload, then Ed25519 over the UTF-8 `content_hash` string (same shape as package index entries), verified against the trust store. `lar repo index --sign-key` signs the file when present.

```toml
format = 1
content_hash = "blake3:…"
key_id = "ed25519:…"
signature = "base64:…"

[[advisories]]
id = "LAR-2026-0001"
package_id = "org.example.lib"
versions = ["1.0.0"]
content_hashes = []
severity = "high"               # low | medium | high | critical
yanked = false
summary = "…"
url = "https://…"
```

| Situation | Behavior |
|-----------|----------|
| Fetch hits `yanked = true` | **Refuse** (error) |
| Resolve finds only yanked pins for a requirement | **Refuse** with explicit yank error (not a generic “no matching version”) |
| Fetch/resolve/install hits non-yanked advisory | **Warn** on stderr; continue |
| Package already in store and yanked | **Warn**; do not delete |
| Present file missing/invalid hash or signature, or untrusted key | **Refuse** |
| `lar audit` | Report; exit non-zero if any high/critical or yanked-in-use |

LAR does not invent CVEs. No advisory from configured sources → no warning for that pin.

## CLI

Consumer (configure sources and trust):

```bash
lar package keygen [--out DIR]
lar repo trust add <pubkey-or-file> [--comment TEXT]
lar repo trust list
lar repo trust remove <key_id>
lar repo add [--name NAME] <path-or-url>
lar repo list                          # priority order (1 = highest)
lar repo move <source> --to N          # or --before/--after/--top/--bottom
lar repo remove <name-or-uri>
lar audit [--installed|--store]   # default: installed apps' pins
```

Publisher (maintain a local package-source directory):

```bash
lar repo init <dir> --sign-key <secret-or-file>
lar repo publish <dir> <file.lar> --sign-key <secret-or-file>
lar repo unpublish <dir> <package_id> <version> --sign-key <secret-or-file>
lar repo validate <dir> [--pubkey <public-or-file>]
lar repo index <dir> --sign-key <secret-or-file>   # full rebuild; also signs advisories.toml if present
```

Walkthrough: [Publish a package source](../guides/publish-repo.md). Clients: [Use a package source](../guides/use-repo.md).

Hand-edit optional `advisories.toml` (unsigned body), then `lar repo index --sign-key` (or any publish/unpublish) to resign.

Resolve and install fetch missing exact pins through this path (hash + signature + advisory checks).

## Crate

Logic lives in `lar-repo` (`fetch_into_store`, sources/trust CRUD, publisher init/publish/unpublish/validate, index build, audit).
