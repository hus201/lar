# Use a package source

Configure LAR to fetch packages from a published source (local path or HTTP(S)). Fetch always verifies content hashes and Ed25519 signatures against your trust store.

Publisher side: [publish-repo.md](publish-repo.md). Reference: [repos.md](../implementation/repos.md).

## 1. Add the source

One command trusts the publisher key and adds the source (apt/dnf-style). The source must publish `{base}/ed25519.pub`.

```bash
# Interactive: prints the key id and asks to confirm
lar repo add https://example.com/lar-repo/

# Scripts / CI: accept without a prompt
lar repo add --yes https://example.com/lar-repo/
lar repo add --fingerprint ed25519:… https://example.com/lar-repo/

# Explicit name or local path:
lar repo add --name vendor --yes https://example.com/lar-vendor/
lar repo add --yes /path/to/my-repo

# Offline / pre-shared key (skip fetching ed25519.pub):
lar repo add --pubkey ./ed25519.pub --yes https://example.com/lar-repo/

# Sources are listed in priority order (1 = highest). Prefer an overlay with:
lar repo move overlay --top
# or: lar repo move overlay --before upstream
# or: lar repo move overlay --to 1
lar repo list
```

For each dependency, LAR collects matching versions, picks the **highest compatible** version, and if that exact pin exists in several sources, takes it from the **highest-priority** source. Package contents are never merged across sources. The local store wins if the pin is already present. The same sources feed `lar resolve` and `lar install <id>`; installable apps are those with `[entry]` in their manifest.

Advanced: `lar repo trust add` / `trust remove` still work for managing keys without adding a source. `lar repo add --skip-trust` only registers the URI (you must trust the key yourself before fetch).

## 2. Resolve, install, audit

```bash
lar resolve                    # fetches missing pins from configured sources
lar install org.example.app    # from a source, local .lar, or store hit
lar audit                      # advisories against installed (or --store) pins
```

Direct `lar store add file.lar` does not require signatures. Anything pulled through a source does.

## Remove a source or key

```bash
lar repo remove vendor
lar repo trust remove ed25519:…
```

## Troubleshooting

| Symptom | Likely cause |
|---------|----------------|
| `source has no ed25519.pub` | Publisher did not run `lar repo init` / `index`, or pass `--pubkey` |
| `fingerprint mismatch` | `--fingerprint` does not match the source’s key |
| `untrusted key_id` | Key was never trusted (`repo add` without confirm, or `--skip-trust`) |
| `invalid signature` | Wrong key, or index signed with a different secret |
| `content hash mismatch` | Hosted `.lar` does not match `index.toml` |
| `package … is yanked` | Advisory with `yanked = true` for that pin |
| `not found in configured sources` | Wrong URI or package not published |
