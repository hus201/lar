# LAR Runtime Engine

`lar runtime build` composes a disposable runtime from a verified `lar.lock` and the local SxS store. Installed apps and PATH/desktop entry points then launch binaries from that runtime tree. `lar run` is a lockfile-oriented helper that builds or reuses a runtime and starts the root entry (development / debugging).

## Scope (v1)

- Input: path to `lar.lock` or a directory containing it
- All locked packages (including root) must be in the store with matching `content_hash`
- Configurable compose mode (default **symlink**); same relative path from two packages → error
- No package-source fetch or sandbox

## Compose modes

| Mode | Behavior |
|------|----------|
| `symlink` (default) | Relative symlinks into the SxS store (relocatable within the prefix) |
| `hardlink` | Hard links to store files (requires store and runtimes on the same filesystem) |
| `copy` | Byte copies into the runtime tree |

`runtime_id` includes the compose mode, so different modes never collide or incorrectly reuse each other.

## Layout

Content-addressed under the LAR prefix:

```text
{prefix}/runtimes/<runtime_id>/
  runtime.toml
  files/
    bin/...
    lib/...
```

`runtime_id` is a BLAKE3 digest over compose mode + canonical lock identity (sorted package id, version, and content_hash). Identical lock+mode pairs reuse the same directory.

`runtime.toml` records format, compose mode, root id/version, `runtime_id`, and the locked package list.

## Algorithm

1. Load and validate `lar.lock`.
2. Verify every package (including root) against the store (`verify_lockfile_ready`).
3. Compute `runtime_id`; if `{prefix}/runtimes/{runtime_id}` already exists with matching metadata, reuse it.
4. Otherwise compose under `.tmp-runtime-*` using the selected mode, write `runtime.toml`, rename into place.

`build`, `list`, `inspect`, and `gc` remove leftover `{prefix}/runtimes/.tmp-runtime-*` directories from failed or crashed builds.

## Launch environment

Every LAR launch (PATH trampoline, `.desktop` → export link, `lar launch`, and `lar run`) prepares the same process environment before `exec` of the entry ELF:

- Select the root `[entry]` default (or a listed binary).
- Prepend runtime `bin` / `usr/bin` (and sbin variants) to `PATH`.
- Prepend library roots (`lib`, `lib64`, `lib32`, `usr/lib*`, plus one subdirectory level such as `lib/x86_64-linux-gnu`) to `LD_LIBRARY_PATH`.
- Set `LAR_RUNTIME` to the runtime directory.

Installed applications normally start via PATH exports or desktop menus — [desktop.md](desktop.md). `lar run` only applies for a working-tree / lockfile workflow.

## Garbage collection

```bash
lar runtime gc          # remove broken/orphan runtimes (store packages missing or hash mismatch)
lar runtime gc --all    # remove every composed runtime
```

Runtimes are disposable; `--all` is safe if you can rebuild from lockfiles. Default mode only deletes runtimes that no longer match the store (and corrupt/orphan dirs under `runtimes/`).

## CLI

Runtime management:

```bash
lar runtime build
lar runtime build --compose hardlink
lar runtime build --compose copy path/to/dir-or-lar.lock
lar runtime list
lar runtime gc
lar runtime gc --all
lar runtime inspect <runtime_id-or-path>
lar runtime inspect --json <runtime_id-or-path>
```

- `runtime list` prints `runtime_id root_id root_version compose path` (sorted by id).
- `runtime inspect` prints root, id, compose, path, and packages (or JSON with `--json`).
- `runtime gc` prints each removed runtime/orphan and a summary (`broken` vs `orphan` counts).

### Lockfile launch (`lar run`)

For package authors working from a `lar.lock` (not the normal path for installed apps):

```bash
lar run
lar run --compose hardlink
lar run path/to/dir-or-lar.lock -- --help
```

```bash
lar package pack && lar store add *.lar
lar resolve
lar runtime build
lar run
```

Installed applications start via PATH exports or desktop menus — [desktop.md](desktop.md).
## Related

- Resolve / lockfile: [resolve-lockfile.md](resolve-lockfile.md)
- SxS store: [sxs-store.md](sxs-store.md)
- Desktop / PATH launch: [desktop.md](desktop.md)
- Design: [Runtime model](../design/runtime.md)
