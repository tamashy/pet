use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser};

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

    // Doesn't touch config/snippets at all, so generate it before the config
    // directory gets created below — completions should work in a fresh
    // environment (e.g. a package install script) with no config yet.
    if let Commands::Completions { shell } = cli.command {
        let mut command = Cli::command();
        let name = command.get_name().to_string();
        clap_complete::generate(shell, &mut command, name, &mut std::io::stdout());
        return Ok(());
    }

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
        Commands::List {
            oneline,
            tags,
            filter,
        } => {
            cmd::list::run(
                &cfg,
                cmd::list::ListOptions {
                    oneline,
                    tags,
                    filter,
                    debug: cli.debug,
                },
            )?;
        }
        Commands::Configure => {
            cmd::configure::run(&cfg, &config_path)?;
        }
        Commands::Delete { query, tag, color } => {
            cmd::delete::run(&cfg, cmd::delete::DeleteOptions { query, tag, color })?;
        }
        Commands::Edit { query, tag } => {
            cmd::edit::run(&cfg, cmd::edit::EditOptions { query, tag })?;
        }
        Commands::Search {
            raw,
            query,
            tag,
            filter,
            delimiter,
            color,
        } => {
            cmd::search::run(
                &cfg,
                cmd::search::SearchOptions {
                    query,
                    tag,
                    filter,
                    delimiter,
                    raw,
                    color,
                },
            )?;
        }
        Commands::Exec {
            query,
            tag,
            silent,
            color,
        } => {
            cmd::exec::run(
                &cfg,
                cmd::exec::ExecOptions {
                    query,
                    tag,
                    silent,
                    color,
                },
            )?;
        }
        Commands::Clip {
            raw,
            query,
            tag,
            delimiter,
            show_command,
            color,
        } => {
            cmd::clip::run(
                &cfg,
                cmd::clip::ClipOptions {
                    query,
                    tag,
                    delimiter,
                    raw,
                    show_command,
                    color,
                },
            )?;
        }
        Commands::Version => {
            cmd::version::run();
        }
        Commands::Completions { .. } => unreachable!("handled above, before config is loaded"),
    }

    Ok(())
}
