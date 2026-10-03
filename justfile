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

# Build release executables
build:
    cargo build --workspace --release

# Install the cargo tools the other recipes use
tools:
    cargo install --locked cargo-deny cargo-fuzz cargo-machete cargo-mutants
