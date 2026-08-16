mod cli;

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Commands, PackageCmd, RepoCmd, RuntimeCmd, StoreCmd, TrustCmd};
use lar_manager::{
    install as install_app, launch as launch_app, list as list_installs, rollback as rollback_app,
    uninstall as uninstall_app, update as update_app, InstallSource, UpdateOutcome,
};
use lar_package::{init_package, inspect, pack, validate_package, InitOptions};
use lar_repo::{
    add_source, audit, audit_should_fail, build_index, default_source_name, init_repo, keygen,
    load_sources, load_trust, publish_package, remove_source, sign_advisories_in_dir, trust_add,
    trust_remove, unpublish_package, validate_repo, write_index, AuditScope, SourcePolicy,
};
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
            run_from_lockfile(system, &lockfile, compose, &args)
        }
        Commands::Launch { app, binary, args } => {
            run_launch_installed(system, &app, binary.as_deref(), &args)
        }
        Commands::Install {
            app,
            compose,
            force,
        } => {
            let compose: ComposeMode = compose
                .parse()
                .map_err(|e: lar_runtime::Error| e.to_string())?;
            run_install(system, &app, compose, force)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::List => {
            run_list_installs(system)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Update { app } => {
            run_update(system, &app)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Rollback { app } => {
            run_rollback(system, &app)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Uninstall { app } => {
            run_uninstall(system, &app)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Repo { command } => {
            run_repo(system, command)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Audit { store, installed } => run_audit(system, store, installed),
        Commands::Config { json } => {
            run_config(system, json)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn open_store(system: bool) -> Store {
    let paths = Paths::from_prefix(prefix(system), system);
    Store::open(paths)
}

fn run_install(system: bool, app: &str, compose: ComposeMode, force: bool) -> Result<(), String> {
    let store = open_store(system);
    let source = InstallSource::parse(app).map_err(|err| err.to_string())?;
    let outcome = install_app(&store, &source, compose, force).map_err(|err| err.to_string())?;
    let action = if outcome.replaced {
        "reinstalled"
    } else {
        "installed"
    };
    println!(
        "{action} {} {} ({}) runtime {}",
        outcome.record.id,
        outcome.record.version,
        outcome.record.compose,
        outcome.record.runtime_id
    );
    print_path_shadows(&outcome.path_shadows);
    Ok(())
}

fn run_list_installs(system: bool) -> Result<(), String> {
    let store = open_store(system);
    let installs = list_installs(&store).map_err(|err| err.to_string())?;
    for rec in installs {
        println!(
            "{} {} {} {}",
            rec.id, rec.version, rec.compose, rec.runtime_id
        );
    }
    Ok(())
}

fn run_uninstall(system: bool, app: &str) -> Result<(), String> {
    let store = open_store(system);
    let record = uninstall_app(&store, app).map_err(|err| err.to_string())?;
    println!(
        "uninstalled {} {} (runtime {})",
        record.id, record.version, record.runtime_id
    );
    Ok(())
}

fn run_update(system: bool, app: &str) -> Result<(), String> {
    let store = open_store(system);
    match update_app(&store, app).map_err(|err| err.to_string())? {
        UpdateOutcome::UpToDate(rec) => {
            println!("up to date {} {}", rec.id, rec.version);
        }
        UpdateOutcome::Updated {
            from,
            to,
            path_shadows,
        } => {
            println!(
                "updated {} {} -> {} (runtime {})",
                to.id, from.version, to.version, to.runtime_id
            );
            print_path_shadows(&path_shadows);
        }
    }
    Ok(())
}

fn run_rollback(system: bool, app: &str) -> Result<(), String> {
    let store = open_store(system);
    let outcome = rollback_app(&store, app).map_err(|err| err.to_string())?;
    println!(
        "rolled back {} {} (runtime {})",
        outcome.record.id, outcome.record.version, outcome.record.runtime_id
    );
    print_path_shadows(&outcome.path_shadows);
    Ok(())
}

fn print_path_shadows(shadows: &[lar_manager::PathShadow]) {
    for shadow in shadows {
        eprintln!("warning: {shadow}");
    }
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

fn run_from_lockfile(
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

fn run_launch_installed(
    system: bool,
    app: &str,
    binary: Option<&str>,
    args: &[String],
) -> Result<ExitCode, String> {
    let store = open_store(system);
    let status = launch_app(&store, app, binary, args).map_err(|err| err.to_string())?;
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
            "installs": paths.installs,
            "config": paths.config,
            "sources": paths.sources_toml(),
            "trust": paths.trust_toml(),
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
        println!("installs {}", paths.installs.display());
        println!("config {}", paths.config.display());
        println!("sources {}", paths.sources_toml().display());
        println!("trust {}", paths.trust_toml().display());
    }
    Ok(())
}

fn run_repo(system: bool, command: RepoCmd) -> Result<(), String> {
    let store = open_store(system);
    match command {
        RepoCmd::Add {
            uri,
            policy,
            main,
            name,
        } => {
            let policy = policy.unwrap_or_else(|| if main { "deps".into() } else { "both".into() });
            let policy: SourcePolicy =
                policy.parse().map_err(|e: lar_repo::Error| e.to_string())?;
            let name = name.unwrap_or_else(|| default_source_name(&uri, main));
            let entry = add_source(&store, name, uri, policy, main).map_err(|e| e.to_string())?;
            let main_tag = if entry.main { " main" } else { "" };
            println!(
                "added {} {} ({}){main_tag}",
                entry.name, entry.uri, entry.policy
            );
            Ok(())
        }
        RepoCmd::List => {
            let file = load_sources(&store).map_err(|e| e.to_string())?;
            for src in &file.sources {
                let main_tag = if src.main { " main" } else { "" };
                println!("{} {} ({}){main_tag}", src.name, src.uri, src.policy);
            }
            Ok(())
        }
        RepoCmd::Remove { source } => {
            let entry = remove_source(&store, &source).map_err(|e| e.to_string())?;
            println!("removed {} {}", entry.name, entry.uri);
            Ok(())
        }
        RepoCmd::Init { dir, sign_key } => {
            let secret = read_key_material(&sign_key)?;
            let path = init_repo(&dir, &secret).map_err(|e| e.to_string())?;
            println!("initialized {} ({})", dir.display(), path.display());
            Ok(())
        }
        RepoCmd::Publish {
            dir,
            package,
            sign_key,
        } => {
            let secret = read_key_material(&sign_key)?;
            let (info, index) =
                publish_package(&dir, &package, &secret).map_err(|e| e.to_string())?;
            println!(
                "published {} {} -> {} ({} packages in index)",
                info.id,
                info.version,
                info.file,
                index.packages.len()
            );
            Ok(())
        }
        RepoCmd::Unpublish {
            dir,
            package_id,
            version,
            sign_key,
        } => {
            let secret = read_key_material(&sign_key)?;
            let index = unpublish_package(&dir, &package_id, &version, &secret)
                .map_err(|e| e.to_string())?;
            println!(
                "unpublished {} {} ({} packages in index)",
                package_id,
                version,
                index.packages.len()
            );
            Ok(())
        }
        RepoCmd::Validate { dir, pubkey } => {
            let public = match pubkey {
                Some(material) => Some(read_key_material(&material)?),
                None => None,
            };
            let report = validate_repo(&dir, public.as_deref()).map_err(|e| e.to_string())?;
            if public.is_some() {
                println!(
                    "ok {} ({} packages, {} advisories; signatures verified)",
                    dir.display(),
                    report.packages,
                    report.advisories
                );
            } else {
                println!(
                    "ok {} ({} packages, {} advisories; layout and hashes only — pass --pubkey to verify signatures)",
                    dir.display(),
                    report.packages,
                    report.advisories
                );
            }
            Ok(())
        }
        RepoCmd::Index { dir, sign_key } => {
            let secret = read_key_material(&sign_key)?;
            let index = build_index(&dir, &secret).map_err(|e| e.to_string())?;
            let path = write_index(&dir, &index).map_err(|e| e.to_string())?;
            println!(
                "wrote {} ({} packages)",
                path.display(),
                index.packages.len()
            );
            if let Some(adv_path) =
                sign_advisories_in_dir(&dir, &secret).map_err(|e| e.to_string())?
            {
                println!("wrote {} (signed)", adv_path.display());
            }
            Ok(())
        }
        RepoCmd::Trust { command } => run_trust(system, command),
    }
}

fn run_trust(system: bool, command: TrustCmd) -> Result<(), String> {
    let store = open_store(system);
    match command {
        TrustCmd::Add { pubkey, comment } => {
            let public = read_key_material(&pubkey)?;
            let entry = trust_add(&store, &public, comment.unwrap_or_default())
                .map_err(|e| e.to_string())?;
            println!("trusted {} {}", entry.id, entry.public_key);
            Ok(())
        }
        TrustCmd::List => {
            let file = load_trust(&store).map_err(|e| e.to_string())?;
            for key in &file.keys {
                if key.comment.is_empty() {
                    println!("{} {}", key.id, key.public_key);
                } else {
                    println!("{} {} ({})", key.id, key.public_key, key.comment);
                }
            }
            Ok(())
        }
        TrustCmd::Remove { key_id } => {
            let entry = trust_remove(&store, &key_id).map_err(|e| e.to_string())?;
            println!("removed {}", entry.id);
            Ok(())
        }
    }
}

fn run_audit(system: bool, store_scope: bool, _installed: bool) -> Result<ExitCode, String> {
    let store = open_store(system);
    let scope = if store_scope {
        AuditScope::Store
    } else {
        AuditScope::Installed
    };
    let mut out = Vec::new();
    let findings = audit(&store, scope, &mut out).map_err(|e| e.to_string())?;
    print!("{}", String::from_utf8_lossy(&out));
    if findings.is_empty() {
        println!("ok: no advisories matched");
    }
    if audit_should_fail(&findings) {
        Ok(ExitCode::FAILURE)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

/// Read a key from a file path, or return the input if it looks like inline `base64:…`.
fn read_key_material(input: &str) -> Result<String, String> {
    let path = Path::new(input);
    if path.is_file() {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(text.trim().to_string())
    } else {
        Ok(input.to_string())
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
        PackageCmd::Keygen { out } => {
            let (public, secret, id) = keygen().map_err(|e| e.to_string())?;
            fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            let pub_path = out.join("ed25519.pub");
            let sec_path = out.join("ed25519.sec");
            fs::write(&pub_path, format!("{public}\n"))
                .map_err(|e| format!("{}: {e}", pub_path.display()))?;
            fs::write(&sec_path, format!("{secret}\n"))
                .map_err(|e| format!("{}: {e}", sec_path.display()))?;
            println!(
                "wrote {} and {} ({id})",
                pub_path.display(),
                sec_path.display()
            );
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
