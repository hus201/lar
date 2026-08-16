# Platform Boundary, Security, and Updates

**Status:** Partial — content hashes, package signatures (repo fetch), signed advisories/`lar audit`, install records, update/rollback, and **platform requirements (MVP)** are implemented; broader OS integration policy remains open — [repos.md](../implementation/repos.md), [install.md](../implementation/install.md), [package-format.md](../implementation/package-format.md)

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

## Platform requirements

**Status:** Implemented — MVP

Applications declare **LAR dependencies** (other LAR packages) and optionally **platform capabilities** that are not LAR packages. Platform requirements preserve the [OS/LAR boundary](#operating-system-boundary): LAR does not ship the desktop, GPU stack, or kernel inside `.lar` archives.

```text
Application
   │
   ├── LAR dependencies      → resolve into lar.lock / SxS store / runtime
   │
   └── Platform requirements → probe the host OS at install and launch
```

### Manifest

```toml
[platform]
requires = ["wayland", "dbus"]
optional = ["vulkan"]
```

Built-in ids (MVP): `wayland`, `x11`, `vulkan`, `opengl`, `dbus`, `dri`, `systemd-user`. Unknown ids are a validation error. Presence only (no min-version). A capability must not appear in both `requires` and `optional`.

Needs are the **union** of `[platform]` from the root package and every locked/installed dependency (each store `package.toml`).

### Enforcement

| Command | Behavior |
|---------|----------|
| `lar resolve` | LAR deps only — **no** host probes |
| `lar install` | After pins are in the store: fail if any **required** cap is missing; warn on stderr for missing **optional** |
| `lar launch` / PATH (`lar-exec`) | Same check before exec |
| `lar platform check [package.toml\|id]` | Print host probes and needs; exit non-zero if required missing |

Probes are **presence heuristics** (best-effort, no root): env vars, sockets, device nodes, or shared libraries on common search paths. A pass means the host *looks like* it has that surface — not that the compositor, GPU, or D-Bus service will actually work for the app. A fail is the useful half: refuse when the surface is obviously absent.

Tests may force outcomes with `LAR_PLATFORM_OVERRIDE=missing=wayland+x11,present=dbus`.

### Known limits (MVP)

- Presence heuristics only — not runtime / ICD / compositor verification
- No minimum-version or vendor-specific constraints
- No user-defined / plugin capabilities
- Platform caps are not pins in `lar.lock`
- OS stacks are never fetched into the SxS store

## Security Model

LAR provides package integrity through:

- Package signatures (Ed25519 over `content_hash` for repo fetch).
- Content hashes.
- Metadata verification.
- Repo-published vulnerability advisories (Ed25519-signed; warnings; refuse yank on new fetch).

LAR does not define a mandatory security isolation model and does not aim to become a sandboxing platform. Integrity and provenance are in scope; confinement is left to the OS and the application.

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
