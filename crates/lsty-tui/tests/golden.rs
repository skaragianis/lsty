//! The golden frames under `testdata/tui/`: one test per transcript,
//! named `<folder>/<case>`. A transcript listed in `testdata/xfail` must
//! fail; `UPDATE_SCRIPTS=1` updates the expected sections of the others.

use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use lsty_scripttest::txtar::ByteArchive;
use lsty_scripttest::{RunFile, run_trials, transcript_trials};
use lsty_tui::tuitest::script;

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = fs::canonicalize(&root).unwrap_or(root);
    let testdata = root.join("testdata");
    let run: Arc<RunFile> = Arc::new(move |name, bytes, update| {
        let archive = ByteArchive::parse(bytes);
        let comment = archive.comment;
        let (done, files) =
            script(&root, name, &comment, archive.files, update);
        let bytes = files.map(|files| ByteArchive { comment, files }.format());
        (done, bytes)
    });
    run_trials(transcript_trials(&testdata, "tui", &run, |_| true))
}
