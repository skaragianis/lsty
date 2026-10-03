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
mod txtar;

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
    reference: bool,
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
            reference: false,
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

    /// When the host variable `var` names a program, makes `exec name` run
    /// it instead and expects every transcript to pass, `testdata/xfail`
    /// or not: a check that the transcripts hold for that program too.
    #[must_use]
    pub fn reference(mut self, name: impl Into<String>, var: &str) -> Self {
        if let Some(path) = host_var(var).filter(|v| !v.is_empty()) {
            self.programs.insert(name.into(), path.into());
            self.reference = true;
        }
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
        let args = Arguments::from_args();
        let trials = self.trials().unwrap_or_else(|e| {
            vec![Trial::test("transcripts", move || Err(e.into()))]
        });
        libtest_mimic::run(&args, trials).exit_code()
    }

    /// Runs one transcript given as text, without rewriting anything.
    pub fn run_source(&self, name: &str, source: &str) -> Outcome {
        match self.config(false) {
            Ok(config) => script::run(&config, name, source).0,
            Err(e) => Outcome::Fail(e),
        }
    }

    fn testdata(&self) -> PathBuf {
        self.root.join("testdata")
    }

    fn config(&self, update: bool) -> Result<Config, String> {
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
            update,
            keep_work: host_var("KEEP_WORK").is_some_and(|v| v == "1"),
            profile_file: coverage_profile(),
        })
    }

    fn trials(&self) -> Result<Vec<Trial>, String> {
        let dir = self.testdata().join("script");
        let xfail = if self.reference {
            Vec::new()
        } else {
            read_xfail(&self.testdata().join("xfail"))?
        };
        let update = host_var("UPDATE_SCRIPTS").is_some_and(|v| v == "1");
        let config = Arc::new(self.config(update)?);
        let frozen = if update {
            Arc::new(self.config(false)?)
        } else {
            Arc::clone(&config)
        };

        let mut trials = Vec::new();
        for group in sorted_entries(&dir)? {
            let Some(group_name) = self.wanted_group(&group) else {
                continue;
            };
            trials.extend(group_trials(
                &group,
                &group_name,
                &xfail,
                &config,
                &frozen,
            )?);
        }
        Ok(trials)
    }

    /// The name of `group` when it is a folder this suite runs.
    fn wanted_group(&self, group: &Path) -> Option<String> {
        let name = file_name(group)?;
        let wanted = group.is_dir()
            && (self.only.is_empty() || self.only.contains(&name))
            && !self.except.contains(&name);
        wanted.then_some(name)
    }
}

/// One trial per transcript of `group`. Transcripts listed in `xfail` run
/// with `frozen`, a config that never rewrites them; the rest with `config`.
fn group_trials(
    group: &Path,
    group_name: &str,
    xfail: &[String],
    config: &Arc<Config>,
    frozen: &Arc<Config>,
) -> Result<Vec<Trial>, String> {
    let mut trials = Vec::new();
    for file in sorted_entries(group)? {
        let Some(case) = transcript_case(&file) else {
            continue;
        };
        let name = format!("{group_name}/{case}");
        let expected_failure =
            xfail.iter().any(|x| *x == group_name || *x == name);
        let config =
            Arc::clone(if expected_failure { frozen } else { config });
        trials.push(Trial::test(name.clone(), move || {
            run_file(&config, &name, &file, expected_failure)
        }));
    }
    Ok(trials)
}

/// The case name of a `.txtar` file: its stem.
fn transcript_case(file: &Path) -> Option<&str> {
    if file.extension().is_none_or(|e| e != "txtar") {
        return None;
    }
    file.file_stem().and_then(|s| s.to_str())
}

fn run_file(
    config: &Config,
    name: &str,
    file: &Path,
    expected_failure: bool,
) -> Result<(), Failed> {
    let source = fs::read_to_string(file)
        .map_err(|e| format!("{}: {e}", file.display()))?;
    let (outcome, updated) = script::run(config, name, &source);
    if let Some(text) = updated {
        fs::write(file, text)
            .map_err(|e| format!("{}: {e}", file.display()))?;
    }
    match (outcome, expected_failure) {
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
fn read_xfail(path: &Path) -> Result<Vec<String>, String> {
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
