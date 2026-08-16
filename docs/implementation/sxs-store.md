# LAR SxS Package Store

The Side-by-Side (SxS) store is LAR’s local immutable package storage. Verified `.lar` archives are unpacked under a prefix so multiple versions of the same package can coexist. Future runtime composition reads package content from this store.

## Prefixes

| Mode | Prefix |
|------|--------|
| User (default) | `~/.local/share/lar` |
| System (`lar --system`) | `/var/lib/lar` |

**Overrides (tests / advanced layouts):**

- `LAR_USER_PREFIX` — replaces `~/.local/share/lar` in user mode

System mode (`--system`) always uses `/var/lib/lar` (no override).

Store root: `{prefix}/store`

## Layout

```text
{prefix}/
  store/
    packages/
      org.example.editor/
        0.1.0/
          package.toml
          manifest.json
          files/
            ...
        0.2.0/
          ...
```

Each `(id, version)` directory matches the `.lar` archive contents.

## Immutability

- An existing `(id, version)` cannot be overwritten.
- After a successful add, do not mutate `package.toml`, `manifest.json`, or `files/` in place.
- Adds verify the archive (BLAKE3 digests / `content_hash`) once during extract, write files, then re-hash the on-disk tree before commit.
- Commit extracts into `{store}/packages/.tmp-add-*`, then atomically renames to `{id}/{version}/`.
- `add` and `list` remove any leftover `{store}/packages/.tmp-add-*` directories from failed or crashed adds.

## Remove policy

### Refuse (default)

`store remove` deletes one `(id, version)` only if **nothing still requires it**:

- No other package in the store pins that exact id and version under `[dependencies]`.
- No install record under `{prefix}/installs/*/install.toml` lists that package in `[[packages]]`.

```bash
lar store remove org.example.lib 1.0.0   # fails if a package or install still pins it
```

Install pins are reported as `install:{app_id}` in the error (for example `install:org.example.app`). Uninstall the application before removing its packages.

### Cascade (`--force`)

`store remove --force` recursively removes package dependents first, then the target (dependents before dependencies). Cycles are tolerated via a visit set.

**Install pins still block `--force`.** Force must not silently delete product installs; run `lar uninstall <id>` first.

```bash
lar store remove --force org.example.lib 1.0.0
# removes org.example.app 0.1.0, then org.example.lib 1.0.0
# (fails if an install record still pins either package)
```

## CLI

```bash
lar store add app-0.1.0.lar
lar --system store add app-0.1.0.lar
lar store list
lar store remove org.example.editor 0.1.0
lar store remove --force org.example.lib 1.0.0
lar config
lar config --json
```

- `store add` verifies the `.lar`, extracts it into the store, and prints `id version -> path`.
- `store list` prints `id version content_hash` (sorted).
- `store remove` deletes one `(id, version)` if unused by packages or install pins; with `--force`, cascades through package dependents but still refuses install pins.
- `config` prints `prefix`, `store`, `runtimes`, `installs`, and whether system mode is active.

## Related

- Package format: [package-format.md](package-format.md)
- Resolve / lockfile: [resolve-lockfile.md](resolve-lockfile.md)
- Install records: [install.md](install.md)
- Design: [SxS Package Store](../design/architecture.md#sxs-package-store)
