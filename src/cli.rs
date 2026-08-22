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

        /// Use the previous shell command as the command, read from your shell's
        /// history file, instead of prompting for it
        #[arg(short = 'l', long = "last", conflicts_with_all = ["command", "multiline", "editor"])]
        last: bool,
    },
    /// Show all snippets
    List {
        /// Display snippets in one line
        #[arg(long)]
        oneline: bool,

        /// List by specified tags as comma separated values
        #[arg(short = 't', long = "tags")]
        tags: Option<String>,

        /// Only show snippets whose description or command contains this text
        #[arg(short = 'f', long = "filter")]
        filter: Option<String>,
    },
    /// Edit config file
    Configure,
    /// Delete a snippet
    Delete {
        /// Initial value for query
        #[arg(short = 'q', long = "query")]
        query: Option<String>,

        /// Filter tag
        #[arg(short = 't', long = "tag")]
        tag: Option<String>,

        /// Enable colorized output (only fzf)
        #[arg(long)]
        color: bool,
    },
    /// Edit snippet file (default: opened by vim)
    Edit {
        /// Initial value for query
        #[arg(short = 'q', long = "query")]
        query: Option<String>,

        /// Filter tag
        #[arg(short = 't', long = "tag")]
        tag: Option<String>,
    },
    /// Search snippets interactively (default filtering tool: fzf)
    Search {
        /// Output raw command without entering parameter dialog
        #[arg(long)]
        raw: bool,

        /// Initial value for query
        #[arg(short = 'q', long = "query")]
        query: Option<String>,

        /// Filter tag
        #[arg(short = 't', long = "tag")]
        tag: Option<String>,

        /// Only offer snippets whose description or command contains this
        /// text, narrowing the list before the selector opens
        #[arg(short = 'f', long = "filter")]
        filter: Option<String>,

        /// Use delim as the command delimiter character
        #[arg(short = 'd', long = "delimiter", default_value = "; ")]
        delimiter: String,

        /// Enable colorized output (only fzf)
        #[arg(long)]
        color: bool,
    },
    /// Run the selected commands directly
    Exec {
        /// Initial value for query
        #[arg(short = 'q', long = "query")]
        query: Option<String>,

        /// Filter tag
        #[arg(short = 't', long = "tag")]
        tag: Option<String>,

        /// Suppress the command output
        #[arg(short = 's', long = "silent")]
        silent: bool,

        /// Enable colorized output (only fzf)
        #[arg(long)]
        color: bool,
    },
    /// Copy the selected commands to clipboard
    Clip {
        /// Output raw command without entering parameter dialog
        #[arg(long)]
        raw: bool,

        /// Initial value for query
        #[arg(short = 'q', long = "query")]
        query: Option<String>,

        /// Filter tag
        #[arg(short = 't', long = "tag")]
        tag: Option<String>,

        /// Use delim as the command delimiter character
        #[arg(short = 'd', long = "delimiter", default_value = "; ")]
        delimiter: String,

        /// Print the command before copying it to the clipboard
        #[arg(long = "command")]
        show_command: bool,

        /// Enable colorized output (only fzf)
        #[arg(long)]
        color: bool,
    },
    /// Print the version number
    Version,
    /// Print a shell completion script to stdout
    Completions {
        /// Which shell to generate completions for
        shell: clap_complete::Shell,
    },
    /// Sync the snippet file with a GitHub Gist
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum SyncAction {
    /// Upload the local snippet file to the configured gist (creates it on the first push)
    Push,
    /// Download the configured gist and overwrite the local snippet file
    Pull {
        /// Skip the confirmation prompt and overwrite immediately
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },
}
