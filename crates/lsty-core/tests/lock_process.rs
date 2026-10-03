//! Process tests of `lock`. The test binary is its own lock helper: run
//! with `--lock-helper` it takes the locks named on its command line,
//! prints `locked`, holds them and exits.

use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use libtest_mimic::{Arguments, Trial};
use lsty_core::errs::{self, EXIT_LOCK_TIMEOUT};
use lsty_core::fsio;
use lsty_core::lock::{self, Cancel, Flock, Mode, Want};
use rustix::io::Errno;

type Check = Result<(), String>;
type Test = (&'static str, fn() -> Check);

const HELPER_FLAG: &str = "--lock-helper";
const SLACK: Duration = Duration::from_millis(150);

const TESTS: &[Test] = &[
    ("05.1 exclusive against exclusive", t_exclusive_exclusive),
    ("05.2 released in time", t_released_in_time),
    ("05.3 shared with shared", t_shared_shared),
    (
        "05.4 exclusive against shared holder",
        t_exclusive_vs_shared,
    ),
    (
        "05.5 shared against exclusive holder",
        t_shared_vs_exclusive,
    ),
    ("05.6 other file in the same folder", t_same_folder),
    ("05.7 other folder", t_other_folder),
    ("05.8 killed holder", t_killed_holder),
    ("05.9 opposed order", t_opposed_order),
    (
        "05.12 nothing held after a timeout",
        t_nothing_held_after_timeout,
    ),
    ("05.17 sidecar processes", t_sidecar_processes),
    ("05.20 counter", t_counter),
];

fn main() -> ExitCode {
    let args = process_args();
    if args.first().map(String::as_str) == Some(HELPER_FLAG) {
        return helper(&args[1..]);
    }
    run_tests()
}

#[expect(clippy::disallowed_methods, reason = "the helper reads its own argv")]
fn process_args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

/// Runs the cases one at a time unless `--test-threads` says otherwise:
/// they race processes against windows of a few hundred milliseconds.
fn run_tests() -> ExitCode {
    let mut arguments = Arguments::from_args();
    arguments.test_threads.get_or_insert(1);
    let trials = TESTS
        .iter()
        .map(|(name, test)| Trial::test(*name, move || Ok(test()?)))
        .collect();
    libtest_mimic::run(&arguments, trials).exit_code()
}

// The helper process.

struct Refuse;

impl Flock for Refuse {
    fn try_lock(&self, file: &fs::File, mode: Mode) -> io::Result<()> {
        if file.metadata()?.is_dir() {
            let raw = Errno::NOTSUP.raw_os_error();
            return Err(io::Error::from_raw_os_error(raw));
        }
        lock::Os.try_lock(file, mode)
    }
}

#[derive(Default)]
struct Options {
    timeout: Duration,
    hold: Duration,
    stdin: bool,
    fake: bool,
    rounds: u32,
    counter: Option<PathBuf>,
    wants: Vec<Want>,
}

fn parse(args: &[String]) -> Options {
    let mut opts = Options {
        timeout: Duration::from_millis(200),
        rounds: 1,
        ..Options::default()
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().unwrap_or_default();
        match arg.as_str() {
            "-timeout" => opts.timeout = millis(&value()),
            "-hold" => opts.hold = millis(&value()),
            "-rounds" => opts.rounds = value().parse().unwrap_or(1),
            "-counter" => opts.counter = Some(PathBuf::from(value())),
            "-stdin" => opts.stdin = true,
            "-fake" => opts.fake = true,
            other => opts.wants.push(parse_want(other)),
        }
    }
    opts
}

fn millis(text: &str) -> Duration {
    Duration::from_millis(text.parse().unwrap_or(0))
}

fn parse_want(arg: &str) -> Want {
    let (mode, file) = arg.split_once(':').unwrap_or(("s", arg));
    Want {
        file: PathBuf::from(file),
        mode: if mode == "x" {
            Mode::Exclusive
        } else {
            Mode::Shared
        },
    }
}

fn helper(args: &[String]) -> ExitCode {
    let opts = parse(args);
    for _ in 0..opts.rounds {
        match one_round(&opts) {
            Ok(()) => {}
            Err(code) => return ExitCode::from(code),
        }
    }
    ExitCode::SUCCESS
}

fn one_round(opts: &Options) -> Result<(), u8> {
    let cancel = Cancel::new();
    let held = if opts.fake {
        lock::acquire_with(&Refuse, &opts.wants, opts.timeout, &cancel)
    } else {
        lock::acquire(&opts.wants, opts.timeout, &cancel)
    };
    let mut held = held.map_err(|e| e.exit())?;
    say("locked").map_err(|_| 1_u8)?;
    if let Some(counter) = &opts.counter {
        bump(counter).map_err(|_| 1_u8)?;
    }
    if opts.stdin {
        let mut line = String::new();
        drop(io::stdin().lock().read_line(&mut line));
    }
    thread::sleep(opts.hold);
    held.release().map_err(|_| 1_u8)
}

fn say(text: &str) -> io::Result<()> {
    let mut out = io::stdout();
    writeln!(out, "{text}")?;
    out.flush()
}

fn bump(path: &Path) -> errs::Result<()> {
    let data = fsio::read(path)?.unwrap_or_default();
    let n: u64 = String::from_utf8_lossy(&data).trim().parse().unwrap_or(0);
    fsio::write(path, format!("{}\n", n + 1).as_bytes(), &fsio::Os)
}

// The test side.

struct Holder {
    child: Child,
}

impl Drop for Holder {
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

fn exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

fn command(args: &[String]) -> Result<Command, String> {
    let mut cmd = Command::new(exe()?);
    cmd.arg(HELPER_FLAG).args(args);
    Ok(cmd)
}

fn start_holder(args: &[String]) -> Result<Holder, String> {
    let mut all = vec!["-stdin".to_string()];
    all.extend_from_slice(args);
    let mut child = command(&all)?
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut holder = Holder { child };
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if line != "locked\n" {
        return Err(format!("holder said {line:?}"));
    }
    drop(holder.child.stdout.take());
    Ok(holder)
}

impl Holder {
    fn release(&mut self) -> Check {
        let stdin = self.child.stdin.as_mut().ok_or("no stdin")?;
        stdin.write_all(b"\n").map_err(|e| e.to_string())
    }
}

struct Outcome {
    out: String,
    code: i32,
    elapsed: Duration,
}

impl Outcome {
    fn locked(&self) -> bool {
        self.code == 0 && self.out.starts_with("locked\n")
    }

    fn describe(&self) -> String {
        format!("{:?}, exit {}, {:?}", self.out, self.code, self.elapsed)
    }
}

fn run_helper(args: &[String]) -> Result<Outcome, String> {
    let start = Instant::now();
    let output = command(args)?
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    Ok(Outcome {
        out: String::from_utf8_lossy(&output.stdout).into_owned(),
        code: output.status.code().unwrap_or(-1),
        elapsed: start.elapsed(),
    })
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn folders(names: &[&str]) -> Result<(tempfile::TempDir, PathBuf), String> {
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = fs::canonicalize(tmp.path()).map_err(|e| e.to_string())?;
    for name in names {
        fs::create_dir(root.join(name)).map_err(|e| e.to_string())?;
    }
    Ok((tmp, root))
}

fn want_arg(mode: &str, root: &Path, rel: &str) -> String {
    format!("{mode}:{}", root.join(rel).display())
}

fn expect(ok: bool, what: &str, got: &Outcome) -> Check {
    if ok {
        Ok(())
    } else {
        Err(format!("{what}: got {}", got.describe()))
    }
}

fn times_out(got: &Outcome) -> Check {
    let waited = got.elapsed >= Duration::from_millis(180)
        && got.elapsed <= Duration::from_millis(200) + SLACK * 3;
    expect(
        i32::from(EXIT_LOCK_TIMEOUT) == got.code && waited,
        "want lock_timeout after about 200ms",
        got,
    )
}

fn at_once(got: &Outcome) -> Check {
    let quick = got.elapsed <= Duration::from_millis(200) + SLACK;
    expect(got.locked() && quick, "want locked at once", got)
}

/// A holder asks `hold` on `hold_on`, then an asker asks `ask` on
/// `ask_on`.
fn contend(hold: &str, hold_on: &str, ask: &str, ask_on: &str) -> Not {
    let (_tmp, root) = folders(&["d", "d1", "d2"])?;
    let _holder = start_holder(&[want_arg(hold, &root, hold_on)])?;
    run_helper(&[want_arg(ask, &root, ask_on)])
}

type Not = Result<Outcome, String>;

fn t_exclusive_exclusive() -> Check {
    times_out(&contend("x", "d/todo.txt", "x", "d/todo.txt")?)
}

fn t_shared_shared() -> Check {
    at_once(&contend("s", "d/todo.txt", "s", "d/todo.txt")?)
}

fn t_exclusive_vs_shared() -> Check {
    times_out(&contend("s", "d/todo.txt", "x", "d/todo.txt")?)
}

fn t_shared_vs_exclusive() -> Check {
    times_out(&contend("x", "d/todo.txt", "s", "d/todo.txt")?)
}

fn t_same_folder() -> Check {
    times_out(&contend("x", "d/todo.txt", "x", "d/done.txt")?)
}

fn t_other_folder() -> Check {
    at_once(&contend("x", "d1/todo.txt", "x", "d2/todo.txt")?)
}

fn t_released_in_time() -> Check {
    let (_tmp, root) = folders(&["d"])?;
    let want = want_arg("x", &root, "d/todo.txt");
    let mut holder = start_holder(&args(&["-hold", "50", &want]))?;
    holder.release()?;
    let got = run_helper(&[want])?;
    expect(got.locked(), "want locked", &got)
}

fn t_killed_holder() -> Check {
    let (_tmp, root) = folders(&["d"])?;
    let want = want_arg("x", &root, "d/todo.txt");
    let mut holder = start_holder(std::slice::from_ref(&want))?;
    holder.child.kill().map_err(|e| e.to_string())?;
    holder.child.wait().map_err(|e| e.to_string())?;
    let got = run_helper(&[want])?;
    expect(got.locked(), "want locked", &got)
}

fn spawn_helper(args: Vec<String>) -> thread::JoinHandle<Not> {
    thread::spawn(move || run_helper(&args))
}

fn join(handle: thread::JoinHandle<Not>) -> Not {
    handle
        .join()
        .map_err(|_| "helper thread panicked".to_string())?
}

fn t_opposed_order() -> Check {
    let (_tmp, root) = folders(&["x", "y"])?;
    let base = args(&["-rounds", "50", "-timeout", "2000"]);
    let mut first = base.clone();
    first.push(want_arg("x", &root, "x/todo.txt"));
    first.push(want_arg("x", &root, "y/done.txt"));
    let mut second = base;
    second.push(want_arg("x", &root, "y/todo.txt"));
    second.push(want_arg("x", &root, "x/done.txt"));
    let (a, b) = (spawn_helper(first), spawn_helper(second));
    for got in [join(a)?, join(b)?] {
        let fifty = got.out.matches("locked").count() == 50;
        expect(got.code == 0 && fifty, "want 50 locks", &got)?;
    }
    Ok(())
}

fn t_nothing_held_after_timeout() -> Check {
    let (_tmp, root) = folders(&["a", "b"])?;
    let _holder = start_holder(&[want_arg("x", &root, "b/todo.txt")])?;
    let got = run_helper(&[
        want_arg("x", &root, "a/todo.txt"),
        want_arg("x", &root, "b/todo.txt"),
    ])?;
    expect(
        i32::from(EXIT_LOCK_TIMEOUT) == got.code,
        "want timeout",
        &got,
    )?;
    let again = run_helper(&[want_arg("x", &root, "a/todo.txt")])?;
    at_once(&again)
}

fn t_sidecar_processes() -> Check {
    let (_tmp, root) = folders(&["d"])?;
    let want = want_arg("x", &root, "d/todo.txt");
    let _holder = start_holder(&args(&["-fake", &want]))?;
    let got = run_helper(&args(&["-fake", &want]))?;
    times_out(&got)?;
    let sidecar = root.join("d/.todo.txt.lock");
    expect(sidecar.exists(), "want the sidecar", &got)?;
    let plain = run_helper(&[want])?;
    expect(plain.locked(), "folder lock is separate", &plain)
}

fn t_counter() -> Check {
    let (_tmp, root) = folders(&["d"])?;
    let counter = root.join("d/counter.txt");
    let mut call = args(&["-rounds", "50", "-timeout", "30000", "-counter"]);
    call.push(counter.display().to_string());
    call.push(want_arg("x", &root, "d/todo.txt"));
    let runs: Vec<_> = (0..8).map(|_| spawn_helper(call.clone())).collect();
    for run in runs {
        let got = join(run)?;
        expect(got.code == 0, "want success", &got)?;
    }
    let mut text = String::new();
    fs::File::open(&counter)
        .and_then(|mut f| f.read_to_string(&mut text))
        .map_err(|e| e.to_string())?;
    if text.trim() == "400" {
        Ok(())
    } else {
        Err(format!("counter is {:?}, want 400", text.trim()))
    }
}
