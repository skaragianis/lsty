//! The golden frames under `testdata/tui/`: one test per transcript,
//! named `<folder>/<case>`. A transcript listed in `testdata/xfail` must
//! fail; `UPDATE_SCRIPTS=1` updates the expected sections of the others.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use libtest_mimic::{Arguments, Failed, Trial};
use lsty_scripttest::read_xfail;
use lsty_scripttest::txtar::ByteArchive;
use lsty_tui::tuitest::{Transcript, script};

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = fs::canonicalize(&root).unwrap_or(root);
    let args = Arguments::from_args();
    let trials = trials(&root).unwrap_or_else(|e| {
        vec![Trial::test("transcripts", move || Err(e.into()))]
    });
    libtest_mimic::run(&args, trials).exit_code()
}

fn trials(root: &Path) -> Result<Vec<Trial>, String> {
    let testdata = root.join("testdata");
    let xfail = read_xfail(&testdata.join("xfail"))?;
    let update = setting("UPDATE_SCRIPTS").is_some_and(|v| v == "1");
    let mut trials = Vec::new();
    for group in sorted_entries(&testdata.join("tui"))? {
        if group.is_dir() {
            trials.extend(group_trials(root, &group, &xfail, update)?);
        }
    }
    Ok(trials)
}

/// One trial per transcript of the folder `group`.
fn group_trials(
    root: &Path,
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
        let root = root.to_path_buf();
        trials.push(Trial::test(name.clone(), move || {
            check(&root, &name, &file, failing, update && !failing)
        }));
    }
    Ok(trials)
}

/// Runs one transcript and compares how it ended with what is expected.
fn check(
    root: &Path,
    name: &str,
    file: &Path,
    failing: bool,
    update: bool,
) -> Result<(), Failed> {
    let bytes =
        fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let parsed = ByteArchive::parse(&bytes);
    let source = Transcript {
        comment: parsed.comment,
        files: parsed.files,
    };
    let (result, updated) = script(root, name, &source, update);
    if let Some(new) = updated {
        let bytes = ByteArchive {
            comment: new.comment,
            files: new.files,
        }
        .format();
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
