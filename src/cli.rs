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
    /// Create a new snippet
    New {
        /// The command to save; if omitted, you'll be prompted for it
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,

        /// Display tag prompt (delimiter: space)
        #[arg(short = 't', long = "tag")]
        tag: bool,

        /// Can enter multiline snippet (blank line twice to quit)
        #[arg(short = 'm', long = "multiline")]
        multiline: bool,

        /// Use editor to create snippet
        #[arg(short = 'e', long = "editor")]
        editor: bool,
    },
    /// Show all snippets
    List {
        /// Display snippets in one line
        #[arg(long)]
        oneline: bool,

        /// List by specified tags as comma separated values
        #[arg(short = 't', long = "tags")]
        tags: Option<String>,
    },
    /// Edit config file
    Configure,
    /// Print the version number
    Version,
}
