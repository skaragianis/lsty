# Tidy then test
default: tidy test

# Format, then lint
tidy:
    cargo fmt --all
    @just lint

# Run clippy, cargo-deny and cargo-machete
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo deny check
    cargo machete

# Run all tests
test:
    cargo test --workspace --all-features

# Rewrite the expected output of failing transcripts
update:
    UPDATE_SCRIPTS=1 cargo test --workspace --all-features

# Run every transcript against another lsty binary, ignoring xfail
diff reference:
    LSTY_REFERENCE="$(realpath {{reference}})" cargo test -p lsty --test script

# Mutation-test lsty-core
mutants:
    cargo mutants --package lsty-core

# Build release executables
build:
    cargo build --workspace --release

# Install the cargo tools the other recipes use
tools:
    cargo install --locked cargo-deny cargo-machete cargo-mutants
