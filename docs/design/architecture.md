# Architecture Overview

**Status:** Partial — SxS store, resolver (lockfile + version ranges), runtime, install records (including update/rollback), package sources (fetch/signatures/advisories), and desktop/PATH launch (`.desktop` + exports) are implemented.

LAR consists of four major components.

## Package sources (repos)

**Status:** Implemented (foundation) — [repos.md](../implementation/repos.md)

A **package source** (CLI: `lar repo`) is a remote or local index that distributes LAR packages (`.lar` archives and metadata). There is a single source abstraction — not a separate “application repository” vs “package registry.”

That matches the package model: **one package kind**. Libraries, frameworks, and applications are the same artifact shape; capabilities come from the manifest (`[entry]`, `[desktop]`, dependencies), not from which subsystem published them.

### Responsibilities

- Package distribution and metadata
- Package versions and integrity (content hashes and Ed25519 signatures)
- Serving content for resolve-time fetch into the SxS store
- Publishing vulnerability advisories / yank metadata (LAR warns; does not auto-purge)

Sources are decentralized. Examples:

- A shared **main** dependency ecosystem
- Vendor sources
- Community sources
- Enterprise sources
- Local/offline mirrors

### Source policy

Policy is attached to the **source**, not inferred from the package id.

| Policy | Meaning |
|--------|---------|
| `deps` | Resolver may fetch packages from this source to satisfy `[dependencies]` |
| `apps` | `lar install <id>` may select a root application from this source |

A source may allow `deps`, `apps`, or both.

**Default policy for the main source:** **dependency-only** (`deps`, not `apps`).

- Users do not install applications from main.
- Applications come from sources that allow `apps` (vendor/community/enterprise) or from a local `.lar`.
- Resolve may still pull shared libraries and runtimes from main.

App-capable sources may also publish their own dependencies (air-gapped or fully pinned stacks) even when main exists.

### Source priority

When fetching a missing dependency, search order is:

1. Local SxS store (already present → use it; no fetch)
2. **main** (if configured and allows `deps`)
3. Other configured sources that allow `deps`, in configuration order

First exact `(id, version)` hit wins. Do not merge or prefer a later source’s copy of the same pin.

For `lar install <id>`, only sources with `apps` are considered (main is skipped when it is deps-only), in configuration order, after local `.lar` / store.

### Install vs resolve lookup

- **`lar install <id>`** — search only sources with `apps` (plus local `.lar` / already-in-store).
- **Resolve / fetch missing deps** — search sources with `deps`, **main first**, then others.

Exact pins and the local SxS store remain the source of truth after fetch; repos do not replace the store.

## SxS Package Store

**Status:** Implemented — [sxs-store.md](../implementation/sxs-store.md)

The local immutable package storage.

Responsibilities:

- Store resolved packages.
- Maintain multiple versions.
- Provide package content for runtime creation.
- Enforce remove referrers (package deps and install pins).

The SxS Store is the source of truth for packages present on the machine.

## Runtime Resolver

**Status:** Partial — resolve/lockfile (including fetch and version ranges), runtime compose and launch environment, install records, and desktop/PATH launch are implemented — [resolve-lockfile.md](../implementation/resolve-lockfile.md), [runtime.md](../implementation/runtime.md), [install.md](../implementation/install.md), [desktop.md](../implementation/desktop.md), [repos.md](../implementation/repos.md)

Responsible for creating application execution environments.

Responsibilities:

- Read application manifests.
- Resolve dependencies (store + fetch from package sources).
- Select compatible package versions.
- Create runtime environments.
- Launch applications.

## Application lifecycle

**Status:** Partial — [install.md](../implementation/install.md), [desktop.md](../implementation/desktop.md)

Install records under `{prefix}/installs/` track what the user installed, pin store packages, and point at a composed runtime. `lar update` / `lar rollback` keep a single previous generation. Apps with `[entry]` get freedesktop `.desktop` files and PATH exports; menus and shells are the normal launch path (`lar launch` is admin/debug).
