# LAR (Linux Application Runtime)

**Version:** `0.8.0` (first public MVP)

LAR is a Linux application runtime that decouples application lifecycle from the operating system. Packages are immutable and stored side-by-side; each application gets a resolved runtime instead of depending on `/usr/lib`.

Applications remain native ELF processes. LAR does not introduce a new ABI, a custom loader, or mandatory sandboxing.

## Usage

```bash
cargo run -p lar -- --help
```

Package commands:

```bash
cargo run -p lar -- package init --id org.example.editor --name "Example Editor"
cargo run -p lar -- package validate
cargo run -p lar -- package pack
cargo run -p lar -- package inspect org.example.editor-0.1.0.lar
cargo run -p lar -- package keygen --out ./keys
cargo run -p lar -- store add org.example.editor-0.1.0.lar
cargo run -p lar -- store list
cargo run -p lar -- store remove org.example.editor 0.1.0
cargo run -p lar -- repo add --main /path/to/repo
cargo run -p lar -- install org.example.app
cargo run -p lar -- launch org.example.app
cargo run -p lar -- update org.example.app
cargo run -p lar -- rollback org.example.app
cargo run -p lar -- audit
cargo run -p lar -- resolve
cargo run -p lar -- runtime build
cargo run -p lar -- run
cargo run -p lar -- config
```

Installed apps normally start from PATH exports or desktop menus after `install`. `resolve` / `runtime build` / `run` are for lockfile and package-author workflows.
See [docs/](docs/) for the documentation index: [proposal](docs/proposal.md), [design](docs/design/), [implementation](docs/implementation/), and [release 0.8.0](docs/releases/0.8.0.md).

## Build

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets
```

The CLI binaries are `lar` and `lar-exec` (PATH-export trampoline; separate package, light deps).

## Packages

```bash
./packaging/dist.sh   # needs cargo-deb and cargo-generate-rpm
```

Produces `dist/lar_*.deb`, `dist/lar-*.rpm`, and `dist/lar-*-linux-*.tar.gz`. Details: [packaging/README.md](packaging/README.md).
