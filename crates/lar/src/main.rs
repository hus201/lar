mod cli;

use std::path::Path;
use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Commands, PackageCmd, RepoCmd, RuntimeCmd, StoreCmd};
use lar_package::{init_package, inspect, pack, validate_package, InitOptions};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let _ = cli.system;

    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Commands) -> Result<(), String> {
    match command {
        Commands::Package { command } => run_package(command),
        other => Err(format!("{}: not implemented yet", command_name(&other))),
    }
}

fn run_package(command: PackageCmd) -> Result<(), String> {
    match command {
        PackageCmd::Init {
            dir,
            id,
            name,
            version,
            force,
        } => {
            let name = name.unwrap_or_else(|| default_package_name(&dir, &id));
            let path = init_package(
                &dir,
                &InitOptions {
                    id,
                    name,
                    version,
                    force,
                },
            )
            .map_err(|err| err.to_string())?;
            println!("wrote {}", path.display());
            Ok(())
        }
        PackageCmd::Validate { manifest } => {
            let parsed = validate_package(&manifest).map_err(|err| err.to_string())?;
            println!("ok {} {}", parsed.package.id, parsed.package.version);
            Ok(())
        }
        PackageCmd::Pack { dir, output } => {
            let manifest_path =
                lar_package::resolve_manifest_path(&dir).map_err(|err| err.to_string())?;
            let package_dir = lar_package::package_dir_from_manifest(&manifest_path)
                .map_err(|err| err.to_string())?;
            let manifest =
                lar_package::load_manifest(&manifest_path).map_err(|err| err.to_string())?;
            let output = output.unwrap_or_else(|| {
                package_dir.join(format!(
                    "{}-{}.lar",
                    manifest.package.id, manifest.package.version
                ))
            });
            let packed = pack(&package_dir, &output).map_err(|err| err.to_string())?;
            println!(
                "wrote {} ({})",
                output.display(),
                packed
                    .manifest
                    .package
                    .content_hash
                    .as_deref()
                    .unwrap_or("blake3:?")
            );
            Ok(())
        }
        PackageCmd::Inspect { package, json } => {
            let archive = inspect(&package).map_err(|err| err.to_string())?;
            if json {
                let value = serde_json::json!({
                    "format": archive.index.format,
                    "id": archive.index.id,
                    "version": archive.index.version,
                    "content_hash": archive.index.content_hash,
                    "files": archive.index.files.iter().map(|f| serde_json::json!({
                        "path": f.path,
                        "blake3": f.blake3,
                        "size": f.size,
                    })).collect::<Vec<_>>(),
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
                );
            } else {
                println!(
                    "{} {} (format {})",
                    archive.manifest.package.id,
                    archive.manifest.package.version,
                    archive.manifest.package.format
                );
                println!("{}", archive.index.content_hash);
                println!("{} files", archive.index.files.len());
                for file in &archive.index.files {
                    println!("  {}  {}  {}", file.size, file.blake3, file.path);
                }
            }
            Ok(())
        }
    }
}

fn default_package_name(dir: &Path, id: &str) -> String {
    package_dir_stem(dir).unwrap_or_else(|| {
        id.rsplit('.')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(id)
            .to_string()
    })
}

/// Resolve `dir` to a concrete folder name (so `.` uses the cwd basename).
fn package_dir_stem(dir: &Path) -> Option<String> {
    let absolute = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(dir)
    };
    let resolved = std::fs::canonicalize(&absolute).unwrap_or(absolute);
    resolved
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && *s != "/" && *s != ".")
        .map(|s| s.to_string())
}

fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Package { command } => match command {
            PackageCmd::Init { .. } => "lar package init",
            PackageCmd::Validate { .. } => "lar package validate",
            PackageCmd::Pack { .. } => "lar package pack",
            PackageCmd::Inspect { .. } => "lar package inspect",
        },
        Commands::Store { command } => match command {
            StoreCmd::Add { .. } => "lar store add",
            StoreCmd::List => "lar store list",
        },
        Commands::Resolve { .. } => "lar resolve",
        Commands::Runtime { command } => match command {
            RuntimeCmd::Build { .. } => "lar runtime build",
        },
        Commands::Run { .. } => "lar run",
        Commands::Install { .. } => "lar install",
        Commands::Update { .. } => "lar update",
        Commands::Rollback { .. } => "lar rollback",
        Commands::Uninstall { .. } => "lar uninstall",
        Commands::Repo { command } => match command {
            RepoCmd::Add { .. } => "lar repo add",
            RepoCmd::List => "lar repo list",
            RepoCmd::Remove { .. } => "lar repo remove",
        },
        Commands::Config { .. } => "lar config",
    }
}
