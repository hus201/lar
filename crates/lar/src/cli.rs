use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

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
        /// Application package id
        app: String,
    },
    /// Install an application from a repository or local package
    Install {
        /// Application package id or path to a .lar file
        app: String,
    },
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
    /// Manage application repositories and package registries
    Repo {
        #[command(subcommand)]
        command: RepoCmd,
    },
    /// Print resolved configuration and store paths
    Config {
        /// Emit JSON instead of text
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PackageTypeArg {
    Application,
    Library,
    Runtime,
    Resource,
}

impl std::fmt::Display for PackageTypeArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Application => "application",
            Self::Library => "library",
            Self::Runtime => "runtime",
            Self::Resource => "resource",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Subcommand)]
pub enum PackageCmd {
    /// Write a package.toml template
    Init {
        /// Directory to create the package in (default: current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Reverse-DNS package id
        #[arg(long)]
        id: Option<String>,
        /// Human-readable name
        #[arg(long)]
        name: Option<String>,
        /// Package type
        #[arg(long, value_enum, default_value_t = PackageTypeArg::Application)]
        r#type: PackageTypeArg,
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
}

#[derive(Debug, Subcommand)]
pub enum RuntimeCmd {
    /// Compose a runtime environment from an application lockfile
    Build {
        /// Application package id
        app: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum RepoCmd {
    /// Add an application repository or package registry
    Add {
        /// Repository URL
        url: String,
        /// Source kind
        #[arg(long, value_parser = ["app", "registry"], default_value = "app")]
        r#type: String,
    },
    /// List configured repositories
    List,
    /// Remove a configured repository
    Remove {
        /// Repository URL or name
        source: String,
    },
}
