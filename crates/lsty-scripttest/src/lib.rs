//! The transcript harness of lsty.
//!
//! It runs the `.txtar` transcripts under `testdata/script/` with
//! testscript's syntax and commands, plus lsty's `exit`, `match` and
//! `cmpjson` commands and the `<idN>` and `<any>` placeholders.

mod diff;
mod json;
mod parse;
mod placeholder;
mod pty;
mod script;
pub mod txtar;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use libtest_mimic::{Arguments, Failed, Trial};

use crate::script::Config;

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

/// A set of transcripts and the programs they run.
#[derive(Debug, Clone)]
pub struct Suite {
    root: PathBuf,
    programs: BTreeMap<String, PathBuf>,
    only: Vec<String>,
    except: Vec<String>,
}

impl Suite {
    /// The transcripts of the workspace at `root`, the folder that holds
    /// `testdata/`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let root = fs::canonicalize(&root).unwrap_or(root);
        Self {
            root,
            programs: BTreeMap::new(),
            only: Vec::new(),
            except: Vec::new(),
        }
    }

    /// Makes `exec name` run the program at `path`.
    #[must_use]
    pub fn program(
        mut self,
        name: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Self {
        self.programs.insert(name.into(), path.into());
        self
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
            Ok(config) => script::run(&config, name, source, false).0,
            Err(e) => Outcome::Fail(e),
        }
    }

    fn testdata(&self) -> PathBuf {
        self.root.join("testdata")
    }

    fn config(&self) -> Result<Config, String> {
        let bin = bin_folder(&self.programs)?;
        let host_path = host_var("PATH").unwrap_or_default();
        let path = if host_path.is_empty() {
            bin.path().display().to_string()
        } else {
            format!("{}:{host_path}", bin.path().display())
        };
        Ok(Config {
            programs: self.programs.clone(),
            testdata: self.testdata(),
            path,
            _bin: bin,
            keep_work: host_var("KEEP_WORK").is_some_and(|v| v == "1"),
            profile_file: coverage_profile(),
        })
    }

    fn trials(&self) -> Result<Vec<Trial>, String> {
        let config = self.config()?;
        transcript_trials(
            &self.testdata(),
            "script",
            move |name, bytes, update| {
                let source =
                    std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
                let (outcome, updated) =
                    script::run(&config, name, source, update);
                Ok((outcome, updated.map(String::into_bytes)))
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

/// Runs one transcript file, given its name, its bytes and whether it may
/// rewrite itself. It says how the transcript ended and gives the new bytes
/// when it rewrote itself.
type RunFile = dyn Fn(&str, &[u8], bool) -> Ran + Send + Sync;

/// What running a transcript gives: how it ended and its new bytes when it
/// rewrote itself, or why it could not be run at all.
pub type Ran = Result<(Outcome, Option<Vec<u8>>), String>;

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
/// folder `wanted` accepts, named `<folder>/<case>`. A file listed in
/// `testdata/xfail` (by name or by folder) must fail; `UPDATE_SCRIPTS=1`
/// lets `run` rewrite the others.
pub fn transcript_trials(
    testdata: &Path,
    sub: &str,
    run: impl Fn(&str, &[u8], bool) -> Ran + Send + Sync + 'static,
    wanted: impl Fn(&str) -> bool,
) -> Result<Vec<Trial>, String> {
    let run: Arc<RunFile> = Arc::new(run);
    let xfail = read_xfail(&testdata.join("xfail"))?;
    let update = host_var("UPDATE_SCRIPTS").is_some_and(|v| v == "1");
    let mut trials = Vec::new();
    for group in sorted_entries(&testdata.join(sub))? {
        let Some(group_name) = file_name(&group) else {
            continue;
        };
        if !group.is_dir() || !wanted(&group_name) {
            continue;
        }
        for file in sorted_entries(&group)? {
            if file.extension().is_none_or(|e| e != "txtar") {
                continue;
            }
            let Some(case) = file.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let name = format!("{group_name}/{case}");
            let failing = xfail.iter().any(|x| *x == group_name || *x == name);
            let run = Arc::clone(&run);
            trials.push(Trial::test(name.clone(), move || {
                run_file(&*run, &name, &file, failing, update && !failing)
            }));
        }
    }
    Ok(trials)
}

fn run_file(
    run: &RunFile,
    name: &str,
    file: &Path,
    failing: bool,
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
    match (outcome, failing) {
        (Outcome::Pass, true) => Err(format!(
            "{name} passes but is listed in testdata/xfail; remove it there"
        )
        .into()),
        (Outcome::Fail(log), false) => Err(log.into()),
        (Outcome::Pass | Outcome::Skip(_), false)
        | (Outcome::Fail(_) | Outcome::Skip(_), true) => Ok(()),
    }
}

/// The entries of `testdata/xfail`: folders or `<folder>/<case>`.
pub fn read_xfail(path: &Path) -> Result<Vec<String>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

/// A folder with a link to each program under its name, put first on the
/// scripts' `PATH` so that a shell started by a script finds them too.
fn bin_folder(
    programs: &BTreeMap<String, PathBuf>,
) -> Result<tempfile::TempDir, String> {
    let bin = tempfile::Builder::new()
        .prefix("lsty-bin-")
        .tempdir()
        .map_err(|e| format!("bin folder: {e}"))?;
    for (name, path) in programs {
        let target = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        std::os::unix::fs::symlink(&target, bin.path().join(name))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
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

fn file_name(path: &Path) -> Option<String> {
    path.file_name()?.to_str().map(str::to_owned)
}

/// The host's `LLVM_PROFILE_FILE`, for programs started with a cleared
/// environment, so a coverage run counts them.
#[must_use]
#[expect(
    clippy::disallowed_methods,
    reason = "the harness, not lsty, passes the coverage profile through"
)]
pub fn coverage_profile() -> Option<OsString> {
    std::env::var_os("LLVM_PROFILE_FILE")
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
    fn a_skipped_transcript_may_be_listed_in_xfail() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txtar");
        fs::write(&file, "").unwrap();
        let skip =
            |_: &str, _: &[u8], _| Ok((Outcome::Skip("x".into()), None));
        assert!(run_file(&skip, "g/a", &file, true, false).is_ok());
        assert!(run_file(&skip, "g/a", &file, false, false).is_ok());
    }
}
