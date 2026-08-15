# Package sources (repos)

**Status:** Implemented (foundation)

Configured package sources distribute `.lar` archives into the local SxS store. Fetch requires Ed25519 signatures over `content_hash` and a trusted publisher key. Repos may also publish vulnerability advisories; LAR **warns** (and refuses new fetch of **yanked** pins) but never auto-deletes store packages.

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

## Source config

```toml
format = 1

[[sources]]
name = "main"
uri = "/path/to/repo"           # or file://, http://, https://
policy = "deps"                 # deps | apps | both
main = true                     # at most one; must allow deps
```

Fetch priority for missing dependencies:

1. Local store
2. **main** (among `deps` sources)
3. Other `deps` sources in config order

`lar install <id>` uses only sources with `apps` (after local `.lar` / store).

## Signatures and trust

- Algorithm: Ed25519
- Signed message: UTF-8 bytes of `content_hash` (e.g. `blake3:<hex>`)
- Signature fields live on each `index.toml` package entry (`key_id`, `signature`)
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

Advisories are not signed; integrity is trust-on-transport (same path as the index). Package authenticity remains Ed25519 over `content_hash`.

```toml
format = 1

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
| Fetch/resolve/install hits non-yanked advisory | **Warn** on stderr; continue |
| Package already in store and yanked | **Warn**; do not delete |
| `lar audit` | Report; exit non-zero if any high/critical or yanked-in-use |

LAR does not invent CVEs. No advisory from configured sources → no warning for that pin.

## CLI

```bash
lar package keygen [--out DIR]
lar repo trust add <pubkey-or-file> [--comment TEXT]
lar repo trust list
lar repo trust remove <key_id>
lar repo add [--policy deps|apps|both] [--main] [--name NAME] <path-or-url>
# --main defaults policy to deps; otherwise default is both
lar repo list
lar repo remove <name-or-uri>
lar repo index <dir> --sign-key <secret-or-file>
lar audit [--installed|--store]   # default: installed apps' pins
```

Resolve and install fetch missing exact pins through this path (hash + signature + advisory checks).

## Crate

Logic lives in `lar-repo` (`fetch_into_store`, sources/trust CRUD, index build, audit).
