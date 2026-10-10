//! The golden frames under `testdata/tui/`: one test per transcript,
//! named `<folder>/<case>`. `UPDATE_SCRIPTS=1` updates the expected
//! sections.

use std::process::ExitCode;

use lsty_scripttest::{
    Outcome, run_trials, transcript_trials, txtar::Archive, workspace_root,
};
use lsty_tui::tuitest::script;

fn main() -> ExitCode {
    let root = workspace_root();
    let testdata = root.join("testdata");
    run_trials(transcript_trials(
        &testdata,
        "tui",
        move |name, bytes, update| {
            let Archive { comment, files } = Archive::parse(bytes)?;
            let (done, files) = script(&root, name, &comment, files, update);
            let bytes = files.map(|files| Archive { comment, files }.format());
            Ok((done.map_or_else(Outcome::Fail, |()| Outcome::Pass), bytes))
        },
        |_| true,
    ))
}
