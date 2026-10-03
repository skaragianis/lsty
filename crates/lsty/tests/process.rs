//! Process tests of the mutation commands: real `lsty` processes at once
//! on one list.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use libtest_mimic::{Arguments, Failed, Trial};
use serde_json::Value;

const LIST: &str = "(A) Write parser +spec01 id:parser\n\
(A) Add CLI ls +spec02 id:k3f9 dep:parser\n\
Call client +work id:c1\n\
Pay rent rec:1m due:2026-09-30 id:r1\n";

type Check = Result<(), Failed>;

struct Run {
    code: i32,
    out: String,
    err: String,
}

struct Work {
    _tmp: tempfile::TempDir,
    root: PathBuf,
}

impl Work {
    fn new(todo: &str) -> Result<Self, Failed> {
        let tmp = tempfile::tempdir()?;
        let root = fs::canonicalize(tmp.path())?;
        fs::write(root.join("todo.txt"), todo)?;
        Ok(Self { _tmp: tmp, root })
    }

    fn todo(&self) -> Result<String, Failed> {
        Ok(fs::read_to_string(self.root.join("todo.txt"))?)
    }

    fn run(&self, owner: &str, args: &[&str]) -> Result<Run, Failed> {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lsty"));
        cmd.args(args)
            .env_clear()
            .env("HOME", &self.root)
            .env("TZ", "UTC")
            .env("LSTY_NOW", "2026-09-29T0900Z")
            .env("TODO_FILE", self.root.join("todo.txt"));
        if !owner.is_empty() {
            cmd.env("LSTY_OWNER", owner);
        }
        let out = cmd.output()?;
        Ok(Run {
            code: out.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&out.stdout).into_owned(),
            err: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

fn parallel<T: Send>(
    workers: usize,
    job: impl Fn(usize) -> T + Sync,
) -> Vec<T> {
    thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|i| {
                let job = &job;
                scope.spawn(move || job(i))
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

fn start_race() -> Check {
    let work = Work::new(LIST)?;
    let runs = parallel(8, |i| {
        let owner = format!("agent{}", i + 1);
        work.run(&owner, &["start", "id:c1", "--json", "--no-config"])
    });
    if runs.len() != 8 {
        return Err("a worker panicked".to_owned().into());
    }
    let mut won = 0;
    for run in runs {
        let run = run?;
        if run.code == 0 {
            won += 1;
        } else if run.code != 3 || !run.out.contains("\"code\":\"claimed\"") {
            return Err(format!("loser: {} {}", run.code, run.out).into());
        }
    }
    if won != 1 {
        return Err(format!("{won} processes started the item").into());
    }
    Ok(())
}

fn adds_are_not_lost() -> Check {
    let work = Work::new("")?;
    let runs = parallel(8, |i| {
        for n in 0..25 {
            let text = format!("worker {i} item {n}");
            let args = ["add", &text, "--human", "--no-date", "--no-config"];
            let run = work.run("", &args)?;
            if run.code != 0 {
                return Err(format!("worker {i}: {}", run.err).into());
            }
        }
        Ok::<(), Failed>(())
    });
    if runs.len() != 8 {
        return Err("a worker panicked".to_owned().into());
    }
    for run in runs {
        run?;
    }
    let todo = work.todo()?;
    let mut lines: Vec<&str> = todo.lines().collect();
    if lines.len() != 200 {
        return Err(format!("{} lines, want 200", lines.len()).into());
    }
    lines.sort_unstable();
    lines.dedup();
    if lines.len() != 200 {
        return Err(format!("{} distinct lines", lines.len()).into());
    }
    Ok(())
}

fn mover(work: &Work, done: &AtomicBool) -> Check {
    for _ in 0..30 {
        let moves: [&[&str]; 2] =
            [&["do", "id:c1", "--archive"], &["restore", "id:c1"]];
        for args in moves {
            let mut line = args.to_vec();
            line.extend(["--human", "--no-config"]);
            let run = work.run("", &line)?;
            if run.code != 0 {
                done.store(true, Ordering::SeqCst);
                return Err(format!("{line:?}: {}", run.err).into());
            }
        }
    }
    done.store(true, Ordering::SeqCst);
    Ok(())
}

fn item_once(work: &Work) -> Check {
    let run = work.run("", &["lsa", "--json", "--human", "--no-config"])?;
    if run.code != 0 {
        return Err(format!("lsa: {}", run.err).into());
    }
    let got: Value = serde_json::from_str(&run.out)?;
    let found = got["items"]
        .as_array()
        .map_or(0, |items| items.iter().filter(|i| i["id"] == "c1").count());
    if found != 1 {
        return Err(format!("lsa lists c1 {found} times").into());
    }
    let show =
        work.run("", &["show", "id:c1", "--json", "--human", "--no-config"])?;
    if show.code != 0 {
        return Err(format!("show: {}", show.out).into());
    }
    Ok(())
}

fn reader_sees_item_once() -> Check {
    let work = Work::new(LIST)?;
    let done = AtomicBool::new(false);
    thread::scope(|scope| {
        let mover_job = scope.spawn(|| mover(&work, &done));
        let mut seen = item_once(&work);
        while !done.load(Ordering::SeqCst) && seen.is_ok() {
            seen = item_once(&work);
        }
        let moved = mover_job
            .join()
            .unwrap_or_else(|_| Err("the mover panicked".to_owned().into()));
        seen.and(moved)
    })
}

fn lock_timeout() -> Check {
    let work = Work::new(LIST)?;
    let before = work.todo()?;
    let folder = File::open(Path::new(&work.root))?;
    folder.lock()?;
    let run = work.run("agent1", &["do", "id:c1", "--json", "--no-config"])?;
    folder.unlock()?;
    if run.code != 4 || !run.out.contains("\"code\":\"lock_timeout\"") {
        return Err(format!("exit {}, {}", run.code, run.out).into());
    }
    if work.todo()? != before {
        return Err("the file changed".to_owned().into());
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let trials = vec![
        Trial::test("25.42 eight agents start one item", start_race),
        Trial::test("25.43 eight processes add", adds_are_not_lost),
        Trial::test(
            "25.44 a reader sees the item once",
            reader_sees_item_once,
        ),
        Trial::test("25.45 lock timeout", lock_timeout),
    ];
    libtest_mimic::run(&Arguments::from_args(), trials).exit_code()
}
