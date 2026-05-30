use anyhow::Result;
use clap::Parser;
use gpm::cli::{Cli, Commands};
use gpm::commands;
use gpm::extractor::ArchiveExtractor;
use gpm::github::GithubClient;
use gpm::installer::GpmInstaller;
use gpm::manifest::JsonStateManager;
use gpm::network::ReqwestClient;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let paths = gpm::paths::GpmPaths::new();
    let http = ReqwestClient::new()?;
    let github = GithubClient::new(&http);
    let extractor = ArchiveExtractor::new();
    let installer = GpmInstaller::new(&http, &extractor, paths.clone());
    let state = JsonStateManager::new(paths);

    match cli.command {
        Commands::Install(args) => commands::install(&installer, &github, &state, &args).await?,
        Commands::Uninstall(args) => commands::uninstall(&installer, &state, &args)?,
        Commands::Link(args) => commands::link(&installer, &state, &args)?,
        Commands::Unlink(args) => commands::unlink(&installer, &state, &args)?,
        Commands::List => commands::list(&state)?,
        Commands::Outdated(args) => commands::outdated(&github, &state, &args).await?,
        Commands::Upgrade(args) => commands::upgrade(&installer, &github, &state, &args).await?,
        Commands::SelfUpdate => commands::self_update().await?,
        Commands::Prune(args) => commands::prune(&installer, &state, &args)?,
    }

    Ok(())
}
