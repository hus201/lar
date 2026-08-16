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
# Main dependency source (unique; default policy deps)
lar repo add --main https://example.com/lar-repo/

# Or a local path while developing
lar repo add --main /path/to/my-repo

# Apps-only or both:
lar repo add --policy apps --name vendor-apps https://example.com/lar-apps/
lar repo add --policy both --name vendor https://example.com/lar-repo/

lar repo list
```

Policies:

| Policy | Used for |
|--------|----------|
| `deps` | Missing dependencies during resolve / install |
| `apps` | `lar install <id>` / `lar update` discovery |
| `both` | Both |

At most one `--main` source; it must allow deps. Missing deps are sought in: local store → main → other deps sources in config order.

## 3. Resolve, install, audit

```bash
lar resolve                    # fetches missing pins from deps sources
lar install org.example.app    # needs an apps (or both) source, or a local .lar / store hit
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
| `not found in configured sources` | Wrong URI, policy, or package not published |
