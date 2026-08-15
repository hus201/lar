# Runtime Model

**Status:** Planned — linking and execution principles apply now; disposable runtime composition and `lar run` are not implemented yet.

## Side-by-Side Runtime

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

## Linking Model

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

## Application Execution Model

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

## Runtime Launching

Normal users should not need to interact directly with runtime management.

The runtime resolution process should be transparent.

A command similar to `lar run` may exist for:

- Development.
- Debugging.
- Testing.
- Administrative operations.

It is not required for normal application execution.
