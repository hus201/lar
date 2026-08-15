# LAR Install Records

Implemented in the `lar-manager` crate. `lar install` records an application under the LAR prefix, resolves its exact dependency pins from the local store (fetching from package sources when needed), composes a runtime, and writes an install record. `lar update` / `lar rollback` manage a single previous generation. `lar uninstall` removes the record and its runtimes; store packages remain.

## Scope (v1)

- Sources: path to a `.lar`, store package `id` / `id@version`, or fetch from package sources with `apps` — [repos.md](repos.md)
- One active install per application id (use `--force` to replace)
- Update: newest newer semver from `apps` sources
- Rollback: swap with one `previous.toml` slot
- Uninstall does not purge store packages

## Layout

```text
{prefix}/installs/{app_id}/
  install.toml       # active
  previous.toml      # last displaced generation (optional)
```

Atomic writes use `{prefix}/installs/.tmp-install-*`, then rename into place.

### `install.toml` / `previous.toml`

Same schema:

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

The `[[packages]]` lists (active **and** previous) are install pin sets used as store-remove referrers.

## Algorithm

### Install

1. Ensure the root package is in the store (add `.lar` if needed; on `AlreadyExists`, reuse only if the archive `content_hash` matches the store).
2. Refuse if `{installs}/{id}` already exists unless `--force`.
3. Resolve the root manifest against the store (`resolve_manifest`).
4. Verify the lock is runtime-ready (root included with matching `content_hash`).
5. Compose a runtime with the selected `--compose` mode.
6. Write `install.toml` atomically under `installs/{id}/`.
7. On replace (`--force` or update): stash the old active record as `previous.toml` and **keep** its runtime. If an older `previous.toml` already existed, drop that older runtime when it is unused by the new active/previous pair.

### Update

1. Load active install.
2. Scan configured `apps` sources for the same id; pick the highest semver **strictly greater** than the active version.
3. If none → report up to date (no change).
4. Fetch that pin (`LookupMode::Apps`), resolve, compose using the active install’s compose mode, and activate as a replace (stash previous).

### Rollback

1. Require `previous.toml`.
2. Atomically swap `install.toml` ↔ `previous.toml`.
3. Keep both runtimes.

### Uninstall

1. Load active (and previous if present).
2. Remove both referenced runtimes when distinct.
3. Remove `installs/{id}/`.
4. Leave store packages in place.

## Store remove referrers

Packages listed in any install record’s `[[packages]]` — including `previous.toml` — cannot be removed with `lar store remove`, including `--force`. The error names the pin as `install:{app_id}`. Uninstall the application before removing its packages.

Package-to-package dependency referrers continue to work as before.

## CLI

```bash
lar install path/to/app.lar
lar install --compose hardlink path/to/app.lar
lar install --force org.example.app
lar install org.example.app@0.1.0
lar update org.example.app
lar rollback org.example.app
lar list
lar uninstall org.example.app
```

- `install` prints `installed` or `reinstalled` (only when replacing an existing install) plus `id version (compose) runtime <runtime_id>`.
- `update` prints `updated id old -> new (runtime …)` or `up to date id version`.
- `rollback` prints `rolled back id version (runtime …)`.
- `list` prints `id version compose runtime_id` (sorted by id).
- `uninstall` prints `uninstalled id version (runtime <runtime_id>)`.

## Related

- SxS store: [sxs-store.md](sxs-store.md)
- Resolve / lockfile: [resolve-lockfile.md](resolve-lockfile.md)
- Runtime: [runtime.md](runtime.md)
- Repos: [repos.md](repos.md)
- Design: [Platform](../design/platform.md), [Architecture](../design/architecture.md)
