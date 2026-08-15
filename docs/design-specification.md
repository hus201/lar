# LAR (Linux Application Runtime)
## Design Specification

---

# 1. Introduction

LAR (Linux Application Runtime) is a Linux application runtime platform designed to provide a consistent application execution model independent from Linux distribution release cycles.

LAR introduces:

- Package-based application management.
- Side-by-Side (SxS) package storage.
- Runtime resolution.
- Application dependency management.
- Decentralized application distribution.

The primary objective is to separate:

- Operating system lifecycle.
- Application lifecycle.

The Linux platform provides the foundation, while LAR manages application execution environments.

---

# 2. Design Principles

## 2.1 Native Linux Compatibility

LAR preserves the native Linux application model.

LAR applications continue using:

- ELF binaries.
- Linux system calls.
- Shared libraries.
- Existing desktop protocols.

LAR does not introduce:

- A new binary format.
- A replacement Linux ABI.
- A custom execution model.

---

## 2.2 Runtime Resolution Instead of Global Installation

Applications should not depend on global system paths.

Instead of installing dependencies into:

/usr/lib

or other global locations, LAR resolves an application-specific runtime.

Each application receives the dependency environment it requires.

---

## 2.3 Immutable Package Storage

Packages stored by LAR are immutable.

A package version, once created, cannot be modified.

Benefits:

- Reproducibility.
- Safe upgrades.
- Multiple versions.
- Reliable rollback.

---

## 2.4 Application Independence

Applications should be able to evolve independently from the operating system.

Application updates should not require:

- Distribution repository changes.
- Full operating system upgrades.
- System-wide dependency changes.

---

# 3. Architecture Overview

LAR consists of four major components:

## 3.1 Application Repository

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

---

## 3.2 Package Registry

Provides shared packages required by applications.

Responsibilities:

- Library packages.
- Runtime components.
- Frameworks.
- Dependency metadata.

The Package Registry is the shared dependency ecosystem.

---

## 3.3 SxS Package Store

The local immutable package storage.

Responsibilities:

- Store resolved packages.
- Maintain multiple versions.
- Provide package content for runtime creation.

The SxS Store is the source of truth for installed packages.

---

## 3.4 Runtime Resolver

Responsible for creating application execution environments.

Responsibilities:

- Read application manifests.
- Resolve dependencies.
- Select compatible package versions.
- Create runtime environments.
- Launch applications.

---

# 4. Package Model

Everything managed by LAR is represented as a package.

Package categories:

## Application Package

Represents user-facing software.

Examples:

- Firefox.
- Blender.
- LibreOffice.

---

## Library Package

Provides reusable libraries.

Examples:

- Qt.
- GTK.
- FFmpeg.
- OpenSSL.

---

## Runtime Component Package

Provides execution components.

Examples:

- Language runtimes.
- Frameworks.
- Supporting services.

---

## Resource Package

Provides shared resources.

Examples:

- Icons.
- Localization files.
- Assets.

---

# 5. Package Identity

Every package must have:

- Unique identifier.
- Version.
- Type.
- Metadata.
- Integrity information.

Example:

Package:

org.qt.qtbase

Version:

6.8.1

Type:

Library

---

# 6. Application Manifest

Applications define their requirements through a manifest.

The manifest describes:

- Application identity.
- Version.
- Package type.
- Dependencies.
- Runtime requirements.
- Desktop integration metadata.

Example information:

Application:

org.example.editor

Dependencies:

- org.qt.qtbase 6.8.1
- org.ffmpeg 7.1.0

---

# 7. Dependency Resolution

Dependency resolution is based on application requirements.

Resolution process:

1. Application manifest is loaded.
2. Required dependencies are identified.
3. Available packages are checked.
4. Compatible versions are selected.
5. Missing packages are retrieved.
6. Runtime environment is created.

The goal is deterministic runtime creation.

---

# 8. Side-by-Side Runtime Model

A runtime is a generated environment created from packages.

A runtime contains:

- Application executable.
- Required libraries.
- Runtime components.
- Resources.

Runtime construction may use:

- Symbolic links.
- Hard links.
- Filesystem composition techniques.

The runtime itself is disposable.

The SxS Store remains the permanent package storage.

---

# 9. Linking Model

LAR preserves the existing Linux dynamic linking model.

Applications continue using:

- ELF binaries.
- Shared libraries.
- Linux dynamic loader.

LAR does not replace:

- ELF.
- The Linux loader.
- The Linux ABI.

LAR changes dependency availability, not Linux execution behavior.

The application sees the resolved runtime environment instead of relying on distribution-wide libraries.

---

# 10. Application Execution Model

Applications execute as native Linux processes.

LAR is responsible for preparing the environment before execution.

Applications do not require:

- A special binary format.
- A custom virtual machine.
- Mandatory sandboxing.

After installation, applications can be launched through:

- Desktop launchers.
- Command line.
- Service managers.

---

# 11. Runtime Launching

Normal users should not need to interact directly with runtime management.

The runtime resolution process should be transparent.

A command similar to:

lar run

may exist for:

- Development.
- Debugging.
- Testing.
- Administrative operations.

It is not required for normal application execution.

---

# 12. Desktop Integration

The Desktop Environment remains part of the operating system.

LAR applications integrate through standard Linux interfaces:

- Wayland.
- D-Bus.
- Desktop Entry specification.
- MIME handling.
- Notifications.
- Portals.

Applications should not depend on desktop implementation internals.

---

# 13. Service Application Support

LAR supports non-GUI applications.

Application categories include:

- Desktop Applications.
- CLI Applications.
- Background Applications.
- Service Applications.

Examples:

- PostgreSQL.
- Redis.
- Nginx.
- RabbitMQ.

Service applications use:

- Application packages.
- Runtime resolution.
- Service metadata.

The operating system service manager manages lifecycle.

LAR provides the runtime environment.

---

# 14. Operating System Boundary

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

---

# 15. Security Model

LAR provides package integrity through:

- Package signatures.
- Content hashes.
- Metadata verification.

LAR does not define a mandatory security isolation model.

Application security continues using Linux mechanisms:

- User permissions.
- File permissions.
- Linux security frameworks.

---

# 16. Update Model

Applications and operating systems have independent update cycles.

Application updates:

- Managed by application repositories.
- Resolve new runtime environments.
- Do not require OS upgrades.

Operating system updates:

- Managed by the OS.
- Focus on platform components.

---

# 17. Rollback Model

Because packages are immutable:

- Previous runtimes remain available.
- Failed updates can be reverted.
- Multiple versions can coexist.

Rollback does not require rebuilding packages.

---

# 18. Future Extensions

Possible future features:

- Runtime garbage collection.
- Developer SDK.
- Enterprise package registries.
- Build system integration.
- Application permission models.
- Runtime sharing optimization.

---

# 19. Design Summary

LAR introduces a new application management layer for Linux.

The architecture separates:

Linux Platform:

- Kernel.
- Drivers.
- Desktop.
- System services.

LAR:

- Package management.
- Runtime resolution.
- Dependency management.

Applications:

- Independent software lifecycle.
- Own declared requirements.

The core principle:

Applications should run as native Linux applications while having predictable, reproducible, and independent runtime environments.
