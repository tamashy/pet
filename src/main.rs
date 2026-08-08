use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use pet::cli::{Cli, Commands};
use pet::cmd;
use pet::config::{self, Config};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let config_path = match &cli.config {
        Some(path) => PathBuf::from(path),
        None => config::default_config_dir()?.join("config.toml"),
    };
    let cfg = Config::load(&config_path)?;

    match cli.command {
        Commands::New {
            command,
            tag,
            multiline,
            editor,
        } => {
            cmd::new::run(
                &cfg,
                cmd::new::NewOptions {
                    command_args: command,
                    prompt_tag: tag,
                    multiline,
                    use_editor: editor,
                },
            )?;
        }
        Commands::List { oneline, tags } => {
            cmd::list::run(&cfg, oneline, tags.as_deref(), cli.debug)?;
        }
        Commands::Configure => {
            cmd::configure::run(&cfg, &config_path)?;
        }
        Commands::Edit { query, tag } => {
            cmd::edit::run(&cfg, cmd::edit::EditOptions { query, tag })?;
        }
        Commands::Search {
            raw,
            query,
            tag,
            delimiter,
        } => {
            cmd::search::run(
                &cfg,
                cmd::search::SearchOptions {
                    query,
                    tag,
                    delimiter,
                    raw,
                },
            )?;
        }
        Commands::Exec { query, tag, silent } => {
            cmd::exec::run(&cfg, cmd::exec::ExecOptions { query, tag, silent })?;
        }
        Commands::Clip {
            raw,
            query,
            tag,
            delimiter,
            show_command,
        } => {
            cmd::clip::run(
                &cfg,
                cmd::clip::ClipOptions {
                    query,
                    tag,
                    delimiter,
                    raw,
                    show_command,
                },
            )?;
        }
        Commands::Version => {
            cmd::version::run();
        }
    }

    Ok(())
}
