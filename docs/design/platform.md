# Platform Boundary, Security, and Updates

**Status:** Partial — content hashes and install records (`lar install` / `uninstall` / `list`) are implemented; package signatures, update/rollback product flows, and OS integration policy remain planned — [install.md](../implementation/install.md)

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

- Package signatures.
- Content hashes.
- Metadata verification.

LAR does not define a mandatory security isolation model.

Application security continues using Linux mechanisms:

- User permissions.
- File permissions.
- Linux security frameworks.

## Update Model

Applications and operating systems have independent update cycles.

Application updates:

- Managed via package sources (repos) that allow applications.
- Resolve new runtime environments.
- Do not require OS upgrades.

Operating system updates:

- Managed by the OS.
- Focus on platform components.

## Rollback Model

Because packages are immutable:

- Previous runtimes remain available.
- Failed updates can be reverted.
- Multiple versions can coexist.

Rollback does not require rebuilding packages.
