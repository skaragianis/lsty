# Tidy then test
default: tidy test

# Format, then lint
tidy:
    cargo fmt --all
    @just lint

# Run rustfmt on the fuzz crate too, clippy with all features and with the default ones, a build check of the fuzz targets with warnings denied, cargo-deny, rustdoc with warnings denied and cargo-machete
lint:
    cargo fmt --all --check
    cargo fmt --manifest-path fuzz/Cargo.toml --check
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
    cargo clippy --workspace --all-targets --locked -- -D warnings
    RUSTFLAGS=-Dwarnings cargo check --manifest-path fuzz/Cargo.toml --bins --locked
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
diff runs="1000" seed="1" lines="":
    #!/usr/bin/env sh
    set -eu
    test -x bin/todo.sh || { echo "bin/todo.sh is missing: run just tools" >&2; exit 1; }
    TODO_SH="$(pwd)/bin/todo.sh" LSTY_DIFF_RUNS={{runs}} LSTY_DIFF_SEED={{seed}} \
        LSTY_DIFF_LINES="{{lines}}" cargo test -p lsty --test todo_sh --release --locked

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

# Time every command shared with todo.sh end to end with hyperfine; LINES in each file, SAVE names the result (bench/SAVE.json in the target folder), RUNS timed runs per command
bench lines="10000" save="" runs="40":
    #!/usr/bin/env sh
    set -eu
    command -v hyperfine >/dev/null || { echo "hyperfine is missing: run just tools" >&2; exit 1; }
    LSTY_BENCH=1 LSTY_BENCH_LINES={{lines}} LSTY_BENCH_SAVE="{{save}}" \
        LSTY_BENCH_RUNS={{runs}} \
        cargo test -p lsty --test bench --release --locked -j 3 -- --nocapture measure

# Print the median of each command in two saved bench results and the change
bench-compare base new:
    LSTY_BENCH_COMPARE="{{base}} {{new}}" \
        cargo test -p lsty --test bench --release --locked -j 3 -- --nocapture compare

