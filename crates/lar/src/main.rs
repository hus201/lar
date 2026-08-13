mod cli;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Commands, PackageCmd, RepoCmd, RuntimeCmd, StoreCmd};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let _ = cli.system;
    eprintln!("{}: not implemented yet", command_name(&cli.command));
    ExitCode::FAILURE
}

fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Package { command } => match command {
            PackageCmd::Init { .. } => "lar package init",
            PackageCmd::Validate { .. } => "lar package validate",
            PackageCmd::Pack { .. } => "lar package pack",
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
