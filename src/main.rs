use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use pet::cli::{Cli, Commands};
use pet::cmd;
use pet::config::{self, Config};

fn main() {
    reset_sigpipe();

    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

/// Rust ignores SIGPIPE by default and turns a write to a closed pipe into a panic
/// instead. pet's output is routinely piped into other commands (`pet search | head`,
/// shell widgets capturing `$(pet search ...)`); restore the standard Unix behavior
/// (the process just exits) so a downstream reader closing early doesn't panic us.
#[cfg(unix)]
fn reset_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

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
        Commands::Version => {
            cmd::version::run();
        }
    }

    Ok(())
}
