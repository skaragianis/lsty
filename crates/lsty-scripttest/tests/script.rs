//! The harness's own transcripts, with `lsty` mapped to the stub.

use std::process::ExitCode;

use lsty_scripttest::Suite;

fn main() -> ExitCode {
    Suite::new(env!("CARGO_BIN_EXE_lsty-stub"))
        .only(&["00-harness"])
        .run()
}
