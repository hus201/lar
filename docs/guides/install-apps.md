# Install and update apps

Install records, PATH/desktop integration, and update/rollback. Details: [install.md](../implementation/install.md), [desktop.md](../implementation/desktop.md).

## Prerequisites

- App package in the store, as a `.lar` path, or fetchable from a configured source — [use-repo.md](use-repo.md)
- Dependencies available in the store or from configured sources

## Install

```bash
# From a local archive
lar install ./org.example.app-0.1.0.lar

# From store / package sources (id, optional @version)
lar install org.example.app
lar install org.example.app@0.1.0

lar list
lar launch org.example.app
```

Apps with `[entry]` also get PATH exports and a `.desktop` file under the LAR prefix. Day-to-day use is usually the desktop menu or an export on `PATH`, not `lar launch`.

Replace an existing install:

```bash
lar install --force org.example.app@0.2.0
```

## Update and rollback

```bash
lar update org.example.app      # newest newer semver from configured sources
lar rollback org.example.app    # swap with the single previous generation
```

Only one previous generation is kept.

## Uninstall

```bash
lar uninstall org.example.app
```

Removes the install record, desktop/PATH publish for that app, and unused runtimes. Store packages remain until you `lar store remove` (and nothing else refers to them).

## Lockfile / author workflow

For developing against a lockfile without an install record:

```bash
lar resolve
lar runtime build
lar run
```

Installed users should prefer install + PATH/desktop; `lar run` is for package authors and CI.
