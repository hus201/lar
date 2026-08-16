# Platform Boundary, Security, and Updates

**Status:** Partial — content hashes, package signatures (repo fetch), signed advisories/`lar audit`, install records, and update/rollback are implemented; OS integration policy remains planned — [repos.md](../implementation/repos.md), [install.md](../implementation/install.md)

## Operating System Boundary

The operating system provides platform functionality.

The OS owns:

- Linux Kernel.
- Hardware drivers.
- System services.
- Desktop Environment.
- Hardware integration.

LAR owns:

- Application packages.
- Application runtimes.
- Application dependencies.

## Security Model

LAR provides package integrity through:

- Package signatures (Ed25519 over `content_hash` for repo fetch).
- Content hashes.
- Metadata verification.
- Repo-published vulnerability advisories (Ed25519-signed; warnings; refuse yank on new fetch).

LAR does not define a mandatory security isolation model.

Application security continues using Linux mechanisms:

- User permissions.
- File permissions.
- Linux security frameworks.

SxS keeps previous `(id, version)` trees on disk until the user removes them. LAR surfaces advisory risk; it does not silently delete store packages.

## Update Model

Applications and operating systems have independent update cycles.

Application updates:

- Managed via package sources (repos) that allow applications (`lar update` picks the newest newer semver).
- Resolve new runtime environments.
- Keep one previous generation for rollback.
- Do not require OS upgrades.

Operating system updates:

- Managed by the OS.
- Focus on platform components.

## Rollback Model

Because packages are immutable:

- Previous runtimes remain available while stashed in `previous.toml`.
- Failed updates can be reverted with `lar rollback`.
- Multiple versions can coexist in the SxS store.

Rollback does not require rebuilding packages.
