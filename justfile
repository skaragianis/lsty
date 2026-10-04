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
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo check --manifest-path fuzz/Cargo.toml --bins --locked
    cargo deny check
    RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --workspace
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

# Fuzz every target for SECONDS each, starting from its seeds
fuzz seconds="60":
    cd fuzz && for t in $(cargo fuzz list); do \
        mkdir -p corpus/$t && \
        cargo fuzz run $t corpus/$t seeds/$t -- -max_total_time={{seconds}} \
            || exit 1; \
    done

# Mutation-test lsty-core
mutants:
    cargo mutants --package lsty-core

# Measure test coverage, transcripts included; HTML in target/llvm-cov/html
cover:
    cargo llvm-cov --workspace --all-features --html

# Build release executables
build:
    cargo build --workspace --release

# Install the cargo tools the other recipes use
tools:
    rustup component add llvm-tools-preview
    cargo install --locked cargo-deny cargo-fuzz cargo-llvm-cov \
        cargo-machete cargo-mutants
