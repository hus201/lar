# Architecture Overview

**Status:** Partial — SxS store, resolver (lockfile), runtime, and install records are implemented; application repositories and package registries are planned.

LAR consists of four major components.

## Application Repository

**Status:** Planned

Provides applications.

Responsibilities:

- Application distribution.
- Application metadata.
- Application versions.
- Application signatures.

Application repositories are decentralized.

Examples:

- Vendor repositories.
- Community repositories.
- Enterprise repositories.

## Package Registry

**Status:** Planned

Provides shared packages required by applications.

Responsibilities:

- Library packages.
- Runtime components.
- Frameworks.
- Dependency metadata.

The Package Registry is the shared dependency ecosystem.

## SxS Package Store

**Status:** Implemented — [sxs-store.md](../implementation/sxs-store.md)

The local immutable package storage.

Responsibilities:

- Store resolved packages.
- Maintain multiple versions.
- Provide package content for runtime creation.

The SxS Store is the source of truth for installed packages.

## Runtime Resolver

**Status:** Partial — resolve/lockfile, runtime compose/`lar run`, and install records are implemented; version ranges, fetch, and desktop launch planned — [resolve-lockfile.md](../implementation/resolve-lockfile.md), [runtime.md](../implementation/runtime.md), [install.md](../implementation/install.md)

Responsible for creating application execution environments.

Responsibilities:

- Read application manifests.
- Resolve dependencies.
- Select compatible package versions.
- Create runtime environments.
- Launch applications.
