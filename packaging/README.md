# Dist packaging

Build release artifacts for LAR (`lar` + `lar-exec`):

| Artifact | Tool |
|----------|------|
| Native `.tar.gz` | `packaging/dist.sh` |
| Debian `.deb` | [cargo-deb](https://crates.io/crates/cargo-deb) |
| RPM (DNF/yum) `.rpm` | [cargo-generate-rpm](https://crates.io/crates/cargo-generate-rpm) |

## Prerequisites

```bash
cargo install cargo-deb cargo-generate-rpm
# Debian/Ubuntu hosts also need dpkg tools (usually preinstalled).
# RPM generation does not require rpmbuild when using cargo-generate-rpm.
```

## Build all

```bash
./packaging/dist.sh
```

Outputs land in `dist/`:

- `lar-<version>-linux-<arch>.tar.gz`
- `lar_<version>-1_<debarch>.deb` (typical)
- `lar-<version>-1.<arch>.rpm` (typical)

## Install

```bash
# Debian / Ubuntu
sudo dpkg -i dist/lar_*.deb

# Fedora / RHEL / openSUSE (dnf/yum/zypper)
sudo dnf install ./dist/lar-*.rpm

# Native tarball
tar -xzf dist/lar-*-linux-*.tar.gz
sudo install -m 755 lar-*/bin/lar lar-*/bin/lar-exec /usr/local/bin/
```

Both binaries must share a directory so `lar` can resolve the `lar-exec` sibling (or set `LAR_EXEC`).
