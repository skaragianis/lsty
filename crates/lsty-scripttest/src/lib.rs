//! The transcript harness of lsty.
//!
//! It runs the `.txtar` transcripts under `testdata/script/` with
//! testscript's syntax and commands, plus lsty's `exit`, `match` and
//! `cmpjson` commands and the `<idN>` and `<any>` placeholders.
#![expect(
    clippy::disallowed_methods,
    reason = "tests build paths of their own, which are UTF-8"
)]

mod diff;
mod json;
mod parse;
mod placeholder;
mod pty;
pub use pty::open_pty;
mod script;
pub mod txtar;

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::Arc,
};

use libtest_mimic::{Arguments, Failed, Trial};

use crate::{script::Config, txtar::Archive};

/// How a transcript ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Every line ran as expected.
    Pass,
    /// `skip` ended the script, with its reason.
    Skip(String),
    /// A line failed; the log of the script up to the failure.
    Fail(String),
}

/// The workspace folder, which holds `testdata/`. Found from the package
/// cargo runs the test in, not the one it was compiled in.
///
/// # Panics
///
/// When cargo did not start the test.
#[must_use]
#[expect(clippy::expect_used, reason = "only cargo runs the tests")]
pub fn workspace_root() -> PathBuf {
    let dir = host_var("CARGO_MANIFEST_DIR")
        .expect("cargo sets CARGO_MANIFEST_DIR when it runs the tests");
    let root = Path::new(&dir).join("../..");
    fs::canonicalize(&root).unwrap_or(root)
}

/// A set of transcripts and the `lsty` program they run.
#[derive(Debug)]
pub struct Suite {
    root: PathBuf,
    lsty: PathBuf,
    only: Vec<String>,
    except: Vec<String>,
}

impl Suite {
    /// The transcripts of the workspace, run against the program at `lsty`.
    pub fn new(lsty: impl Into<PathBuf>) -> Self {
        Self {
            root: workspace_root(),
            lsty: lsty.into(),
            only: Vec::new(),
            except: Vec::new(),
        }
    }

    /// Runs only the transcripts of these folders, such as `00-harness`.
    #[must_use]
    pub fn only(mut self, folders: &[&str]) -> Self {
        self.only = folders.iter().map(|s| (*s).to_owned()).collect();
        self
    }

    /// Skips the transcripts of these folders.
    #[must_use]
    pub fn except(mut self, folders: &[&str]) -> Self {
        self.except = folders.iter().map(|s| (*s).to_owned()).collect();
        self
    }

    /// Runs the suite as a test binary with `harness = false`: one test per
    /// transcript, named `<folder>/<case>`.
    pub fn run(self) -> ExitCode {
        run_trials(self.trials())
    }

    /// Runs one transcript given as text, without rewriting anything.
    pub fn run_source(&self, name: &str, source: &str) -> Outcome {
        match self.config() {
            Ok(config) => match Archive::parse(source.as_bytes()) {
                Ok(archive) => script::run(&config, name, archive, false).0,
                Err(e) => Outcome::Fail(e),
            },
            Err(e) => Outcome::Fail(e),
        }
    }

    fn testdata(&self) -> PathBuf {
        self.root.join("testdata")
    }

    fn config(&self) -> Result<Config, String> {
        let bin = bin_folder(&self.lsty)?;
        let host_path = host_var("PATH").unwrap_or_default();
        let path = if host_path.is_empty() {
            bin.path().display().to_string()
        } else {
            format!("{}:{host_path}", bin.path().display())
        };
        Ok(Config {
            testdata: self.testdata(),
            path,
            _bin: bin,
            keep_work: host_var("KEEP_WORK").is_some_and(|v| v == "1"),
        })
    }

    fn trials(&self) -> Result<Vec<Trial>, String> {
        let config = self.config()?;
        transcript_trials(
            &self.testdata(),
            "script",
            move |name, bytes, update| {
                let archive = Archive::parse(bytes)?;
                Ok(script::run(&config, name, archive, update))
            },
            |name| self.wanted_group(name),
        )
    }

