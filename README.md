# lsty

A blazingly fast, vim-inspired todo.txt TUI for humans, written in Rust, with
a todo.sh-inspired CLI built for safe concurrent use by humans, scripts, and
LLM agents.

## Development

[just](https://just.systems) wraps cargo:

```sh
just tools   # once: the tools the other recipes use
just         # format, lint, test
just build   # release binary in target/release/lsty
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
remains an experiment in local agentic development. See [Contributing](CONTRIBUTING.md).

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
