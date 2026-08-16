# Architecture Overview

**Status:** MVP implemented

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

### Source priority

Configured sources are ordered: **earlier entries in `sources.toml` are higher priority**.

When resolving or fetching:

1. Collect candidate versions that satisfy the requirement (store ∪ sources, non-yanked).
2. Select the **highest compatible** semver version.
3. If that exact `(id, version)` exists in multiple sources, take it from the **highest-priority** source.
4. **Never merge** package contents from different sources — one source supplies the whole pin.

Local SxS store wins if the exact `(id, version)` is already present (no fetch).

```bash
lar repo list   # prints sources in priority order (1 = highest)
lar repo move overlay --top
lar repo move overlay --before upstream
lar repo move overlay --to 1
```

Edit `sources.toml` only if you prefer hand-editing. Exact pins and the local store remain the source of truth after fetch.

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

**Status:** Implemented — MVP — [resolve-lockfile.md](../implementation/resolve-lockfile.md), [runtime.md](../implementation/runtime.md), [install.md](../implementation/install.md), [desktop.md](../implementation/desktop.md), [repos.md](../implementation/repos.md)

Responsible for creating application execution environments.

Responsibilities:

- Read application manifests.
- Resolve dependencies (store + fetch from package sources).
- Select compatible package versions.
- Create runtime environments.
- Launch applications.

Known limitations:

- One rollback generation
- No platform requirement model yet
- Real-world ELF compatibility still being validated

## Application Lifecycle

**Status:** Implemented — MVP — [install.md](../implementation/install.md), [desktop.md](../implementation/desktop.md)

Install records under `{prefix}/installs/` track what the user installed, pin store packages, and point at a composed runtime. `lar update` / `lar rollback` keep a single previous generation. Apps with `[entry]` get freedesktop `.desktop` files and PATH exports; menus and shells are the normal launch path (`lar launch` is admin/debug).

Known limitations:

- One rollback generation
- No platform requirement model yet
- Real-world ELF compatibility still being validated
