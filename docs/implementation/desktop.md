# LAR Desktop Launch and PATH Exports

Implemented in `lar-manager` (`desktop`, `exports`) and the slim `lar-exec` binary (`lar-trampoline`). When an installed application has `[entry]`, LAR publishes a freedesktop `.desktop` file and PATH command links so menus and shells start the entry ELF through a native trampoline (no shell script; not the `lar` CLI).

## Scope (v1)

- Publish/remove `.desktop` and PATH exports on install, update, force replace, rollback, and uninstall
- Optional `[desktop]` metadata: `name`, `icon`, `categories`
- PATH exports: symlink → `{prefix}/libexec/lar-exec` + metadata; trampoline applies runtime env and `exec`s the entry
- CLI: `lar launch <app_id> [--binary <rel>] [-- args…]` remains for debug/admin
- Out of scope: MIME handlers, systemd services, portals, D-Bus activation, multi-binary extra desktop files, profile.d snippets

## Desktop layout

```text
{prefix}/share/applications/{app_id}.desktop   # LAR-owned canonical copy
```

Session copy (menus):

| Mode | Target |
|------|--------|
| user | `$XDG_DATA_HOME/applications/lar-{app_id}.desktop` (default `~/.local/share/applications/`) |
| system (`--system`) | `/usr/local/share/applications/lar-{app_id}.desktop` |

The `lar-` filename prefix avoids colliding with distro packages. Uninstall deletes both copies. `update-desktop-database` is invoked best-effort when present on `PATH`.

## PATH exports (native trampoline)

Command name = basename of each `[entry].binaries` path (e.g. `bin/firefox` → `firefox`). Duplicate basenames in one package are an error.

```text
{prefix}/bin/{cmd}                      → symlink to {prefix}/libexec/lar-exec
{prefix}/share/lar/exports/{cmd}.toml   # app_id, runtime, binary (absolute paths)
~/.local/bin/{cmd}                      → symlink to {prefix}/bin/{cmd}   # user mode
```

When the kernel runs `{cmd}`, it executes the `lar-exec` binary with `argv[0]` basename `{cmd}`. `lar-exec` loads the export metadata (by walking `argv[0]` / symlink targets for a `…/bin/{cmd}` under a LAR prefix), applies the shared [runtime launch environment](runtime.md#launch-environment), and `exec`s the entry binary.

Export metadata is rewritten whenever the install’s `runtime_id` changes (install replace / update / rollback). `{prefix}/libexec/lar-exec` is refreshed to the current `lar-exec` executable on publish and `lar launch` (resolved as a sibling of `lar`, or via `LAR_EXEC`).

Desktop `Exec` / `TryExec` point at the **prefix** `{prefix}/bin/{cmd}` link for the default entry binary.

**Collisions:** refusing to overwrite a path that is not a LAR export (symlink chain to `libexec/lar-exec`). Uninstall removes metadata and links for the app id.

**System store:** `--system` always uses prefix `/var/lib/lar`. Session `/usr/local/bin` is PATH-only, not the store.

If distro `/usr/bin/firefox` is earlier on `PATH` than `~/.local/bin`, the host binary wins; put `~/.local/bin` first to prefer LAR. On install/update/rollback, LAR **warns** when a non-LAR runnable earlier on `PATH` would shadow a published export (stderr: `warning: PATH: … shadows LAR export …`).

### Export metadata (`format = 1`)

```toml
format = 1
app_id = "org.example.app"
runtime = "/…/runtimes/<runtime_id>"
binary = "/…/runtimes/<runtime_id>/files/bin/app"
```

## `.desktop` contents

```ini
[Desktop Entry]
Type=Application
Version=1.5
Name=…
Exec=…/prefix/bin/{cmd}
TryExec=…/prefix/bin/{cmd}
Icon=…          # only if set
Categories=…;   # only if set
StartupNotify=true
```

- **Name:** `[desktop].name`, else `package.name`
- **Exec / TryExec:** absolute path to the prefix PATH export link for the default entry binary
- **Icon:** if `[desktop].icon` is a relative payload path, an absolute path under the store package tree (`…/files/…`). Missing icons fail install publish.
- Packages without `[entry]` do not get a desktop file or PATH exports.

## `lar launch`

Debug/admin path for an installed app (menus and PATH exports are the normal entry points):

1. Load the install record for `app_id`
2. Require `{prefix}/runtimes/{runtime_id}` (suggest reinstall if missing)
3. Require root package `[entry]`; optional `--binary` must be listed in `entry.binaries`
4. Apply the shared [runtime launch environment](runtime.md#launch-environment) and `exec` the binary

```bash
lar launch org.example.app
lar launch org.example.app --binary bin/helper -- --help
```

## Lifecycle hooks

- After successful activate (install / update / force replace / rollback): publish PATH exports then desktop if the root has `[entry]`; otherwise remove any stale files
- On uninstall: remove desktop and PATH exports before deleting the install directory

Paths are derived from `app_id` / entry basenames / `runtime_id` and prefix/system mode; `install.toml` format is unchanged.

## Related

- Install records: [install.md](install.md)
- Package `[desktop]` / `[entry]`: [package-format.md](package-format.md)
- Runtime launch: [runtime.md](runtime.md)
- Design: [desktop-and-services.md](../design/desktop-and-services.md)
