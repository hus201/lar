# Use a package source

Configure LAR to fetch packages from a published source (local path or HTTP(S)). Fetch always verifies content hashes and Ed25519 signatures against your trust store.

Publisher side: [publish-repo.md](publish-repo.md). Reference: [repos.md](../implementation/repos.md).

## 1. Trust the publisher key

Obtain the publisher’s `ed25519.pub` (or a `base64:…` public key string) out of band.

```bash
lar repo trust add ./ed25519.pub --comment "Example vendor"
lar repo trust list
```

## 2. Add the source

```bash
# HTTP(S) or local path (name defaults from URI basename/host)
lar repo add https://example.com/lar-repo/
lar repo add /path/to/my-repo

# Explicit name:
lar repo add --name vendor https://example.com/lar-vendor/

# Sources are listed in priority order (earlier = higher). Prefer an overlay by
# listing it first in sources.toml (or add it before other sources).
lar repo list
```

For each dependency, LAR collects matching versions, picks the **highest compatible** version, and if that exact pin exists in several sources, takes it from the **highest-priority** source. Package contents are never merged across sources. The local store wins if the pin is already present. The same sources feed `lar resolve` and `lar install <id>`; installable apps are those with `[entry]` in their manifest.

## 3. Resolve, install, audit

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
| `untrusted key_id` | Missing `lar repo trust add` for that publisher |
| `invalid signature` | Wrong key, or index signed with a different secret |
| `content hash mismatch` | Hosted `.lar` does not match `index.toml` |
| `package … is yanked` | Advisory with `yanked = true` for that pin |
| `not found in configured sources` | Wrong URI or package not published |
