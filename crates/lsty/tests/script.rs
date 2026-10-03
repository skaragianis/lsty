//! The transcripts under `testdata/script/`, run against `lsty`, or
//! against the program that `LSTY_REFERENCE` names.

use std::path::Path;
use std::process::ExitCode;

use lsty_scripttest::Suite;

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Suite::new(root)
        .program("lsty", env!("CARGO_BIN_EXE_lsty"))
        .reference("lsty", "LSTY_REFERENCE")
        .except(&["00-harness"])
        .run()
}
