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
        Commands::Version => {
            cmd::version::run();
        }
    }

    Ok(())
}
