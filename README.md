# pet

A command-line snippet manager, written in Rust. Save the commands you always
forget, tag them, find them again in seconds, and run them without retyping —
all from the terminal. Originally inspired by
[`knqyf263/pet`](https://github.com/knqyf263/pet), it's grown its own feature
set since.

Config and snippets are plain TOML files under `~/.config/pet` (or wherever
`PET_CONFIG_DIR`/`--config` points), created automatically on first run.

## Why

You know the feeling: a command you use once every few months, just obscure
enough that you always have to look it up again. `pet` lets you save it once,
with a description and tags, and pull it back up instantly with a fuzzy
search — without digging through shell history or a notes file.

## Features

- Save command snippets with a description and tags (`pet new`)
- Fuzzy-search and run them without retyping (`pet exec`)
- Fuzzy-search and print them, e.g. to pipe into your shell history
  (`pet search`)
- Copy a snippet straight to your clipboard (`pet clip`)
- Parameterized snippets — fill in the blanks interactively before running
  (`<name>`, `<name=default>`, `<name=|_option_a_||_option_b_|>`)
- Snippets are a plain TOML file, so `pet edit` (or any text editor) works
  too
- Fuzzy-select and delete a snippet without hand-editing the TOML (`pet
  delete`)
- Reads config/snippet files from older installs too, including ones that
  predate the current field-casing conventions

Not yet implemented: syncing snippets via Gist/GitLab/GHE.

## Installation

### Download a release binary

Prebuilt binaries for macOS (Apple Silicon and Intel) and Linux (x86_64,
statically linked against musl — runs on any distro/glibc version) are
published on the [Releases page](https://github.com/tamashy/pet/releases).
Download the archive for your platform, extract it, and put `pet` somewhere
on your `$PATH`:

```bash
tar -xzf pet-<version>-<target>.tar.gz
sudo mv pet-<version>-<target>/pet /usr/local/bin/pet
```

### Build from source

Requires a recent [Rust toolchain](https://rustup.rs/).

```bash
git clone git@github.com:tamashy/pet.git
cd pet
cargo install --path .
```

### A selector

`pet search`/`exec`/`clip`/`edit` (when using multiple snippet directories)
shell out to an external fuzzy-finder for interactive selection — by default
[`fzf`](https://github.com/junegunn/fzf). Install it, or point the `selectcmd`
config option (see below) at something else, e.g. `peco`.

## Quick start

```bash
# Save a snippet
$ pet new
Command> tar -czf <archive=out.tar.gz> <dir=.>
Description> Compress a directory

# Find it again and run it
$ pet exec
# ...fzf opens, you pick the snippet, then fill in archive/dir if you like the defaults or not...

# Just print it instead of running it (e.g. to inspect it, or pipe elsewhere)
$ pet search --raw
```

## Usage

```
pet [command]

Commands:
  new         Create a new snippet
  list        Show all snippets
  configure   Edit config file
  delete      Delete a snippet
  edit        Edit snippet file
  search      Search snippets interactively
  exec        Run the selected commands
  clip        Copy the selected commands to clipboard
  version     Print the version number

Flags:
      --config <path>   config file (default $HOME/.config/pet/config.toml)
      --debug           debug mode
```

### `pet new [COMMAND...]`

Create a new snippet. Prompts for Command (if not given as arguments) and
Description; add `-t` to also be prompted for space-separated tags.

```bash
pet new                       # prompts for everything
pet new echo hello world      # command given, still prompts for description/tags
pet new -t                    # also prompts for tags
pet new -m                    # multiline command (blank line twice to finish)
pet new -e                    # skip prompts, open $EDITOR on a blank entry instead
```

### `pet list`

Print all snippets.

```bash
pet list                      # full detail
pet list --oneline            # one line per snippet
pet list -t tag1,tag2         # only snippets tagged tag1 OR tag2
```

### `pet delete`

Fuzzy-select one or more snippets and delete them. Hand-editing the TOML file
(`pet edit`) still works too, if you prefer.

```bash
pet delete                    # pick one or more snippets to remove
pet delete -t net              # only offer snippets tagged "net"
pet delete -q "docker"         # pre-fill the fzf query
```

### `pet search` / `pet exec` / `pet clip`

Fuzzy-select one or more snippets, then respectively print, run, or copy the
result.

```bash
pet search                    # print the selected command
pet search --raw              # skip the parameter dialog even if it has one
pet search -q "docker"        # pre-fill the fzf query
pet search -t net              # only search snippets tagged "net"
pet search -d $'\n'            # join multi-selected commands with this instead of "; "

pet exec                      # run the selected command
pet exec -s                   # ...without echoing "> command" first

pet clip                      # copy the selected command to the clipboard
pet clip --command             # ...and print what was copied

pet search --color             # force description/tags coloring in the fzf list even if config.toml disables it (needs --ansi in selectcmd, on by default)
```

If exactly one snippet is selected (not `--raw`) and its command contains a
`<param>`-style placeholder, an interactive form opens to fill it in before
the command is used - see [Parameters](#parameters).

### `pet edit`

Open the snippet file in `$EDITOR`. If you've configured multiple
`snippetdirs`, you'll be prompted to pick which file first.

```bash
pet edit
pet edit -t work               # narrow the file picker to snippets tagged "work"
```

### `pet configure`

Open `config.toml` in `$EDITOR`.

## Parameters

Snippets can contain placeholders that get filled in interactively:

```toml
command = "ssh <user=admin>@<host>"
```

- `<name>` - a required value, no default
- `<name=default>` - pre-filled with `default`, editable
- `<name=|_option_a_||_option_b_|>` - cycle through fixed options with ↑/↓

In the dialog: type to edit the focused field, `Tab`/`Shift-Tab` to switch
fields, `Enter` to confirm the current values, `Esc`/`Ctrl-C` to cancel
(nothing gets printed, run, or copied).

## Configuration

Config lives at `$HOME/.config/pet/config.toml` by default — override the
directory with the `PET_CONFIG_DIR` environment variable, or the file
directly with `--config`. It's created for you on first run.

```toml
[General]
  snippetfile = "/Users/you/.config/pet/snippet.toml"
  snippetdirs = []                 # extra directories of *.toml snippet files
  editor = "vim"
  column = 40                      # truncation width for `list --oneline`
  selectcmd = "fzf --ansi --layout=reverse --border --height=90% --pointer=* --cycle --prompt=Snippets:"
  sortby = ""                      # recency (default) | -recency | description | -description | command | -command | output | -output
  cmd = ["sh", "-c"]                # shell used to run selectcmd/editor/exec
  format = "[$description]: $command $tags"   # how snippets are displayed to the selector
  color = true                     # colorize description/tags in the selector list, same as --color (default: true for new configs; set false to disable)
```

Snippets are plain TOML (`pet edit` to open it directly):

```toml
[[snippets]]
  description = "Compress a directory"
  command = "tar -czf <archive=out.tar.gz> <dir=.>"
  tag = ["files"]
```

## Shell integration

Bind a key to search your snippets and drop the result on your command line,
so it lands in shell history too:

**bash** (`.bashrc`)
```bash
function pet-select() {
  BUFFER=$(pet search --query "$READLINE_LINE")
  READLINE_LINE=$BUFFER
  READLINE_POINT=${#BUFFER}
}
bind -x '"\C-x\C-r": pet-select'
```

**zsh** (`.zshrc`)
```zsh
function pet-select() {
  BUFFER=$(pet search --query "$LBUFFER")
  CURSOR=$#BUFFER
  zle redisplay
}
zle -N pet-select
stty -ixon
bindkey '^s' pet-select
```

## Development

```bash
cargo build              # debug build -> target/debug/pet
cargo test                # unit + integration tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Use `PET_CONFIG_DIR=/some/scratch/dir` to sandbox manual testing away from
your real config.

## Credits

Originally inspired by [Teppei Fukuda (knqyf263)](https://github.com/knqyf263)'s
[`pet`](https://github.com/knqyf263/pet).