    /// Whether the folder `name` is one this suite runs.
    fn wanted_group(&self, name: &str) -> bool {
        (self.only.is_empty() || self.only.iter().any(|o| o == name))
            && !self.except.iter().any(|e| e == name)
    }
}

/// What running a transcript gives: how it ended and its new bytes when it
/// rewrote itself, or why it could not be run at all.
type FileRun = Result<(Outcome, Option<Vec<u8>>), String>;

/// Runs trials as a test binary with `harness = false`; a failure to build
/// them is one failing test.
pub fn run_trials(trials: Result<Vec<Trial>, String>) -> ExitCode {
    let args = Arguments::from_args();
    let trials = trials.unwrap_or_else(|e| {
        vec![Trial::test("transcripts", move || Err(e.into()))]
    });
    libtest_mimic::run(&args, trials).exit_code()
}

/// One trial per `.txtar` file under `testdata/<sub>/<folder>/` for each
/// folder `wanted` accepts, named `<folder>/<case>`. `UPDATE_SCRIPTS=1`
/// lets `run` rewrite them.
pub fn transcript_trials(
    testdata: &Path,
    sub: &str,
    run: impl Fn(&str, &[u8], bool) -> FileRun + Send + Sync + 'static,
    wanted: impl Fn(&str) -> bool,
) -> Result<Vec<Trial>, String> {
    let run = Arc::new(run);
    let update = host_var("UPDATE_SCRIPTS").is_some_and(|v| v == "1");
    let mut trials = Vec::new();
    for group in sorted_entries(&testdata.join(sub))? {
        let group_name =
            group.file_name().unwrap_or_default().to_string_lossy();
        if !group.is_dir() || !wanted(&group_name) {
            continue;
        }
        for file in sorted_entries(&group)? {
            if file.extension().is_none_or(|e| e != "txtar") {
                continue;
            }
            let case = file.file_stem().unwrap_or_default().to_string_lossy();
            let name = format!("{group_name}/{case}");
            let run = Arc::clone(&run);
            trials.push(Trial::test(name.clone(), move || {
                run_file(&*run, &name, &file, update)
            }));
        }
    }
    Ok(trials)
}

fn run_file(
    run: &impl Fn(&str, &[u8], bool) -> FileRun,
    name: &str,
    file: &Path,
    rewrite: bool,
) -> Result<(), Failed> {
    let bytes =
        fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let (outcome, updated) = run(name, &bytes, rewrite)
        .map_err(|e| format!("{}: {e}", file.display()))?;
    if let Some(bytes) = updated {
        fs::write(file, bytes)
            .map_err(|e| format!("{}: {e}", file.display()))?;
    }
    match outcome {
        Outcome::Fail(log) => Err(log.into()),
        Outcome::Pass | Outcome::Skip(_) => Ok(()),
    }
}

/// A folder with a link to `lsty`, put first on the scripts' `PATH`,
/// where `exec` and any shell a script starts find it.
fn bin_folder(lsty: &Path) -> Result<tempfile::TempDir, String> {
    let bin = tempfile::Builder::new()
        .prefix("lsty-bin-")
        .tempdir()
        .map_err(|e| format!("bin folder: {e}"))?;
    let target = fs::canonicalize(lsty)
        .map_err(|e| format!("{}: {e}", lsty.display()))?;
    let link = bin.path().join("lsty");
    std::os::unix::fs::symlink(&target, &link)
        .map_err(|e| format!("{}: {e}", link.display()))?;
    Ok(bin)
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

/// Passes the host's `LLVM_PROFILE_FILE` to a program started with a
/// cleared environment, so a coverage run counts it.
#[expect(
    clippy::disallowed_methods,
    reason = "the harness, not lsty, passes the coverage profile through"
)]
pub fn with_coverage(cmd: &mut Command) -> &mut Command {
    if let Some(file) = std::env::var_os("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", file);
    }
    cmd
}

/// A setting of the harness itself, read from the host environment.
#[expect(
    clippy::disallowed_methods,
    reason = "the harness, not lsty, reads its settings and the host PATH"
)]
fn host_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_program_is_refused() {
        let err = bin_folder(Path::new("/no/such/lsty")).unwrap_err();
        assert!(err.starts_with("/no/such/lsty: "), "{err}");
    }
}
