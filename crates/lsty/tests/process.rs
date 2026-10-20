//! Process tests of the mutation commands: real `lsty` processes at once
//! on one list.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use libtest_mimic::{Arguments, Failed, Trial};
use rustix::process::{Pid, Signal, kill_process};
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
        // Each write syncs to disk under the lock, which is slow on macOS,
        // so eight busy processes can keep one out past the default 2 s.
        let config = root.join(".config").join("lsty");
        fs::create_dir_all(&config)?;
        fs::write(config.join("config.toml"), "lock_timeout = \"30s\"\n")?;
        Ok(Self { _tmp: tmp, root })
    }

    fn todo(&self) -> Result<String, Failed> {
        Ok(fs::read_to_string(self.root.join("todo.txt"))?)
    }

    fn run(&self, owner: &str, args: &[&str]) -> Result<Run, Failed> {
        let out = self.command(owner, args).output()?;
        Ok(Run {
            code: out.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&out.stdout).into_owned(),
            err: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    fn command(&self, owner: &str, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lsty"));
        cmd.args(args)
            .env_clear()
            .env("HOME", &self.root)
            .env("TZ", "UTC")
            .env("TODOTXT_GLOBAL_CFG_FILE", self.root.join("none"))
            .env("LSTY_NOW", "2026-09-29T0900Z")
            .env("TODO_FILE", self.root.join("todo.txt"));
        if !owner.is_empty() {
            cmd.env("LSTY_OWNER", owner);
        }
        lsty_scripttest::with_coverage(&mut cmd);
        cmd
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
        work.run(&owner, &["start", "id:c1", "--json"])
    });
    if runs.len() != 8 {
        return Err("a worker panicked".into());
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
        return Err(format!("{won} processes started the task").into());
    }
    Ok(())
}

fn ready_list(n: usize) -> String {
    (1..=n)
        .map(|i| ["Task ", &i.to_string(), "\n"].concat())
        .collect()
}

/// Eight processes run `next --start` on a list of `tasks` ready tasks,
/// named by `name`.
fn next_race(
    tasks: usize,
    name: impl Fn(usize) -> String + Sync,
) -> Result<(Work, Vec<Run>), Failed> {
    let work = Work::new(&ready_list(tasks))?;
    let runs =
        parallel(8, |i| work.run(&name(i), &["next", "--start", "--json"]));
    if runs.len() != 8 {
        return Err("a worker panicked".into());
    }
    let mut done = Vec::new();
    for (i, run) in runs.into_iter().enumerate() {
        let run = run?;
        if run.code != 0 {
            return Err(format!("worker {i}: {} {}", run.code, run.out).into());
        }
        done.push(run);
    }
    Ok((work, done))
}

/// How many tasks each owner holds.
fn claims(todo: &str) -> BTreeMap<String, usize> {
    let mut owners = BTreeMap::new();
    for line in todo.lines() {
        if let Some((_, owner)) = line.split_once(" owner:") {
            *owners.entry(owner.to_owned()).or_insert(0) += 1;
        }
    }
    owners
}

fn next_claims_eight() -> Check {
    let (work, _) = next_race(8, |i| format!("agent{}", i + 1))?;
    let owners = claims(&work.todo()?);
    if owners.len() != 8 || owners.values().any(|n| *n != 1) {
        return Err(format!("claims: {owners:?}").into());
    }
    Ok(())
}

fn next_runs_out() -> Check {
    let (work, runs) = next_race(5, |i| format!("agent{}", i + 1))?;
    let owners = claims(&work.todo()?);
    if owners.len() != 5 {
        return Err(format!("{} claims: {owners:?}", owners.len()).into());
    }
    let empty = runs
        .iter()
        .filter(|r| r.out.contains("\"tasks\":[]"))
        .count();
    if empty != 3 {
        return Err(format!("{empty} found nothing ready, want 3").into());
    }
    Ok(())
}

fn next_same_agent() -> Check {
    let (work, _) = next_race(8, |_| "agent1".to_owned())?;
    let owners = claims(&work.todo()?);
    if owners.get("agent1") != Some(&1) || owners.len() != 1 {
        return Err(format!("claims: {owners:?}").into());
    }
    Ok(())
}

