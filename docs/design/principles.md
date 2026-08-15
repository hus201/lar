# Design Principles

**Status:** Design

## Native Linux Compatibility

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

## Runtime Resolution Instead of Global Installation

Applications should not depend on global system paths.

Instead of installing dependencies into `/usr/lib` or other global locations, LAR resolves an application-specific runtime.

Each application receives the dependency environment it requires.

## Immutable Package Storage

Packages stored by LAR are immutable.

A package version, once created, cannot be modified.

Benefits:

- Reproducibility.
- Safe upgrades.
- Multiple versions.
- Reliable rollback.

## Application Independence

Applications should be able to evolve independently from the operating system.

Application updates should not require:

- Distribution repository changes.
- Full operating system upgrades.
- System-wide dependency changes.
