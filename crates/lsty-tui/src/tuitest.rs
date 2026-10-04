//! A headless driver for the golden-frame transcripts under
//! `testdata/tui/`.
//!
//! A transcript is a script and the files it needs, in the txtar format.
//! The model is driven directly through [`Model::update`](crate::Model),
//! without a terminal. Commands that ask for a timer are held until
//! `tick`.
//!
//! | Command | Meaning |
//! | --- | --- |
//! | `size W H` | the terminal's size; 80 × 24 when not given |
//! | `now STAMP` | the clock; `2026-09-29T0900Z` when not given |
//! | `start [FILE]` | create the model on FILE (`todo.txt` when left out) |
//! | `keys K…` | press keys, each argument in key notation |
//! | `type TEXT` | press one key per character of TEXT |
//! | `frame NAME` | the screen as plain text must equal the section NAME |
//! | `fg NAME`, `bg NAME` | the foreground or background layer must equal the section NAME |
//! | `status PATTERN` | the status line must match the regular expression |
//! | `file PATH NAME` | the file must equal the section NAME |
//! | `tick DURATION` | move the clock forward and deliver the timers that are due |
//! | `write FILE SECTION` | replace FILE with the section, as another writer would (a temporary file, renamed), and send a watcher event |
//! | `remove FILE` | delete a file or a folder and send a watcher event |
//! | `event [error REASON]` | send a watcher event, or a watcher failure |
//! | `click X Y`, `shiftclick X Y`, `dblclick X Y`, `rightclick X Y` | click at the cell, counted from 0 |
//! | `clickon TEXT`, `shiftclickon TEXT`, `dblclickon TEXT`, `rightclickon TEXT` | click at the first cell of the first place the screen shows TEXT (quoted or one word) |
//! | `drag X1 Y1 X2 Y2`, `dragon TEXT1 TEXT2` | press, move one row at a time, release |
//! | `down X Y`, `move X Y`, `up X Y` | a press, a motion with the button held, a release, each alone |
//! | `wheel up\|down X Y [N]` | N notches of the wheel at the cell, 1 when left out |
//! | `lock hold`, `lock release` | hold the lock of the todo file's folder, or free it |

mod legend;
mod run;
mod screen;
#[cfg(test)]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use libtest_mimic::{Arguments, Failed, Trial};
use lsty_scripttest::read_xfail;

/// The transcripts of the workspace at `root`, the folder that holds
/// `testdata/`.
#[derive(Clone, Debug)]
pub struct Suite {
    root: PathBuf,
}

impl Suite {
    /// The transcripts of the workspace at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            root: fs::canonicalize(&root).unwrap_or(root),
        }
    }

    /// Runs the suite as a test binary with `harness = false`: one test
    /// per transcript, named `<folder>/<case>`. A transcript listed in
    /// `testdata/xfail` must fail; `UPDATE_SCRIPTS=1` updates the
    /// expected sections of the others.
    pub fn run(self) -> ExitCode {
        let args = Arguments::from_args();
        let trials = self.trials().unwrap_or_else(|e| {
            vec![Trial::test("transcripts", move || Err(e.into()))]
        });
        libtest_mimic::run(&args, trials).exit_code()
    }

    fn trials(&self) -> Result<Vec<Trial>, String> {
        let testdata = self.root.join("testdata");
        let xfail = read_xfail(&testdata.join("xfail"))?;
        let update = setting("UPDATE_SCRIPTS").is_some_and(|v| v == "1");
        let mut trials = Vec::new();
        for group in sorted_entries(&testdata.join("tui"))? {
            if group.is_dir() {
                trials.extend(self.group_trials(&group, &xfail, update)?);
            }
        }
        Ok(trials)
    }

    /// One trial per transcript of the folder `group`.
    fn group_trials(
        &self,
        group: &Path,
        xfail: &[String],
        update: bool,
    ) -> Result<Vec<Trial>, String> {
        let group_name = file_name(group).unwrap_or_default();
        let mut trials = Vec::new();
        for file in sorted_entries(group)? {
            let Some(case) = case_name(&file) else {
                continue;
            };
            let name = format!("{group_name}/{case}");
            let failing = xfail.iter().any(|x| *x == name || *x == group_name);
            trials.push(self.trial(name, file, failing, update));
        }
        Ok(trials)
    }

    fn trial(
        &self,
        name: String,
        file: PathBuf,
        failing: bool,
        update: bool,
    ) -> Trial {
        let root = Arc::new(self.root.clone());
        Trial::test(name.clone(), move || {
            check(&root, &name, &file, failing, update && !failing)
        })
    }
}

/// Runs one transcript and compares how it ended with what is expected.
fn check(
    root: &Path,
    name: &str,
    file: &Path,
    failing: bool,
    update: bool,
) -> Result<(), Failed> {
    let source =
        fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let (result, updated) = run::script(root, name, &source, update);
    if let Some(bytes) = updated {
        fs::write(file, bytes)
            .map_err(|e| format!("{}: {e}", file.display()))?;
    }
    match (result, failing) {
        (Ok(()), true) => Err(format!(
            "{name} passes but is listed in testdata/xfail; remove it there"
        )
        .into()),
        (Err(log), false) => Err(log.into()),
        (Ok(()), false) | (Err(_), true) => Ok(()),
    }
}

/// The case name of a `.txtar` file: its stem.
fn case_name(file: &Path) -> Option<&str> {
    if file.extension().is_none_or(|e| e != "txtar") {
        return None;
    }
    file.file_stem().and_then(|s| s.to_str())
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name()?.to_str().map(str::to_owned)
}

fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let read = |e: std::io::Error| format!("{}: {e}", dir.display());
    let mut entries = fs::read_dir(dir)
        .map_err(read)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(read)?;
    entries.sort();
    Ok(entries)
}

/// A setting of the harness itself, read from the host environment.
#[expect(
    clippy::disallowed_methods,
    reason = "the harness, not lsty, reads its settings"
)]
fn setting(name: &str) -> Option<String> {
    std::env::var(name).ok()
}