fn adds_are_not_lost() -> Check {
    let work = Work::new("")?;
    let runs = parallel(8, |i| {
        for n in 0..25 {
            let text = format!("worker {i} task {n}");
            let args = ["add", &text, "--human", "--no-date"];
            let run = work.run("", &args)?;
            if run.code != 0 {
                return Err(format!("worker {i}: {}", run.err).into());
            }
        }
        Ok::<(), Failed>(())
    });
    if runs.len() != 8 {
        return Err("a worker panicked".into());
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
            line.push("--human");
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

fn task_once(work: &Work) -> Check {
    let run = work.run("", &["lsa", "--json", "--human"])?;
    if run.code != 0 {
        return Err(format!("lsa: {}", run.err).into());
    }
    let got: Value = serde_json::from_str(&run.out)?;
    let found = got["tasks"]
        .as_array()
        .map_or(0, |tasks| tasks.iter().filter(|i| i["id"] == "c1").count());
    if found != 1 {
        return Err(format!("lsa lists c1 {found} times").into());
    }
    let show = work.run("", &["show", "id:c1", "--json", "--human"])?;
    if show.code != 0 {
        return Err(format!("show: {}", show.out).into());
    }
    Ok(())
}

fn reader_sees_task_once() -> Check {
    let work = Work::new(LIST)?;
    let done = AtomicBool::new(false);
    thread::scope(|scope| {
        let mover_job = scope.spawn(|| mover(&work, &done));
        let mut seen = task_once(&work);
        while !done.load(Ordering::SeqCst) && seen.is_ok() {
            seen = task_once(&work);
        }
        let moved = mover_job
            .join()
            .unwrap_or_else(|_| Err("the mover panicked".into()));
        seen.and(moved)
    })
}

fn lock_timeout() -> Check {
    let work = Work::new(LIST)?;
    let before = work.todo()?;
    let folder = File::open(Path::new(&work.root))?;
    folder.lock()?;
    // --no-config keeps the default 2 s wait, not Work's 30 s.
    let run = work.run("agent1", &["do", "id:c1", "--json", "--no-config"])?;
    if run.code != 4 || !run.out.contains("\"code\":\"lock_timeout\"") {
        return Err(format!("exit {}, {}", run.code, run.out).into());
    }
    if work.todo()? != before {
        return Err("the file changed".into());
    }
    Ok(())
}

/// How long a wait test may take before it fails.
const LIMIT: Duration = Duration::from_secs(30);

/// Whether the process holds an inotify instance: on Linux, a wait
/// that has taken its snapshot and watches the files.
fn watching(pid: u32) -> bool {
    let fds = fs::read_dir(format!("/proc/{pid}/fd"));
    fds.into_iter().flatten().flatten().any(|fd| {
        fs::read_link(fd.path())
            .is_ok_and(|to| to.as_os_str() == "anon_inode:inotify")
    })
}

/// Starts `lsty wait id:c1` with `args`. On Linux it returns once the
/// wait watches the files; elsewhere at once.
fn start_wait(work: &Work, args: &[&str]) -> Result<Child, Failed> {
    let mut child = work
        .command("", &[&["wait", "id:c1"], args].concat())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let end = Instant::now() + LIMIT;
    while cfg!(target_os = "linux") && !watching(child.id()) {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() > end {
            child.kill()?;
            child.wait()?;
            return Err("the wait never watched the files".into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    Ok(child)
}

/// Starts a wait on `list` with `timeout`, then runs `steps` in turn,
/// again and again, until the wait ends: each round leaves the line as it
/// found it. Gives the wait's exit code and what it printed.
fn wait_through(
    list: &str,
    steps: &[&[&str]],
    timeout: &str,
) -> Result<Run, Failed> {
    let work = Work::new(list)?;
    let mut child = start_wait(&work, &["--timeout", timeout])?;
    let end = Instant::now() + LIMIT;
    while Instant::now() < end {
        for step in steps {
            if child.try_wait()?.is_some() {
                let out = child.wait_with_output()?;
                return Ok(Run {
                    code: out.status.code().unwrap_or(-1),
                    out: String::from_utf8_lossy(&out.stdout).into_owned(),
                    err: String::from_utf8_lossy(&out.stderr).into_owned(),
                });
            }
            let run = work.run("", &[step, &["--human"][..]].concat())?;
            if run.code != 0 {
                child.kill()?;
                child.wait()?;
                return Err(format!("{step:?}: {}", run.err).into());
            }
        }
    }
    child.kill()?;
    child.wait()?;
    Err("the wait did not end".into())
}

/// A wait that ends at a change of the line.
fn wait_ends(list: &str, steps: &[&[&str]]) -> Check {
    let run = wait_through(list, steps, "0")?;
    if run.code != 0 || run.out != "hint: run \"lsty show id:c1\"\n" {
        return Err(
            format!("exit {}: {}{}", run.code, run.out, run.err).into()
        );
    }
    Ok(())
}

const WAITED: &str = "Pay rent id:r1\nCall client +work id:c1\n";

fn wait_sees_an_edit() -> Check {
    wait_ends(WAITED, &[&["pri", "id:c1", "A"], &["depri", "id:c1"]])
}

fn wait_sees_a_do() -> Check {
    let steps: [&[&str]; 2] =
        [&["do", "id:c1", "--no-archive"], &["reopen", "id:c1"]];
    wait_ends(WAITED, &steps)
}

fn wait_sees_an_archive() -> Check {
    let list = "Pay rent id:r1\nx 2026-09-29 Call client +work id:c1\n";
    let steps: [&[&str]; 3] = [
        &["archive"],
        &["restore", "id:c1"],
        &["do", "id:c1", "--no-archive"],
    ];
    wait_ends(list, &steps)
}

fn wait_sees_a_delete() -> Check {
    let steps: [&[&str]; 2] = [
        &["del", "id:c1"],
        &["add", "Call client +work id:c1", "--no-date"],
    ];
    wait_ends(WAITED, &steps)
}

/// Lines added above the task move it, which is no change.
fn wait_ignores_a_new_line_number() -> Check {
    let steps: [&[&str]; 1] = [&["add", "New", "--top", "--no-date"]];
    let run = wait_through(WAITED, &steps, "1s")?;
    if run.code != 5 || !run.out.is_empty() {
        return Err(
            format!("exit {}: {}{}", run.code, run.out, run.err).into()
        );
    }
    Ok(())
}

/// Ctrl-C ends a wait as the signal does (a shell reports 130), so a
/// script stops there, and nothing is printed.
fn wait_ends_on_interrupt() -> Check {
    let work = Work::new(WAITED)?;
    let child = start_wait(&work, &["--timeout", "30s"])?;
    kill_process(Pid::from_child(&child), Signal::INT)?;
    let out = child.wait_with_output()?;
    if out.status.signal() != Some(Signal::INT.as_raw())
        || !out.stdout.is_empty()
        || !out.stderr.is_empty()
    {
        return Err(format!("{:?}: {out:?}", out.status).into());
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let trials = vec![
        Trial::test("25.42 eight agents start one task", start_race),
        Trial::test("26.27 eight agents claim eight", next_claims_eight),
        Trial::test("26.28 five tasks for eight agents", next_runs_out),
        Trial::test("26.29 one agent eight times", next_same_agent),
        Trial::test("25.43 eight processes add", adds_are_not_lost),
        Trial::test(
            "25.44 a reader sees the task once",
            reader_sees_task_once,
        ),
        Trial::test("25.45 lock timeout", lock_timeout),
        Trial::test("wait sees an edit", wait_sees_an_edit),
        Trial::test("wait sees a do", wait_sees_a_do),
        Trial::test("wait sees an archive", wait_sees_an_archive),
        // Elsewhere the first delete may come before the wait reads.
        Trial::test("wait sees a delete", wait_sees_a_delete)
            .with_ignored_flag(cfg!(not(target_os = "linux"))),
        Trial::test(
            "wait ignores a new line number",
            wait_ignores_a_new_line_number,
        ),
        Trial::test("wait ends on Ctrl-C", wait_ends_on_interrupt),
    ];
    libtest_mimic::run(&Arguments::from_args(), trials).exit_code()
}
