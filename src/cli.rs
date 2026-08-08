use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "pet", about = "Simple command-line snippet manager.", version)]
pub struct Cli {
    /// Config file (default is $HOME/.config/pet/config.toml)
    #[arg(long, global = true)]
    pub config: Option<String>,

    /// Debug mode
    #[arg(long, global = true)]
    pub debug: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Show all snippets
    List {
        /// Display snippets in one line
        #[arg(long)]
        oneline: bool,

        /// List by specified tags as comma separated values
        #[arg(short = 't', long = "tags")]
        tags: Option<String>,
    },
    /// Print the version number
    Version,
}
