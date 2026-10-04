//! The golden frames under `testdata/tui/`.

use std::path::Path;
use std::process::ExitCode;

use lsty_tui::tuitest::Suite;

fn main() -> ExitCode {
    Suite::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).run()
}
