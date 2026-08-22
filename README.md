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

- Fuzzy-select snippets with a built-in picker — no external tool required
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
- Save your previous shell command as a snippet without retyping it (`pet new
  -l`/`--last`)
- Reads config/snippet files from older installs too, including ones that
  predate the current field-casing conventions
- Sync your snippet file to a GitHub Gist (`pet sync push`/`pet sync pull`)

Not yet implemented: syncing via GitLab or GitHub Enterprise Gist (GitHub.com
Gist only, for now).

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

`pet search`/`exec`/`clip`/`delete`/`edit` (when using multiple snippet
directories) use a fuzzy-finder for interactive selection. By default that's
a picker built into `pet` itself — nothing else to install. If you'd rather
use [`fzf`](https://github.com/junegunn/fzf), `peco`, or something else, point
the `selectcmd` config option (see below) at it instead.

### The built-in picker

When `selectcmd` is `builtin` (the default for new configs), selection opens
a full-screen fuzzy finder:

- Description, command, and tags are colored distinctly (green/yellow/cyan)
  in every row, and matched characters are bolded and underlined on top of
  that as you type
- Type to filter — matches are scored and sorted like `fzf`'s
- `↑`/`↓` move focus, wrapping at either end
- `Tab` toggles the focused snippet for multi-select and advances
- `Enter` confirms — whatever's toggled, or just the focused snippet if
  nothing's been toggled
- `Esc`/`Ctrl-C` cancels (nothing gets selected)

`-q`/`--query` still pre-fills the picker's own query box, and `general.format`
still controls what each line shows, same as with an external selector. `color`
doesn't apply, though — the built-in picker always renders in color, using its
own native styling rather than embedding ANSI codes in the text like an
external `selectcmd` needs.

### Shell completions

`pet completions <shell>` prints a completion script for `bash`, `zsh`,
`fish`, `elvish`, or `powershell` — install it the way your shell expects:

```bash
# bash
pet completions bash > /usr/local/etc/bash_completion.d/pet   # or wherever your bashrc sources completions from

# zsh (anywhere on $fpath, then start a new shell)
pet completions zsh > "${fpath[1]}/_pet"

# fish
pet completions fish > ~/.config/fish/completions/pet.fish
```

## Quick start

```bash
# Save a snippet
$ pet new
Command> tar -czf <archive=out.tar.gz> <dir=.>
Description> Compress a directory

# Find it again and run it
$ pet exec
# ...the picker opens, you pick the snippet, then fill in archive/dir if you like the defaults or not...

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
  completions Print a shell completion script
  sync push   Upload the snippet file to a gist
  sync pull   Download the gist and overwrite the snippet file

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
pet new -l                    # use the previous shell command (read from history) instead of prompting for it
```

`-l`/`--last` reads your shell's history file directly (`$HISTFILE`, or the
default location for `$SHELL`) rather than needing a wrapper shell function.
On shells that only flush history to disk periodically (e.g. plain bash
without `history -a` in `PROMPT_COMMAND`), the previous command may not be on
disk yet when `pet new -l` runs.

### `pet list`

Print all snippets.

```bash
pet list                      # full detail
pet list --oneline            # one line per snippet
pet list -t tag1,tag2         # only snippets tagged tag1 OR tag2
pet list -f docker             # only snippets whose description or command contains "docker"
```

### `pet delete`

Fuzzy-select one or more snippets and delete them. Hand-editing the TOML file
(`pet edit`) still works too, if you prefer.

```bash
pet delete                    # pick one or more snippets to remove
pet delete -t net              # only offer snippets tagged "net"
pet delete -q "docker"         # pre-fill the picker's query
```

### `pet search` / `pet exec` / `pet clip`

Fuzzy-select one or more snippets, then respectively print, run, or copy the
result.

```bash
pet search                    # print the selected command
pet search --raw              # skip the parameter dialog even if it has one
pet search -q "docker"        # pre-fill the picker's query (still its own live-editable filter)
pet search -t net              # only search snippets tagged "net"
pet search -f docker           # only offer snippets whose description or command contains "docker" — narrows the list itself, combines with -t
pet search -d $'\n'            # join multi-selected commands with this instead of "; "

pet exec                      # run the selected command
pet exec -s                   # ...without echoing "> command" first

pet clip                      # copy the selected command to the clipboard
pet clip --command             # ...and print what was copied

pet search --color             # with an external selectcmd, force description/tags coloring even if config.toml disables it (needs --ansi in a fzf-style selectcmd; no effect on the built-in picker, which always colorizes)
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

### `pet sync push` / `pet sync pull`

Sync your snippet file with a [GitHub Gist](https://gist.github.com). `push`
uploads it as-is (creating the gist on the first push, updating it after);
`pull` downloads it and replaces your local snippet file.

```bash
pet sync push                 # create the gist on first run, update it after
pet sync pull                 # prompts before overwriting local snippets
pet sync pull -y              # skip the confirmation prompt
```

Set `access_token` under `[Gist]` in `config.toml` first (`pet configure`) —
a [personal access token](https://github.com/settings/tokens) with the
`gist` scope. A `GITHUB_TOKEN` environment variable works too, if you'd
rather not put the token in a file — `access_token` in config.toml takes
priority when both are set. `gist_id` fills in automatically after your
first `pet sync push`; `file_name` (default `pet-snippet.toml`) and `public`
control the gist's file name and visibility.

GitLab and GitHub Enterprise sync aren't implemented yet, despite
`config.toml` having sections for them (kept for compatibility with the
original Go pet's config format).

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
  selectcmd = "builtin"            # "builtin" for the native picker (default for new configs), or an external command, e.g. "fzf --ansi --layout=reverse --border --height=90% --pointer=* --cycle --prompt=Snippets:"
  sortby = ""                      # recency (default) | -recency | description | -description | command | -command | output | -output | usage | -usage
  cmd = ["sh", "-c"]                # shell used to run selectcmd/editor/exec
  format = "[$description]: $command $tags"   # how snippets are displayed to the selector
  color = true                     # colorize description/tags in the selector list, same as --color (default: true for new configs; set false to disable)

[Gist]
  file_name = "pet-snippet.toml"   # file name inside the gist
  access_token = ""                # GitHub personal access token (gist scope) — or set GITHUB_TOKEN instead
  gist_id = ""                     # filled in automatically after your first `pet sync push`
  public = false                   # create the gist as public
```

`usage`/`-usage` sort by how often a snippet has been picked via `search`/`exec`/`clip`
(most-used first for `usage`, least-used first for `-usage`, ties broken by most/least
recently used). Invocation stats are tracked automatically in `usage.toml`, next to
`snippetfile` — a local-only file, not part of the portable snippet list.

Snippets are plain TOML (`pet edit` to open it directly):

```toml
[[snippets]]
  description = "Compress a directory"
  command = "tar -czf <archive=out.tar.gz> <dir=.>"
  tag = ["files"]
```

## Shell integration

Bind a key to search your snippets and drop the result on your command line,
so it lands in shell history too. This captures `pet search`'s stdout via
`$(...)`; the built-in picker draws itself to the terminal directly rather
than through stdout, so it still shows up correctly here.

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
