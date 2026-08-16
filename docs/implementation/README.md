# Implementation Docs

These documents describe shipped (or actively implemented) formats and subsystems. Design intent is under [../design/](../design/).

| Doc | Topic |
|-----|--------|
| [package-format.md](package-format.md) | `package.toml`, `.lar` archives, integrity |
| [sxs-store.md](sxs-store.md) | Local SxS store layout, add/list/remove |
| [resolve-lockfile.md](resolve-lockfile.md) | `lar resolve` and `lar.lock` |
| [runtime.md](runtime.md) | Runtime compose, launch environment, list/inspect/gc |
| [install.md](install.md) | Install records via `lar-manager` (`lar install` / `list` / `update` / `rollback` / `uninstall`) |
| [desktop.md](desktop.md) | Freedesktop `.desktop` publish, PATH exports, `lar-exec` trampoline |
| [repos.md](repos.md) | Package sources, signatures, advisories, `lar audit` |

How-tos: [../guides/](../guides/).
