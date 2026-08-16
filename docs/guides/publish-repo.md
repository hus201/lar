# Publish a package source

Maintain a local directory that LAR clients can use as a package source (`index.toml`, `packages/*.lar`, optional `advisories.toml`). LAR does not upload to remotes — you serve or sync the tree yourself (rsync, GitHub Pages, S3 static hosting, etc.).

Subsystem reference: [repos.md](../implementation/repos.md).

## Layout

```text
my-repo/
  index.toml
  advisories.toml          # optional
  packages/
    org.example.lib-1.0.0.lar
```

## 1. Generate a signing key

```bash
lar package keygen --out ./keys
# keys/ed25519.pub  — distribute to clients (trust)
# keys/ed25519.sec  — keep private; used to sign the index
```

## 2. Initialize the source

```bash
lar repo init ./my-repo --sign-key ./keys/ed25519.sec
```

Creates `packages/` and an empty signed `index.toml`.

## 3. Pack and publish packages

```bash
lar package pack ./my-lib
lar repo publish ./my-repo ./my-lib/org.example.lib-1.0.0.lar \
  --sign-key ./keys/ed25519.sec
```

`publish` copies the archive into `packages/{id}-{version}.lar`, rebuilds `index.toml` (including dependency metadata for clients), and re-signs `advisories.toml` when that file exists.

To remove a pin:

```bash
lar repo unpublish ./my-repo org.example.lib 1.0.0 \
  --sign-key ./keys/ed25519.sec
```

Full rescan (after manual file changes):

```bash
lar repo index ./my-repo --sign-key ./keys/ed25519.sec
```

## 4. Validate before hosting

```bash
lar repo validate ./my-repo --pubkey ./keys/ed25519.pub
```

Without `--pubkey`, LAR checks layout and content hashes only. With `--pubkey`, it also verifies Ed25519 signatures on index entries (and advisories if present).

## 5. Host the directory

Point an HTTP(S) server or object store at `my-repo/` so clients can GET:

- `index.toml`
- `packages/…`
- optional `advisories.toml`

Examples: static site (GitHub Pages), `rsync` to a VPS, public S3/CloudFront, or a local path for testing.

Clients then add the URI and your **public** key — [use-repo.md](use-repo.md).

## Advisories (optional)

Hand-edit `advisories.toml` with the advisory body (leave signature fields empty or stale), then resign:

```bash
lar repo index ./my-repo --sign-key ./keys/ed25519.sec
```

Any later `publish` / `unpublish` also resigns advisories when the file is present.

Example body (before signing fills `content_hash` / `key_id` / `signature`):

```toml
format = 1

[[advisories]]
id = "LAR-2026-0001"
package_id = "org.example.lib"
versions = ["1.0.0"]
severity = "high"
yanked = false
summary = "Example issue"
url = "https://example.com/advisory"
```

Yanked pins refuse new fetch; non-yanked advisories warn. See [repos.md](../implementation/repos.md).

## Checklist

1. Secret key stays offline / CI secrets only
2. `validate --pubkey` succeeds before sync
3. Clients receive the matching **public** key out of band
4. Hosted tree is complete (`index.toml` + every `file` path listed in the index)
