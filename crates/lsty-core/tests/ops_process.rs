//! Process tests of `ops`. The test binary is its own helper: run with
//! `--pri-helper DIR NR` it prioritizes line NR of the list in DIR 25
//! times with alternating letters; with `--add-helper DIR NR` it adds 25
//! distinct items.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use libtest_mimic::{Arguments, Trial};
use lsty_core::config::Config;
use lsty_core::dates::Clock;
use lsty_core::discover::{FileState, Found, FoundBy};
use lsty_core::ops::{AddParams, Env, PriPair, Target, add, pri};

const HELPER_FLAG: &str = "--pri-helper";
const ADD_FLAG: &str = "--add-helper";
const PROCESSES: usize = 8;
const ROUNDS: usize = 25;

fn main() -> ExitCode {
    let args = process_args();
    if args.first().map(String::as_str) == Some(HELPER_FLAG) {
        return helper(&args[1..]);
    }
    if args.first().map(String::as_str) == Some(ADD_FLAG) {
        return add_helper(&args[1..]);
    }
    let trials = vec![
        Trial::test("12.29 processes do not lose lines", || Ok(processes()?)),
        Trial::test("14.42 processes add distinct items", || Ok(adders()?)),
    ];
    libtest_mimic::run(&Arguments::from_args(), trials).exit_code()
}

#[expect(clippy::disallowed_methods, reason = "the helper reads its own argv")]
fn process_args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

fn found(dir: &Path) -> Found {
    Found {
        todo: dir.join("todo.txt"),
        done: dir.join("done.txt"),
        found_by: FoundBy::File,
        state: FileState::Exists,
    }
}

fn helper(args: &[String]) -> ExitCode {
    let [dir, nr] = args else {
        return ExitCode::from(2);
    };
    let Ok(nr) = nr.parse::<i64>() else {
        return ExitCode::from(2);
    };
    match alternate(Path::new(dir), nr) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    }
}

#[expect(clippy::print_stderr, reason = "the helper says why it failed")]
fn fail(err: &lsty_core::errs::Error) -> ExitCode {
    eprintln!("{err}");
    ExitCode::from(1)
}

fn alternate(dir: &Path, nr: i64) -> lsty_core::errs::Result<()> {
    let files = found(dir);
    let config = Config::default();
    let var =
        |name: &str| (name == "LSTY_NOW").then(|| "2026-09-29T0900Z".into());
    let clock = Clock::new(
        &var,
        jiff::Timestamp::UNIX_EPOCH,
        jiff::tz::TimeZone::UTC,
    )?;
    let mut env = Env::new(&files, &config, &clock);
    env.timeout = std::time::Duration::from_secs(60);
    for round in 0..ROUNDS {
        let letter = if round.is_multiple_of(2) { "A" } else { "B" };
        let pair = PriPair {
            target: Target::todo(nr),
            letter: letter.to_owned(),
        };
        pri(&env, &[pair])?;
    }
    Ok(())
}

fn processes() -> Result<(), String> {
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let dir: PathBuf =
        fs::canonicalize(tmp.path()).map_err(|e| e.to_string())?;
    let lines: Vec<String> =
        (1..=PROCESSES).map(|i| format!("item{i}")).collect();
    fs::write(dir.join("todo.txt"), lines.join("\n") + "\n")
        .map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut children = Vec::new();
    for nr in 1..=PROCESSES {
        let child = Command::new(&exe)
            .arg(HELPER_FLAG)
            .arg(&dir)
            .arg(nr.to_string())
            .spawn()
            .map_err(|e| e.to_string())?;
        children.push((nr, child));
    }
    for (nr, mut child) in children {
        let status = child.wait().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("process {nr}: {status}"));
        }
    }
    let text =
        fs::read_to_string(dir.join("todo.txt")).map_err(|e| e.to_string())?;
    let got: Vec<&str> = text.lines().collect();
    let last = if (ROUNDS - 1).is_multiple_of(2) {
        "A"
    } else {
        "B"
    };
    let want: Vec<String> = (1..=PROCESSES)
        .map(|i| format!("({last}) item{i}"))
        .collect();
    if got != want {
        return Err(format!("file is {got:?}, want {want:?}"));
    }
    Ok(())
}

fn add_helper(args: &[String]) -> ExitCode {
    let [dir, nr] = args else {
        return ExitCode::from(2);
    };
    match add_items(Path::new(dir), nr) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    }
}

fn add_items(dir: &Path, nr: &str) -> lsty_core::errs::Result<()> {
    let files = found(dir);
    let config = Config::default();
    let var =
        |name: &str| (name == "LSTY_NOW").then(|| "2026-09-29T0900Z".into());
    let clock = Clock::new(
        &var,
        jiff::Timestamp::UNIX_EPOCH,
        jiff::tz::TimeZone::UTC,
    )?;
    let mut env = Env::new(&files, &config, &clock);
    env.timeout = std::time::Duration::from_secs(60);
    for round in 1..=ROUNDS {
        let params = AddParams {
            texts: vec![format!("item {nr}.{round}")],
            ..AddParams::default()
        };
        add(&env, &params)?;
    }
    Ok(())
}

fn adders() -> Result<(), String> {
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let dir: PathBuf =
        fs::canonicalize(tmp.path()).map_err(|e| e.to_string())?;
    fs::write(dir.join("todo.txt"), "").map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut children = Vec::new();
    for nr in 1..=PROCESSES {
        let child = Command::new(&exe)
            .arg(ADD_FLAG)
            .arg(&dir)
            .arg(nr.to_string())
            .spawn()
            .map_err(|e| e.to_string())?;
        children.push((nr, child));
    }
    for (nr, mut child) in children {
        let status = child.wait().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("process {nr}: {status}"));
        }
    }
    let text =
        fs::read_to_string(dir.join("todo.txt")).map_err(|e| e.to_string())?;
    let mut got: Vec<&str> = text.lines().collect();
    got.sort_unstable();
    got.dedup();
    if got.len() != PROCESSES * ROUNDS || text.lines().count() != got.len() {
        return Err(format!(
            "{} lines, {} distinct",
            text.lines().count(),
            got.len()
        ));
    }
    Ok(())
}
