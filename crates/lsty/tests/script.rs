//! The transcripts under `testdata/script/`, run against `lsty`.

use std::process::ExitCode;

use lsty_scripttest::Suite;

fn main() -> ExitCode {
    Suite::new(env!("CARGO_BIN_EXE_lsty"))
        .except(&["00-harness"])
        .run()
}
