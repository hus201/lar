# Create a package

Build a staged package tree, validate it, and produce a `.lar` archive.

Format details: [package-format.md](../implementation/package-format.md).

## 1. Initialize

```bash
lar package init ./my-lib \
  --id org.example.lib \
  --name "Example Lib" \
  --version 1.0.0
```

Layout:

```text
my-lib/
  package.toml
  files/
```

Put payload under `files/` (regular files and directories only — no symlinks). For a library, that is often `files/lib/…`. For an app, include binaries under `files/bin/…` and set `[entry]` (and optional `[desktop]`) in `package.toml`.

## 2. Edit the manifest

Minimum:

```toml
[package]
format = 1
id = "org.example.lib"
name = "Example Lib"
version = "1.0.0"
```

App example extras:

```toml
[dependencies]
"org.example.lib" = "^1.0"

[entry]
default = "bin/app"
binaries = ["bin/app"]

[desktop]
name = "Example App"
categories = ["Utility"]

# Optional: host OS capabilities (not LAR packages)
[platform]
requires = ["wayland"]
optional = ["vulkan"]
```

Debug host probes without installing: `lar platform check ./my-lib` (or an installed app id). Details: [platform.md](../design/platform.md#platform-requirements).

## 3. Validate and pack

```bash
lar package validate ./my-lib
lar package pack ./my-lib
# → my-lib/org.example.lib-1.0.0.lar
lar package inspect ./my-lib/org.example.lib-1.0.0.lar
```

`pack` writes `content_hash` into the archived manifest. Inspect prints id, version, and hash.

## 4. Local store (optional)

Without a published source, add the archive directly:

```bash
lar store add ./my-lib/org.example.lib-1.0.0.lar
lar store list
```

Unsigned store adds are allowed. Fetching from a package source always requires signatures and trust — [use-repo.md](use-repo.md).

## Next

- Publish to a source: [publish-repo.md](publish-repo.md)
- Install an app that depends on this package: [install-apps.md](install-apps.md)
