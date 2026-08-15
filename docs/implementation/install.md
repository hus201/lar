# LAR Install Records

Implemented in the `lar-manager` crate. `lar install` records an application under the LAR prefix, resolves its exact dependency pins from the local store, composes a runtime, and writes an install record. `lar uninstall` removes that record and its runtime; store packages remain.

## Scope (v1)

- Sources: path to a `.lar`, or a store package `id` / `id@version` (bare `id` requires exactly one version in the store)
- No repository fetch (local `.lar` / store only; package sources planned — [architecture.md](../design/architecture.md))
- One active install per application id (use `--force` to replace)
- Uninstall does not purge store packages

## Layout

```text
{prefix}/installs/{app_id}/
  install.toml
```

Atomic writes use `{prefix}/installs/.tmp-install-*`, then rename into place.

### `install.toml`

```toml
format = 1
id = "org.example.app"
version = "0.1.0"
content_hash = "blake3:..."
runtime_id = "..."
compose = "symlink"

[[packages]]
id = "org.example.app"
version = "0.1.0"
content_hash = "blake3:..."

[[packages]]
id = "org.example.lib"
version = "1.0.0"
content_hash = "blake3:..."
```

The `[[packages]]` list is the install pin set used as store-remove referrers.

## Algorithm

### Install

1. Ensure the root package is in the store (add `.lar` if needed; on `AlreadyExists`, reuse only if the archive `content_hash` matches the store).
2. Refuse if `{installs}/{id}` already exists unless `--force`.
3. Resolve the root manifest against the store (`resolve_manifest`).
4. Verify the lock is runtime-ready (root included with matching `content_hash`).
5. Compose a runtime with the selected `--compose` mode.
6. Write `install.toml` atomically under `installs/{id}/`.
7. On `--force` replace, delete the previous runtime when its `runtime_id` differs.

### Uninstall

1. Load `installs/{id}/install.toml`.
2. Remove `{runtimes}/{runtime_id}` if present.
3. Remove `installs/{id}/`.
4. Leave store packages in place.

## Store remove referrers

Packages listed in any install record’s `[[packages]]` cannot be removed with `lar store remove`, including `--force`. The error names the pin as `install:{app_id}`. Uninstall the application first.

Package-to-package dependency referrers continue to work as before.

## CLI

```bash
lar install path/to/app.lar
lar install --compose hardlink path/to/app.lar
lar install --force org.example.app
lar install org.example.app@0.1.0
lar list
lar uninstall org.example.app
```

- `install` prints `installed` or `reinstalled` (only when replacing an existing install) plus `id version (compose) runtime <runtime_id>`.
- `list` prints `id version compose runtime_id` (sorted by id).
- `uninstall` prints `uninstalled id version (runtime <runtime_id>)`.

## Related

- SxS store: [sxs-store.md](sxs-store.md)
- Resolve / lockfile: [resolve-lockfile.md](resolve-lockfile.md)
- Runtime: [runtime.md](runtime.md)
- Design: [Platform](../design/platform.md), [Architecture](../design/architecture.md)
