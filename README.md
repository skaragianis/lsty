# lsty

[![CI](https://github.com/skaragianis/lsty/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/skaragianis/lsty/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/lsty)](https://crates.io/crates/lsty)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](https://github.com/skaragianis/lsty/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.99%2B-orange)](https://github.com/skaragianis/lsty/blob/main/rust-toolchain.toml)

Terminal lists for Humans and Agents.

![lsty navigating, adding, filtering, and completing tasks while an agent works](https://raw.githubusercontent.com/skaragianis/lsty/main/assets/demo.gif)

lsty is a vim-inspired [todo.txt](http://todotxt.org/) TUI written in Rust,
and a [todo.sh](https://github.com/todotxt/todo.txt-cli)-inspired CLI with
extensions for safe concurrent use by humans, scripts, and LLM agents.

## Some Possible Uses

- Personal todo list
- Agentic loop memory that a human can keep an eye on
- Software factory single source of truth
- Backlog of tasks for agents to loop through

## Features

- Watch agents work lists in "real-time"
- Vim-inspired TUI with configurable key bindings and themes. Normal, insert,
  visual and command-line modes
- Plain-text todo.txt files, with priorities, projects, contexts, dependencies,
  dates, and file links
- Filters on text, projects, contexts, tags, priorities and dates, combined
  with and (a space), or (`|`) and not (`-`)
- Auto-completion of project and context names. Cycle priorities and completion
- A CLI for listing, adding, editing and completing tasks
- JSON output for scripts and agents
- Task ownership, claiming, and dependencies for coordinating work
- Concurrent access by lsty processes, with locking, atomic writes and file
  watching
- No databases or daemons, just text files

## Installation

lsty supports Linux and macOS. Windows users might consider WSL. The TUI
requires an interactive terminal. Most CLI commands can be used without one.

### Homebrew

On macOS or Linux:

```sh
brew install skaragianis/lsty/lsty
```

### From crates.io

With Rust 1.99 or newer (`rustup update`):

```sh
cargo install lsty --locked
```

### Prebuilt binaries

[Releases](https://github.com/skaragianis/lsty/releases) has archives for
Linux (x86_64 and aarch64, statically linked) and macOS (Intel and Apple
Silicon), with `SHA256SUMS`.

### Build from source

Install [Rust](https://rustup.rs/), [just](https://just.systems) and Git, then run:

```sh
git clone https://github.com/skaragianis/lsty.git
cd lsty
just install
lsty --version
```

The repository pins Rust 1.99.0 in `rust-toolchain.toml`. With rustup
installed Cargo uses that toolchain automatically. `just install` puts `lsty`
in `~/.cargo/bin`, which rustup adds to your `PATH`.

## Quick start

The commands assume a POSIX shell.

```sh
touch todo.txt
lsty # launches the TUI
```

Default keys in normal mode:

| Key                      | Action                       |
| ------------------------ | ---------------------------- |
| `j` / `k`, or arrow keys | Move down / up               |
| `gg` / `G`               | Go to the first / last item  |
| `O`                      | Add an item above the cursor |
| `x`                      | Toggle completion            |
| `?`                      | Show keys and commands       |
| `:archive`               | Move done items to done.txt  |
| `q`                      | Quit                         |

Press `?` for every key binding and `:` command; `:help TEXT` opens it
filtered.

Task changes are written as you make them. After quitting, try the CLI against
the same list:

```sh
lsty add 'Try the CLI +personal' --human
lsty ls
lsty do 1 --human
lsty ls --json
```

`1` is a task's line number, not a permanent identifier. `--human` explicitly
marks a mutation as yours, including when run without an interactive terminal.
Most task management CLI commands are available in the TUI `:` command-line
mode.

## Files and configuration

### Choosing a task file

Use `lsty PATH` for the TUI or `--file PATH` for CLI commands. Otherwise lsty
checks these sources in order:

1. `TODO_FILE` environment variable.
2. An existing `todo.txt` in the current directory or a parent, stopping at
   the nearest Git root, your home directory, or the filesystem root.
3. `TODO_DIR/todo.txt`.
4. `default_file` from configuration.
5. A fallback `todo.txt` in the Git root, or the current directory outside Git.

CLI commands do not silently create a missing list; use `--create`.

Completed tasks stay in the task file by default. Archiving moves them to
`done.txt` beside `todo.txt`, or to `<name>.done.txt` for a list named
`<name>.txt`. `DONE_FILE` names another done file, but only for a list found
through `TODO_FILE` or `TODO_DIR`.

### Filters

`--filter`, the terms of `lsty ls` and saved `[filters]` (listed in the TUI's
Filters panel) all use one language. Terms are separated by spaces and all
must match.

| Term                                  | Matches                                             |
| ------------------------------------- | --------------------------------------------------- |
| `milk`, `"two words"`                 | Text in the item; a capital makes it case-sensitive |
| `+project`, `@context`, `+`, `@`      | A project or context; `+` or `@` alone, any         |
| `key:value`, `key:`                   | A tag, or the tag with any value                    |
| `pri:A`, `pri:A-C`, `pri:<=B`, `pri:` | A priority, a range, a comparison, or any           |
| `due:<=+7d`, `t:today`                | A date compared with `<` `<=` `>` `>=` `=`          |
| `is:blocked`                          | A state                                             |
| `a\|b`                                | Either                                              |
| `-TERM`                               | Not the whole term                                  |

The date fields are `due`, `t` (threshold), `started`, `created` and
`completed`; a date is `today`, a weekday such as `fri`, `+3d`, `-7d` or
`2026-10-01`. The states are `open`, `done`, `doing`, `blocked`, `later`,
`overdue`, `archived`, `dangling` and `stale`. As a `lsty ls` argument, a
`-TERM` needs `--` before it. `lsty help` describes the language too.

The Filters panel lists the built-in filters Today, Overdue, Upcoming, Doing,
Waiting and No project, then the saved ones.

### Configuration

The default configuration file is `~/.config/lsty/config.toml`, including
on macOS. `XDG_CONFIG_HOME` changes the configuration root, and `LSTY_CONFIG`
selects an alternative configuration file.

A minimal example:

```toml
theme = "catppuccin-mocha"
line_numbers = "hybrid"
auto_archive = false
```

**Files and writing.**

| Option              | Default | Accepts                               | Effect                                             |
| ------------------- | ------- | ------------------------------------- | -------------------------------------------------- |
| `default_file`      | unset   | a path; `~/` allowed                  | The task file when none is found any other way     |
| `auto_archive`      | `false` | `true`, `false`                       | Move completed items to the done file              |
| `include_creation`  | `true`  | `true`, `false`                       | Write a creation date on new items                 |
| `keep_priority`     | `true`  | `true`, `false`                       | Keep an item's priority when it is completed       |
| `default_priority`  | unset   | one letter `A`-`Z`                    | The priority of new items                          |
| `priority_range`    | `"D"`   | one letter `A`-`Z`                    | The last priority that `gp` and `gP` cycle through |
| `lock_timeout`      | `"2s"`  | a duration: `500ms`, `2s`, `4h`, `2d` | How long to wait for another writer's lock         |
| `claim_stale_after` | `"4h"`  | a duration                            | When an untouched claim counts as stale            |

**TUI.**

| Option              | Default                | Accepts                                      | Effect                                                                  |
| ------------------- | ---------------------- | -------------------------------------------- | ----------------------------------------------------------------------- |
| `theme`             | `"catppuccin-mocha"`   | a built-in or custom theme name              | The colour theme                                                        |
| `line_numbers`      | `"hybrid"`             | `hybrid`, `absolute`, `relative`, `off`      | How rows are numbered                                                   |
| `leader`            | `"<Space>"`            | one key in key notation                      | The leader key of `keys.toml` bindings                                  |
| `mouse`             | `true`                 | `true`, `false`                              | Act on clicks and the wheel                                             |
| `keep_adding`       | `false`                | `true`, `false`                              | After a new item is written, start another below it                     |
| `hidden_items`      | `["later", "archive"]` | any of `later`, `blocked`, `done`, `archive` | What the list hides at launch                                           |
| `start_filter`      | unset                  | a filter name, built-in or from `[filters]`  | The filter active at launch                                             |
| `upcoming_days`     | `7`                    | 1 or more                                    | How far ahead `Upcoming` and due-date colours look                      |
| `show_sidebar`      | `true`                 | `true`, `false`                              | Show the projects, contexts and filters panel                           |
| `sidebar_min_width` | `80`                   | 0 or more                                    | The terminal width below which the sidebar is hidden                    |
| `show_detail`       | `true`                 | `true`, `false`                              | Show the detail panel                                                   |
| `detail_min_width`  | `120`                  | 0 or more                                    | The terminal width below which the detail panel is hidden               |
| `inline_projects`   | `false`                | `true`, `false`                              | Show projects in a row's text instead of a column                       |
| `inline_contexts`   | `false`                | `true`, `false`                              | Show contexts in a row's text instead of a column                       |
| `paste_command`     | unset                  | a command line                               | What reads the clipboard; unset tries `wl-paste`, `xclip` and `pbpaste` |

```toml
# Saved filters, listed in the Filters panel after the built-in ones
[filters]
Urgent = "pri:A-B"
Errands = "@shop due:<=+7d"

# The actions shown in the hint row, per mode: normal, visual, insert,
# completion, command, panel, detail, help or picker.
[hints]
normal = ["toggle_complete", "insert_below", "undo", "help"]
```

Key bindings live in `keys.toml` and custom themes in `themes/` under the
configuration directory. `--no-config` ignores configuration, custom keys,
and custom themes.

## Scripts and agents

Use `--json` for structured CLI output. Agents must follow the built-in
guide:

```sh
lsty help --agent
```

### Concurrency

lsty coordinates reads and writes through advisory locks and atomic writes.
Other editors and tools might not honour these locks, so simultaneous writes by
those tools might conflict. Locking is not a cross-machine solution.

### Compatibility and help

The CLI is inspired by todo.sh, but may not always be a drop-in replacement.
lsty has been benchmarked against todo.sh for performance and compatibility,
but lsty intentionally diverges around some todo.sh quirks.

```sh
lsty help          # command overview, options, and environment variables
lsty help add      # help for one command
lsty help --agent  # coordination guidance for agents
```

## Development

[just](https://just.systems) wraps cargo:

```sh
just tools   # once: the tools the other recipes use
just         # format, lint, test
just build   # release binary in target/release/lsty
just install # build and install lsty
just --list  # every recipe
```

## Background

I wondered what it would be like to have agents populating and pulling from a
shared project todo list, where they can leave follow-ups for the human or next
agents. So it began as a ~2,500 line specification that was decomposed into 44
implementation specs. A software factory built it in Go and then that
was used as the reference implementation for the final Rust version. It was
then benchmarked against todo.sh. I've found it very useful (and entertaining)
to have a live "surface" to keep an eye on spec implementation progress.
Frontier LLMs have no issue using it without specific skills.

## Contributing

Bug reports and feature requests are welcome. Pull requests are not while this
remains an experiment in local agentic development. See
[Contributing](https://github.com/skaragianis/lsty/blob/main/CONTRIBUTING.md).

## License

Licensed under the
[Apache License, Version 2.0](https://github.com/skaragianis/lsty/blob/main/LICENSE).