# Run the terminal tests in 8 loops at once, so up to 8 lsty processes, for MINUTES; logs of failed runs stay in target/stress
stress minutes="10":
    #!/usr/bin/env sh
    set -eu
    case "{{minutes}}" in
        ''|*[!0-9]*|0?*) echo "minutes must be a whole number" >&2; exit 1 ;;
    esac
    built="$(cargo test -p lsty --test terminal --locked --no-run --color never 2>&1)" \
        || { printf '%s\n' "$built" >&2; exit 1; }
    exe="$(printf '%s\n' "$built" \
        | sed -n 's/.*Executable tests\/terminal\.rs (\(.*\))$/\1/p')"
    test -n "$exe" || { echo "no terminal test executable in cargo's output" >&2; exit 1; }
    dir=target/stress
    rm -rf "$dir"
    mkdir -p "$dir"
    end=$(( $(date +%s) + {{minutes}} * 60 ))
    for loop in 1 2 3 4 5 6 7 8; do
        (
            n=0
            while [ "$(date +%s)" -lt "$end" ]; do
                n=$((n + 1))
                log="$dir/$loop-$n.log"
                if "$exe" --test-threads=1 --color never > "$log" 2>&1; then
                    rm "$log"; echo pass
                else
                    echo fail
                fi
            done > "$dir/$loop.runs"
        ) &
    done
    wait
    runs=$(cat "$dir"/*.runs | wc -l | tr -d ' ')
    failed=$(cat "$dir"/*.runs | grep -c fail || true)
    echo "$runs runs, $failed failed"
    test "$failed" -eq 0 && exit 0
    cat "$dir"/*.log | sed -n 's/^test \(.*[^ ]\) *\.\.\. FAILED$/\1/p' | sort | uniq -c
    echo "logs in $dir" >&2
    exit 1

# Build the release executable
build:
    cargo build --release --locked -p lsty

# Install the release executable
install:
    cargo install --locked --path crates/lsty

# Write the licenses of the crates in the lsty executable to target/
licenses:
    mkdir -p target
    cargo about generate --locked --fail -m crates/lsty/Cargo.toml \
        -c release/about.toml release/about.hbs -o target/THIRD-PARTY-LICENSES

# Tag the version in Cargo.toml as vVERSION and push main and the tag together, which starts the release build
release:
    #!/usr/bin/env sh
    set -eu
    fail() { echo "release: $*" >&2; exit 1; }
    cargo metadata --locked --format-version 1 >/dev/null \
        || fail "cargo metadata --locked failed"
    id="$(cargo pkgid --locked -p lsty)"
    version="${id##*[#@]}"
    tag="v$version"
    test "$(git symbolic-ref --short -q HEAD)" = main || fail "not on main"
    test -z "$(git status --porcelain)" || fail "the working tree has changes"
    git fetch --quiet origin main
    git merge-base --is-ancestor origin/main HEAD || fail "main is behind origin/main"
    ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || fail "$tag exists already"
    remote="$(git ls-remote --tags origin "refs/tags/$tag")" || fail "can't read the tags on origin"
    test -z "$remote" || fail "$tag exists on origin"
    tags="$(git ls-remote --tags --refs origin 'v*')" || fail "can't read the tags on origin"
    latest="$({ printf '%s\n' "$tags" | sed 's|.*refs/tags/||'; git tag --list 'v*'; } | sed '/^$/d; s/-/~/' | sort -V | tail -n 1 | sed 's/~/-/')"
    test -z "$latest" || test "$(printf '%s\n%s\n' "$latest" "$tag" | sed 's/-/~/' | sort -V | tail -n 1 | sed 's/~/-/')" = "$tag" \
        || fail "$tag is not newer than $latest"
    git tag -a "$tag" -m "lsty $version"
    if ! git push --atomic origin main "$tag"; then
        theirs="$(git ls-remote --tags origin "refs/tags/$tag")" \
            || fail "push failed and origin can't be read; $tag kept"
        test -n "$theirs" || { git tag -d "$tag" >/dev/null; fail "push failed; $tag removed again"; }
        test "${theirs%%[[:space:]]*}" = "$(git rev-parse "refs/tags/$tag")" \
            || fail "push failed; origin has a different $tag; $tag kept"
        echo "release: the push reported a failure, but origin has $tag" >&2
    fi
    echo "pushed $tag"

# Install the cargo tools the other recipes use, and todo.sh into bin/
tools:
    rustup component add llvm-tools-preview
    cargo install --locked cargo-deny cargo-fuzz cargo-llvm-cov \
        cargo-machete cargo-mutants hyperfine
    cargo install --locked cargo-about@0.9.2 --features cli
    just todo-sh

# Install current demo recording tools with Homebrew
[macos]
demo-tools:
    #!/usr/bin/env sh
    set -eu
    command -v brew >/dev/null || { echo "demo-tools requires Homebrew on macOS" >&2; exit 1; }
    brew install vhs ttyd ffmpeg

# Install current demo recording tools with apt (amd64/arm64; uses sudo)
[linux]
demo-tools:
    #!/usr/bin/env sh
    set -eu
    command -v apt-get >/dev/null || { echo "demo-tools requires apt-get on Linux" >&2; exit 1; }
    arch="$(dpkg --print-architecture)"
    case "$arch" in
        amd64) ttyd_asset="ttyd.x86_64" ;;
        arm64) ttyd_asset="ttyd.aarch64" ;;
        *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
    esac
    sudo apt-get update
    sudo apt-get install --yes ca-certificates curl gpg
    dir="$(mktemp -d)"
    trap 'rm -rf "$dir"' EXIT
    curl -fsSL -o "$dir/charm.key" https://repo.charm.sh/apt/gpg.key
    gpg --batch --yes --dearmor --output "$dir/charm.gpg" "$dir/charm.key"
    sudo install -D -m 0644 "$dir/charm.gpg" /etc/apt/keyrings/charm.gpg
    printf '%s\n' 'deb [signed-by=/etc/apt/keyrings/charm.gpg] https://repo.charm.sh/apt/ * *' > "$dir/charm.list"
    sudo install -D -m 0644 "$dir/charm.list" /etc/apt/sources.list.d/charm.list
    sudo apt-get update
    sudo apt-get install --yes vhs ffmpeg chromium fonts-dejavu-core
    curl -fsSL -o "$dir/ttyd" \
        "https://github.com/tsl0922/ttyd/releases/latest/download/$ttyd_asset"
    sudo install -m 0755 "$dir/ttyd" /usr/local/bin/ttyd
    echo "Installed current VHS and ttyd releases, FFmpeg, Chromium, and DejaVu fonts."
    echo "Run recordings as your normal user, not with sudo."

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
