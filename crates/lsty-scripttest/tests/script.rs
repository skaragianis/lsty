//! The harness's own transcripts, with `lsty` mapped to the stub.

use std::path::Path;
use std::process::ExitCode;

use lsty_scripttest::Suite;

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Suite::new(root, env!("CARGO_BIN_EXE_lsty-stub"))
        .only(&["00-harness"])
        .run()
}
