//! Process tests of `ops`. The test binary is its own helper: run with
//! `--pri-helper DIR NR` it prioritizes line NR of the list in DIR 25
//! times with alternating letters; with `--add-helper DIR NR` it adds 25
//! distinct items; with `--reader-helper DIR` it loads the list over and
//! over until `DIR/stop` exists and fails when item `k` is missing or
//! carries a warning; with `--shared-helper DIR DONE NR` it adds 50 done
//! items to its own todo file and archives them into the shared done
//! file.
#![expect(
    clippy::disallowed_methods,
    reason = "tests build paths of their own, which are UTF-8"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use libtest_mimic::{Arguments, Trial};
use lsty_core::config::Config;
use lsty_core::dates::Clock;
use lsty_core::discover::{FileState, Found, FoundBy};
use lsty_core::errs::Source;
use lsty_core::list::{self, Params};
use lsty_core::lock::Cancel;
use lsty_core::ops::{
    AddParams, ClaimOpts, DoOpts, Env, PriPair, Target, Toggle, add, archive,
    finish, parse_refs, pri, restore,
};
use lsty_core::todotxt::Priority;

const HELPER_FLAG: &str = "--pri-helper";
const ADD_FLAG: &str = "--add-helper";
const READER_FLAG: &str = "--reader-helper";
const SHARED_FLAG: &str = "--shared-helper";
const MOVES: usize = 200;
const MAX_MOVES: usize = 20_000;
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
    if args.first().map(String::as_str) == Some(READER_FLAG) {
        return reader_helper(&args[1..]);
    }
    if args.first().map(String::as_str) == Some(SHARED_FLAG) {
        return shared_helper(&args[1..]);
    }
    let trials = vec![
        Trial::test("12.29 processes do not lose lines", || Ok(processes()?)),
        Trial::test("14.42 processes add distinct items", || Ok(adders()?)),
        Trial::test("21.33 a reader during moves", || Ok(moves_and_reader()?)),
        Trial::test("21.34 archiving into one done file", || {
            Ok(shared_done_file()?)
        }),
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
        let letter = if round.is_multiple_of(2) { b'A' } else { b'B' };
        let pair = PriPair {
            target: Target::todo(nr),
            priority: Priority::new(letter),
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

fn test_clock() -> lsty_core::errs::Result<Clock> {
    let var =
        |name: &str| (name == "LSTY_NOW").then(|| "2026-09-29T0900Z".into());
    Clock::new(&var, jiff::Timestamp::UNIX_EPOCH, jiff::tz::TimeZone::UTC)
}

fn reader_helper(args: &[String]) -> ExitCode {
    let [dir] = args else {
        return ExitCode::from(2);
    };
    match read_until_stopped(Path::new(dir)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => fail(&e),
    }
}

/// Loads the list until `stop` exists. `false` when item `k` had a
/// warning, or when the reader never loaded or never saw `k` in the done
/// file, which means it did not overlap the moves.
fn read_until_stopped(dir: &Path) -> lsty_core::errs::Result<bool> {
    let files = found(dir);
    let config = Config::default();
    let params = Params::new(test_clock()?, &config);
    let cancel = Cancel::new();
    let (mut loads, mut in_done) = (0_usize, 0_usize);
    while !dir.join("stop").exists() {
        let snap = list::load(
            &files,
            std::time::Duration::from_secs(60),
            &cancel,
            &params,
        )?;
        loads += 1;
        if loads == 1 {
            fs::write(dir.join("ready"), "").map_err(|e| io_at(dir, &e))?;
        }
        let item = snap.by_id("k")?;
        if !snap.warnings_for([item]).is_empty() {
            return Ok(false);
        }
        if item.source() == Source::Done {
            in_done += 1;
            if in_done == 1 {
                fs::write(dir.join("seen"), "").map_err(|e| io_at(dir, &e))?;
            }
        }
    }
    Ok(loads > 0 && in_done > 0)
}

fn io_at(dir: &Path, err: &std::io::Error) -> lsty_core::errs::Error {
    lsty_core::errs::Error::new(
        lsty_core::errs::Code::Io,
        format!("{}: {err}", dir.display()),
    )
}

fn moves_and_reader() -> Result<(), String> {
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let dir: PathBuf =
        fs::canonicalize(tmp.path()).map_err(|e| e.to_string())?;
    fs::write(dir.join("todo.txt"), "Fix id:k\n")
        .map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut reader = Command::new(exe)
        .arg(READER_FLAG)
        .arg(&dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    let moved = move_back_and_forth(&dir);
    fs::write(dir.join("stop"), "").map_err(|e| e.to_string())?;
    let status = reader.wait().map_err(|e| e.to_string())?;
    moved.map_err(|e| e.to_string())?;
    if status.success() {
        return Ok(());
    }
    Err(format!("reader: {status}"))
}

fn move_back_and_forth(dir: &Path) -> lsty_core::errs::Result<()> {
    let files = found(dir);
    let config = Config::default();
    let clock = test_clock()?;
    let mut env = Env::new(&files, &config, &clock);
    env.timeout = std::time::Duration::from_secs(60);
    let targets = parse_refs("id:k")?;
    let opts = DoOpts {
        archive: Toggle::On,
        ..DoOpts::default()
    };
    // The reader says when it is running and when it has seen the item
    // in the done file; the moves go on until then, at least MOVES times.
    for _ in 0..10_000 {
        if dir.join("ready").exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    for round in 0..MAX_MOVES {
        finish(&env, &targets, opts)?;
        restore(&env, &targets, ClaimOpts::default())?;
        if round + 1 >= MOVES && dir.join("seen").exists() {
            break;
        }
    }
    Ok(())
}

fn shared_helper(args: &[String]) -> ExitCode {
    let [dir, done, nr] = args else {
        return ExitCode::from(2);
    };
    match archive_rounds(Path::new(dir), Path::new(done), nr) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    }
}

fn archive_rounds(
    dir: &Path,
    done: &Path,
    nr: &str,
) -> lsty_core::errs::Result<()> {
    let files = Found {
        todo: dir.join("todo.txt"),
        done: done.to_path_buf(),
        found_by: FoundBy::File,
        state: FileState::Exists,
    };
    let config = Config::default();
    let clock = test_clock()?;
    let mut env = Env::new(&files, &config, &clock);
    env.timeout = std::time::Duration::from_secs(60);
    for round in 0..50 {
        let row = format!("x 2026-09-28 {nr}-{round}\n");
        append_to(&files.todo, &row)
            .map_err(|e| lsty_core::errs::io("append", &files.todo, e))?;
        archive(&env)?;
    }
    Ok(())
}

fn append_to(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;
    file.write_all(text.as_bytes())
}

fn shared_done_file() -> Result<(), String> {
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root: PathBuf =
        fs::canonicalize(tmp.path()).map_err(|e| e.to_string())?;
    let done = root.join("done.txt");
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut children = Vec::new();
    for i in 0..2 {
        let dir = root.join(format!("p{i}"));
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let child = Command::new(&exe)
            .arg(SHARED_FLAG)
            .arg(&dir)
            .arg(&done)
            .arg(i.to_string())
            .spawn()
            .map_err(|e| e.to_string())?;
        children.push((i, child));
    }
    for (i, mut child) in children {
        let status = child.wait().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("process {i}: {status}"));
        }
    }
    let text = fs::read_to_string(&done).map_err(|e| e.to_string())?;
    let count = text.lines().count();
    if count != 100 {
        return Err(format!("{count} lines in the done file"));
    }
    Ok(())
}
