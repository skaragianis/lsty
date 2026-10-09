//! The transcripts under `testdata/script/`, run against `lsty`.

use std::path::Path;
use std::process::ExitCode;

use lsty_scripttest::Suite;

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Suite::new(root, env!("CARGO_BIN_EXE_lsty"))
        .except(&["00-harness"])
        .run()
}
