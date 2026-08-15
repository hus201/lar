# Package Model

**Status:** Implemented — [package-format.md](../implementation/package-format.md)

Everything managed by LAR is represented as a package.

LAR uses a **single package kind**. There is no required package `type` or `role` field.

A package is defined by:

- Identity and version.
- Optional dependencies.
- Optional entry binaries (one or many).
- Optional desktop integration metadata.
- An immutable payload (`files/`).

Capabilities are inferred from what the manifest declares. For example:

- Presence of `[entry]` / `[desktop]` supports launch and desktop install flows.
- Absence of entry still allows the package to be stored and composed into another application's runtime as a dependency.

Illustrative contents (not separate formal types):

- Applications such as Firefox, Blender, or LibreOffice.
- Libraries such as Qt, GTK, FFmpeg, or OpenSSL.
- Execution stacks such as language runtimes (Python, Node.js, JVM distributions).
- Resources such as icons, localization files, or assets.

Terminology note:

- A **package** is an immutable artifact in the SxS store.
- The **runtime resolver** selects packages and builds environments.
- A **runtime environment** is a disposable composed filesystem for execution.

Do not add a type/role enum unless a long-term system blocker requires it (policy that cannot be expressed via entry, desktop metadata, dependencies, or payload conventions).

## Package Identity

Every package must have:

- Unique identifier.
- Version.
- Metadata.
- Integrity information.

Example:

- Package: `org.qt.qtbase`
- Version: `6.8.1`

## Application Manifest

Applications define their requirements through a manifest.

The manifest describes:

- Package identity.
- Version.
- Dependencies.
- Optional entry binaries.
- Optional desktop integration metadata.
- Runtime requirements as needed.

Example:

- Package: `org.example.editor`
- Dependencies:
  - `org.qt.qtbase` `6.8.1`
  - `org.ffmpeg` `7.1.0`

See also: [Package format implementation](../implementation/package-format.md).
