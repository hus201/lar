# LAR (Linux Application Runtime)

LAR is a Linux application runtime that decouples application lifecycle from the operating system. Packages are immutable and stored side-by-side; each application gets a resolved runtime instead of depending on `/usr/lib`.

Applications remain native ELF processes. LAR does not introduce a new ABI, a custom loader, or mandatory sandboxing.

## Usage

```bash
cargo run -p lar -- --help
```

Commands are defined but not implemented yet.

## Build

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets
```

The CLI binary is `lar`.
