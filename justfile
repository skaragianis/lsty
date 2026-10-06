# Tidy then test
default: tidy test

# Format, then lint
tidy:
    cargo fmt --all
    @just lint

# Run rustfmt on the fuzz crate too, clippy, a build check of the fuzz targets, cargo-deny, rustdoc with warnings denied and cargo-machete
lint:
    cargo fmt --all --check
    cargo fmt --manifest-path fuzz/Cargo.toml --check
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
    cargo check --manifest-path fuzz/Cargo.toml --bins --locked
    cargo deny --locked check
    RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --workspace --locked
    cargo machete

# Run all tests
test:
    cargo test --workspace --all-features --locked

# Rewrite the expected output of failing transcripts
update:
    UPDATE_SCRIPTS=1 cargo test --workspace --all-features --locked

# Run random command sequences through lsty and todo.sh and compare the files
diff runs="1000" seed="1":
    #!/usr/bin/env sh
    set -eu
    test -x bin/todo.sh || { echo "bin/todo.sh is missing: run just tools" >&2; exit 1; }
    TODO_SH="$(pwd)/bin/todo.sh" LSTY_DIFF_RUNS={{runs}} LSTY_DIFF_SEED={{seed}} \
        cargo test -p lsty --test todo_sh --locked

# Fuzz every target for SECONDS each, starting from its seeds
fuzz seconds="60":
    cd fuzz && cargo metadata --locked --format-version 1 >/dev/null && \
        for t in $(cargo fuzz list); do \
            mkdir -p corpus/$t && \
            cargo fuzz run $t corpus/$t seeds/$t \
                -- -max_total_time={{seconds}} || exit 1; \
        done

# Mutation-test lsty-core on at most three cores
mutants:
    cargo mutants --package lsty-core -j 3 --jobserver-tasks 3 -- -- --test-threads=1

# Measure test coverage, transcripts included; HTML in target/llvm-cov/html
cover:
    cargo llvm-cov --workspace --all-features --locked --html

# Build the release executable
build:
    cargo build --release --locked -p lsty

# Install the cargo tools the other recipes use, and todo.sh into bin/
tools:
    rustup component add llvm-tools-preview
    cargo install --locked cargo-deny cargo-fuzz cargo-llvm-cov \
        cargo-machete cargo-mutants
    just todo-sh

todo_sh_version := "2.13.0"
todo_sh_sha256 := "d3b925434029aac212213c103fb6573a4f960c74dd467a3efac9bd9afe89d15f"

# Install todo.sh (the todo.txt command line `just diff` compares with) into bin/
todo-sh:
    #!/usr/bin/env sh
    set -eu
    stamp="{{todo_sh_version}} {{todo_sh_sha256}}"
    test -x bin/todo.sh && test "$(cat bin/todo.sh.version 2>/dev/null)" = "$stamp" && exit 0
    dir="$(mktemp -d)"
    trap 'rm -rf "$dir"' EXIT
    curl -fsSL -o "$dir/todo.tgz" \
        "https://github.com/todotxt/todo.txt-cli/releases/download/v{{todo_sh_version}}/todo.txt_cli-{{todo_sh_version}}.tar.gz"
    if command -v sha256sum >/dev/null; then
        sum="$(sha256sum "$dir/todo.tgz" | cut -d' ' -f1)"
    else
        sum="$(shasum -a 256 "$dir/todo.tgz" | cut -d' ' -f1)"
    fi
    test "$sum" = "{{todo_sh_sha256}}" || { echo "todo.sh: unexpected checksum $sum" >&2; exit 1; }
    tar -xzf "$dir/todo.tgz" -C "$dir"
    mkdir -p bin
    cp "$dir/todo.txt_cli-{{todo_sh_version}}/todo.sh" bin/todo.sh
    chmod +x bin/todo.sh
    echo "$stamp" > bin/todo.sh.version
