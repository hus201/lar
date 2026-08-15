mod cli;

use std::path::Path;
use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Commands, PackageCmd, RepoCmd, RuntimeCmd, StoreCmd};
use lar_package::{init_package, inspect, pack, validate_package, InitOptions};
use lar_resolver::{lockfile_path_for_manifest, resolve, write_lockfile};
use lar_runtime::{
    build as build_runtime, gc as gc_runtimes, inspect as inspect_runtime, list as list_runtimes,
    run as run_app, ComposeMode,
};
use lar_store::{prefix, Paths, Store};

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli.system, cli.command) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(system: bool, command: Commands) -> Result<ExitCode, String> {
    match command {
        Commands::Package { command } => {
            run_package(command)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Store { command } => {
            run_store(system, command)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Resolve { manifest } => {
            run_resolve(system, &manifest)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Runtime { command } => {
            run_runtime(system, command)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Run {
            lockfile,
            compose,
            args,
        } => {
            let compose: ComposeMode = compose
                .parse()
                .map_err(|e: lar_runtime::Error| e.to_string())?;
            run_launch(system, &lockfile, compose, &args)
        }
        Commands::Config { json } => {
            run_config(system, json)?;
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("{}: not implemented yet", command_name(&other))),
    }
}

fn open_store(system: bool) -> Store {
    let paths = Paths::from_prefix(prefix(system), system);
    Store::open(paths)
}

fn run_runtime(system: bool, command: RuntimeCmd) -> Result<(), String> {
    let store = open_store(system);
    match command {
        RuntimeCmd::Build { lockfile, compose } => {
            let compose: ComposeMode = compose
                .parse()
                .map_err(|e: lar_runtime::Error| e.to_string())?;
            let built = build_runtime(&lockfile, &store, compose).map_err(|err| err.to_string())?;
            let action = if built.reused { "reused" } else { "built" };
            println!(
                "{action} {} {} ({}) -> {}",
                built.meta.root.id,
                built.meta.root.version,
                built.meta.compose,
                built.path.display()
            );
            Ok(())
        }
        RuntimeCmd::List => {
            let runtimes = list_runtimes(&store).map_err(|err| err.to_string())?;
            for rt in runtimes {
                println!(
                    "{} {} {} {} {}",
                    rt.runtime_id,
                    rt.meta.root.id,
                    rt.meta.root.version,
                    rt.meta.compose,
                    rt.path.display()
                );
            }
            Ok(())
        }
        RuntimeCmd::Gc { all } => {
            let report = gc_runtimes(&store, all).map_err(|err| err.to_string())?;
            for path in &report.orphans {
                println!("removed orphan {}", path.display());
            }
            for rt in &report.removed {
                println!(
                    "removed {} {} {} ({})",
                    rt.runtime_id, rt.meta.root.id, rt.meta.root.version, rt.meta.compose
                );
            }
            if all {
                println!(
                    "gc removed {} runtime(s), {} orphan(s)",
                    report.removed.len(),
                    report.orphans.len()
                );
            } else {
                println!(
                    "gc removed {} broken runtime(s), {} orphan(s), kept {}",
                    report.removed.len(),
                    report.orphans.len(),
                    report.kept
                );
            }
            Ok(())
        }
        RuntimeCmd::Inspect { runtime, json } => {
            let rt = inspect_runtime(&store, &runtime).map_err(|err| err.to_string())?;
            if json {
                let value = serde_json::json!({
                    "format": rt.meta.format,
                    "runtime_id": rt.meta.runtime_id,
                    "compose": rt.meta.compose.as_str(),
                    "path": rt.path,
                    "root": {
                        "id": rt.meta.root.id,
                        "version": rt.meta.root.version,
                    },
                    "packages": rt.meta.packages.iter().map(|p| serde_json::json!({
                        "id": p.id,
                        "version": p.version,
                        "content_hash": p.content_hash,
                        "dependencies": p.dependencies,
                    })).collect::<Vec<_>>(),
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
                );
            } else {
                println!(
                    "{} {} (format {})",
                    rt.meta.root.id, rt.meta.root.version, rt.meta.format
                );
                println!("runtime_id {}", rt.meta.runtime_id);
                println!("compose {}", rt.meta.compose);
                println!("path {}", rt.path.display());
                println!("{} packages", rt.meta.packages.len());
                for pkg in &rt.meta.packages {
                    println!("  {} {} {}", pkg.id, pkg.version, pkg.content_hash);
                }
            }
            Ok(())
        }
    }
}

fn run_launch(
    system: bool,
    lockfile: &Path,
    compose: ComposeMode,
    args: &[String],
) -> Result<ExitCode, String> {
    let store = open_store(system);
    let status = run_app(lockfile, &store, compose, args).map_err(|err| err.to_string())?;
    match status.code() {
        Some(code) => Ok(ExitCode::from(code as u8)),
        None => Ok(ExitCode::FAILURE),
    }
}

fn run_resolve(system: bool, manifest: &Path) -> Result<(), String> {
    let store = open_store(system);
    let manifest_path =
        lar_package::resolve_manifest_path(manifest).map_err(|err| err.to_string())?;
    let lock = resolve(&manifest_path, &store).map_err(|err| err.to_string())?;
    let out = lockfile_path_for_manifest(&manifest_path).map_err(|err| err.to_string())?;
    write_lockfile(&out, &lock).map_err(|err| err.to_string())?;
    println!(
        "resolved {} {} ({} packages) -> {}",
        lock.root.id,
        lock.root.version,
        lock.packages.len(),
        out.display()
    );
    Ok(())
}

fn run_store(system: bool, command: StoreCmd) -> Result<(), String> {
    let store = open_store(system);
    match command {
        StoreCmd::Add { package } => {
            let stored = store.add(&package).map_err(|err| err.to_string())?;
            println!(
                "{} {} -> {}",
                stored.id,
                stored.version,
                stored.path.display()
            );
            Ok(())
        }
        StoreCmd::List => {
            let packages = store.list().map_err(|err| err.to_string())?;
            for pkg in packages {
                println!("{} {} {}", pkg.id, pkg.version, pkg.content_hash);
            }
            Ok(())
        }
        StoreCmd::Remove { id, version, force } => {
            let removed = store
                .remove(&id, &version, force)
                .map_err(|err| err.to_string())?;
            for pkg in removed {
                println!("removed {} {}", pkg.id, pkg.version);
            }
            Ok(())
        }
    }
}

fn run_config(system: bool, json: bool) -> Result<(), String> {
    let paths = Paths::from_prefix(prefix(system), system);
    if json {
        let value = serde_json::json!({
            "system": paths.system,
            "prefix": paths.prefix,
            "store": paths.store,
            "packages": paths.packages,
            "runtimes": paths.runtimes,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
        );
    } else {
        println!("system {}", paths.system);
        println!("prefix {}", paths.prefix.display());
        println!("store {}", paths.store.display());
        println!("packages {}", paths.packages.display());
        println!("runtimes {}", paths.runtimes.display());
    }
    Ok(())
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
            StoreCmd::Remove { .. } => "lar store remove",
        },
        Commands::Resolve { .. } => "lar resolve",
        Commands::Runtime { command } => match command {
            RuntimeCmd::Build { .. } => "lar runtime build",
            RuntimeCmd::List => "lar runtime list",
            RuntimeCmd::Gc { .. } => "lar runtime gc",
            RuntimeCmd::Inspect { .. } => "lar runtime inspect",
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
