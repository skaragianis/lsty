# Contributing

Sincerely, thanks for your interest in lsty! 😊

## Bug reports and feature requests

Issues are welcome, especially bug reports, usability feedback, and feature
requests.

For bugs, please include your lsty version, operating system, steps to
reproduce the problem, and what you expected to happen.

For feature requests, please describe the problem you want solved and if
there's a current work around.

Security problems go through a private report, not an issue: see
[SECURITY.md](SECURITY.md).

## Pull requests

Pull requests are welcome against `main`.

Small fixes (a bug, a typo, a missing test) can come straight as a pull
request. Please open an issue first for anything bigger, so we can agree on
the shape before you spend time on it, and always before changing:

- a large feature, or a new command or option;
- the file format: what lsty reads or writes in `todo.txt` and `done.txt`;
- compatibility with todo.sh, which matters to lsty;
- concurrency: locking, atomic writes, or watching the file.

Review is best effort, and a pull request may not be accepted even when it
works.

## Setup

lsty builds on Linux and macOS only. You need
[rustup](https://rustup.rs) (the toolchain is pinned in
`rust-toolchain.toml`) and [just](https://just.systems), then:

```sh
just tools   # once: the tools the other recipes use
just         # format, lint, test
```

- `just lint` runs rustfmt (nightly, for import ordering), clippy with
  warnings as errors, rustdoc, `cargo deny` and `cargo machete`.
- `just test` runs every test, and golden frames of the interactive view. They
  need `--all-features`, so plain `cargo test --workspace` is not enough.
- `just update` rewrites the expected output of failing transcripts and
  golden frames. Check that only the ones you meant to change changed.

## Code

- Clippy is strict: `pedantic` is on; `unwrap`, `expect`, `panic` and
  printing are denied outside of tests; and `#[allow]` is denied in favour of
  `#[expect(…, reason = "…")]`. `clippy.toml` caps function length,
  nesting, complexity and argument count, and bans reading the
  environment, the clock or the arguments: those come in through `cli::Env`
  and the injected clock.
- `unsafe` is forbidden, and lines stay under 80 characters (I know I know..)
- Argument parsing is hand-written: please don't add an argument parser crate.
- Code that only works on one system needs its tests marked for the system
  they can pass on, as `crates/lsty/tests/terminal.rs` does.
- Don't commit `assets/demo.gif`; it is re-rendered for a release.

## Commits and pull request titles

- Title the pull request as a conventional commit:
  `type(scope): lowercase imperative`, for example
  `fix(cli): keep the priority on append`. It becomes the commit subject
  when the pull request is squashed.
- Author your commits under your own name and email. Whoever wrote the
  code, with or without tools, you stand behind the change: you have read
  every line and can answer for it in review.
- Keep `main`'s history linear: rebase on `main` rather than merging it in.

## What CI checks

Every pull request must pass one required check, `checks`, which waits for:

- `just lint-rust` and `just test` on Linux and macOS;
- `cargo deny` and `cargo machete`;
- `cargo publish --dry-run` for the workspace;
- the four release archives (Linux and macOS, x86_64 and aarch64), built
  and run from the archive.

CodeQL also scans every pull request. A first-time contributor's workflows
wait for approval before they run.
