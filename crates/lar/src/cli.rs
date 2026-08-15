use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "lar",
    version,
    about = "Linux Application Runtime",
    long_about = "LAR manages native Linux applications with immutable side-by-side packages \
and resolved runtimes, so application lifecycle is independent from the OS."
)]
pub struct Cli {
    /// Use the system prefix (`/var/lib/lar`) instead of the user prefix.
    #[arg(long, global = true)]
    pub system: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Create, validate, and pack LAR packages
    Package {
        #[command(subcommand)]
        command: PackageCmd,
    },
    /// Manage the local side-by-side package store
    Store {
        #[command(subcommand)]
        command: StoreCmd,
    },
    /// Resolve application dependencies into a lockfile
    Resolve {
        /// Path to package.toml or a directory containing it
        #[arg(default_value = "package.toml")]
        manifest: PathBuf,
    },
    /// Compose and inspect disposable runtime environments
    Runtime {
        #[command(subcommand)]
        command: RuntimeCmd,
    },
    /// Launch an application using its resolved runtime (debug/admin)
    Run {
        /// Path to lar.lock or a directory containing it
        #[arg(default_value = ".")]
        lockfile: PathBuf,
        /// How to materialize package files into the runtime
        #[arg(long, default_value = "symlink", value_parser = ["symlink", "hardlink", "copy"])]
        compose: String,
        /// Arguments forwarded to the application entry binary
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install an application from a local .lar, the store, or an apps source
    Install {
        /// Path to a `.lar`, or a package `id` / `id@version`
        app: String,
        /// How to materialize package files into the runtime
        #[arg(long, default_value = "symlink", value_parser = ["symlink", "hardlink", "copy"])]
        compose: String,
        /// Replace an existing install of the same id
        #[arg(long)]
        force: bool,
    },
    /// List installed applications
    List,
    /// Update an installed application
    Update {
        /// Application package id
        app: String,
    },
    /// Revert an application to a previously installed version
    Rollback {
        /// Application package id
        app: String,
    },
    /// Remove an installed application
    Uninstall {
        /// Application package id
        app: String,
    },
    /// Manage package sources (repos)
    Repo {
        #[command(subcommand)]
        command: RepoCmd,
    },
    /// Audit packages against repo vulnerability advisories
    Audit {
        /// Scan every package in the SxS store
        #[arg(long)]
        store: bool,
        /// Scan pins from install records (default)
        #[arg(long)]
        installed: bool,
    },
    /// Print resolved configuration and store paths
    Config {
        /// Emit JSON instead of text
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum PackageCmd {
    /// Write a package.toml template
    Init {
        /// Directory to create the package in (default: current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Reverse-DNS package id (required)
        #[arg(long)]
        id: String,
        /// Human-readable name
        #[arg(long)]
        name: Option<String>,
        /// Semver version
        #[arg(long, default_value = "0.1.0")]
        version: String,
        /// Overwrite an existing package.toml
        #[arg(long)]
        force: bool,
    },
    /// Parse and validate a package.toml
    Validate {
        /// Path to package.toml or a directory containing it
        #[arg(default_value = "package.toml")]
        manifest: PathBuf,
    },
    /// Pack a staged directory into a .lar archive
    Pack {
        /// Directory containing package.toml and payload files
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Output .lar path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Inspect a .lar archive and verify payload digests
    Inspect {
        /// Path to a .lar archive
        package: PathBuf,
        /// Emit JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// Generate an Ed25519 keypair for signing package indexes
    Keygen {
        /// Directory to write ed25519.pub / ed25519.sec
        #[arg(long, default_value = ".")]
        out: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum StoreCmd {
    /// Add a .lar package to the local store
    Add {
        /// Path to a .lar archive
        package: PathBuf,
    },
    /// List packages in the local store
    List,
    /// Remove a package version from the local store
    Remove {
        /// Package id
        id: String,
        /// Package version
        version: String,
        /// Also remove packages that depend on this one (cascade)
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum RuntimeCmd {
    /// Compose a runtime environment from an application lockfile
    Build {
        /// Path to lar.lock or a directory containing it
        #[arg(default_value = ".")]
        lockfile: PathBuf,
        /// How to materialize package files into the runtime
        #[arg(long, default_value = "symlink", value_parser = ["symlink", "hardlink", "copy"])]
        compose: String,
    },
    /// List composed runtimes under the LAR prefix
    List,
    /// Remove unused or broken composed runtimes
    Gc {
        /// Remove every composed runtime (not only broken ones)
        #[arg(long)]
        all: bool,
    },
    /// Inspect a composed runtime by id or path
    Inspect {
        /// Runtime id or path to a runtime directory / runtime.toml
        runtime: PathBuf,
        /// Emit JSON instead of text
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum RepoCmd {
    /// Add a package source
    Add {
        /// Local path or http(s) base URI
        uri: String,
        /// What this source may provide: deps only, apps only, or both
        /// (default: deps with --main, otherwise both)
        #[arg(long, value_parser = ["deps", "apps", "both"])]
        policy: Option<String>,
        /// Mark as the main dependency source (unique; default policy deps)
        #[arg(long)]
        main: bool,
        /// Source name (default: main if --main, else basename/host)
        #[arg(long)]
        name: Option<String>,
    },
    /// List configured package sources
    List,
    /// Remove a configured package source
    Remove {
        /// Source name or URI
        source: String,
    },
    /// Write index.toml for a local repo directory (signed)
    Index {
        /// Directory containing .lar packages (and optional packages/)
        dir: PathBuf,
        /// Path to Ed25519 secret key file or inline `base64:…` key
        #[arg(long)]
        sign_key: String,
    },
    /// Manage trusted publisher public keys
    Trust {
        #[command(subcommand)]
        command: TrustCmd,
    },
}

#[derive(Debug, Subcommand)]
pub enum TrustCmd {
    /// Trust a publisher public key
    Add {
        /// Path to pubkey file or `base64:…` string
        pubkey: String,
        #[arg(long)]
        comment: Option<String>,
    },
    /// List trusted keys
    List,
    /// Remove a trusted key by id
    Remove { key_id: String },
}
