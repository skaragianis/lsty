//! End-to-end times of every command `lsty` shares with todo.sh, from the
//! start of the process to its exit, measured with hyperfine on a todo
//! file and a done file of random lines (the generator `todo_sh` uses,
//! fixed seed). Files are restored before every run and `lsty` runs in a
//! scrubbed environment, so the numbers do not depend on the shell.
//!
//! The timing runs only when `LSTY_BENCH` is set (`just bench LINES SAVE
//! RUNS` sets it). `LSTY_BENCH_LINES` is the length of both files (default
//! 10000), `LSTY_BENCH_RUNS` the timed runs of each command (default 40),
//! `LSTY_BENCH_SAVE` a name: the results are then kept in
//! `bench/NAME.json` in the test scratch folder.
//! `LSTY_BENCH_COMPARE="BASE NEW"` (`just bench-compare BASE NEW`)
//! prints, for two saved results, the median of each command and the
//! change.
//!
//! The todo file holds about 7% done items, so `archive` has something to
//! move; a fifth of the items carry an `id:` and one in twenty a `dep:`.
//! Commands act on items in the middle of the file.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use libtest_mimic::{Arguments, Failed, Trial};
use serde_json::{Value, json};

#[expect(dead_code, reason = "shared with todo_sh, which uses the rest")]
#[path = "todo_sh/random.rs"]
mod random;

use random::{
    Rng, SYSTEM_PATH, file_line, host_var, number, percentile, quote,
};

/// The seed of the files.
const SEED: u64 = 1;
/// The day `lsty` believes it is.
const NOW: &str = "2026-10-07T1200Z";
/// Runs before the timed ones, to warm the caches.
const WARMUP: usize = 5;

/// A command's name and its arguments.
type Named = (&'static str, Vec<String>);

/// The lines of the two starting files.
struct Fixture {
    todo: Vec<String>,
    done: Vec<String>,
}

/// A line that is neither blank nor done.
fn is_open(line: &str) -> bool {
    !line.trim().is_empty() && !line.starts_with("x ")
}

/// Lines like `just diff` starts from, with about 7% done items in the
/// todo file and ids and dependencies on some.
fn fixture(lines: usize) -> Fixture {
    let mut rng = Rng(SEED);
    let mut todo = Vec::with_capacity(lines);
    let mut ids = Vec::new();
    while todo.len() < lines {
        let mut line = file_line(&mut rng);
        if line.starts_with("x ") && !rng.one_in(5) {
            continue;
        }
        if is_open(&line) && !line.starts_with(' ') {
            if rng.one_in(5) {
                let id = format!("b{}", ids.len());
                line = format!("{line} id:{id}");
                ids.push(id);
            } else if rng.one_in(20)
                && let Some(id) = ids.get(rng.below(ids.len()))
            {
                line = format!("{line} dep:{id}");
            }
        }
        todo.push(line);
    }
    let done = (0..lines)
        .map(|_| {
            let line = file_line(&mut rng);
            if line.starts_with("x ") {
                line
            } else {
                format!("x 2026-01-15 {}", line.trim())
            }
        })
        .collect();
    Fixture { todo, done }
}

impl Fixture {
    fn text(lines: &[String]) -> String {
        lines.iter().fold(String::new(), |mut out, l| {
            out.push_str(l);
            out.push('\n');
            out
        })
    }

    /// The number of the first open line from the middle on that `pick`
    /// accepts, and what it picked.
    fn middle<'s, T>(
        &'s self,
        pick: impl Fn(&'s str) -> Option<T>,
    ) -> Option<(usize, T)> {
        let mid = self.todo.len() / 2;
        self.todo
            .iter()
            .enumerate()
            .skip(mid)
            .filter(|(_, l)| is_open(l))
            .find_map(|(i, l)| pick(l).map(|t| (i + 1, t)))
    }

    /// Every command: its name and arguments, naming real items.
    fn commands(&self) -> Result<Vec<Named>, String> {
        let plain = |w: &str| {
            w.len() >= 3 && w.chars().all(|c| c.is_ascii_alphabetic())
        };
        let (nr, word) = self
            .middle(|l| {
                l.split(' ')
                    .find(|w| plain(w))
                    .filter(|_| !l.contains("dep:"))
            })
            .ok_or("no unblocked item with a plain word")?;
        let (pri, ()) = self
            .middle(|l| {
                let b = l.as_bytes();
                (b.first() == Some(&b'(')
                    && b.get(1).is_some_and(u8::is_ascii_uppercase)
                    && b.get(2) == Some(&b')'))
                .then_some(())
            })
            .ok_or("no prioritised item")?;
        let n = nr.to_string();
        let args = |list: &[&str]| -> Vec<String> {
            list.iter().map(|s| (*s).to_owned()).collect()
        };
        Ok(vec![
            ("add", args(&["add", "buy milk +home @shop"])),
            ("addm", args(&["addm", "call mum +home\nfix tap +home"])),
            ("append", args(&["append", &n, "and more"])),
            ("prepend", args(&["prepend", &n, "first"])),
            ("replace", args(&["replace", &n, "call the plumber +home"])),
            ("pri", args(&["pri", &n, "A"])),
            ("depri", args(&["depri", &pri.to_string()])),
            ("del", args(&["del", &n])),
            ("del N TERM", args(&["del", &n, word])),
            ("do", args(&["do", &n])),
            ("archive", args(&["archive"])),
            ("deduplicate", args(&["deduplicate"])),
            ("ls", args(&["ls"])),
            ("lsa", args(&["lsa"])),
            ("lsp", args(&["lsp"])),
            ("lsc", args(&["lsc"])),
            ("lsprj", args(&["lsprj"])),
        ])
    }
}

// ------------------------------------------------------------ statistics

/// Summary of one hyperfine row (times in seconds), as milliseconds.
fn summary(row: &Value) -> Result<Value, String> {
    let mut times = row
        .get("times")
        .and_then(Value::as_array)
        .and_then(|t| {
            t.iter()
                .map(|t| t.as_f64().map(Duration::from_secs_f64))
                .collect::<Option<Vec<_>>>()
        })
        .filter(|t| !t.is_empty())
        .ok_or("a result has no usable times")?;
    times.sort_unstable();
    let ms = |s: f64| (s * 1e6).round() / 1e3;
    let p = |q| ms(percentile(&times, q).as_secs_f64());
    Ok(json!({
        "name": text(row, "command")?,
        "runs": times.len(),
        "mean_ms": ms(field(row, "mean")?),
        "p50_ms": p(50),
        "p90_ms": p(90),
        "min_ms": ms(field(row, "min")?),
        "max_ms": ms(field(row, "max")?),
    }))
}

fn field(row: &Value, key: &str) -> Result<f64, String> {
    row.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("no {key}"))
}

fn text<'a>(row: &'a Value, key: &str) -> Result<&'a str, String> {
    row.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("no {key}"))
}

/// The rows of a saved result.
fn rows(results: &Value) -> Result<&[Value], String> {
    results
        .get("commands")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| "no commands".to_owned())
}

fn table(results: &Value) -> Result<String, String> {
    let mut out = format!(
        "{:<12} {:>8} {:>8} {:>8} {:>8} {:>8} {:>5}\n",
        "command", "mean ms", "p50 ms", "p90 ms", "min ms", "max ms", "runs"
    );
    for row in rows(results)? {
        writeln!(
            out,
            "{:<12} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>5}",
            text(row, "name")?,
            field(row, "mean_ms")?,
            field(row, "p50_ms")?,
            field(row, "p90_ms")?,
            field(row, "min_ms")?,
            field(row, "max_ms")?,
            field(row, "runs")?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// The change of every command's median; refuses results that were not
/// measured on the same files with the same commands and arguments.
fn compare_table(base: &Value, new: &Value) -> Result<String, String> {
    if base.get("lines") != new.get("lines") {
        return Err("the results have different numbers of lines".to_owned());
    }
    let (old, now) = (rows(base)?, rows(new)?);
    if old.len() != now.len()
        || old.iter().zip(now).any(|(a, b)| {
            (a.get("name"), a.get("args")) != (b.get("name"), b.get("args"))
        })
    {
        return Err("the results have different commands".to_owned());
    }
    let mut out = format!(
        "{:<12} {:>9} {:>9} {:>8}\n",
        "command", "base p50", "new p50", "change"
    );
    for (was, row) in old.iter().zip(now) {
        let name = text(row, "name")?;
        let (a, b) = (field(was, "p50_ms")?, field(row, "p50_ms")?);
        writeln!(
            out,
            "{name:<12} {a:>9.2} {b:>9.2} {:>+7.1}%",
            (b - a) / a * 100.0
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

// ---------------------------------------------------------------- driving

struct Settings {
    lines: usize,
    runs: usize,
    save: Option<String>,
}

/// A folder for the files: on tmpfs when the system has one.
fn work_dir() -> Result<tempfile::TempDir, String> {
    let runtime = host_var("XDG_RUNTIME_DIR").map(PathBuf::from);
    let candidates = runtime.into_iter().chain([PathBuf::from("/dev/shm")]);
    for dir in candidates {
        if dir.is_dir()
            && let Ok(t) = tempfile::Builder::new()
                .prefix("lsty-bench")
                .tempdir_in(&dir)
        {
            return Ok(t);
        }
    }
    tempfile::Builder::new()
        .prefix("lsty-bench")
        .tempdir()
        .map_err(|e| e.to_string())
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// `bench` in the folder Cargo gives the integration tests.
fn bench_dir() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("bench")
}

/// The whole environment of every program: nothing from the host.
fn scrubbed(root: &Path) -> [(&'static str, OsString); 6] {
    [
        ("PATH", SYSTEM_PATH.into()),
        ("HOME", root.join("home").into()),
        ("XDG_CONFIG_HOME", root.join("cfg").into()),
        ("XDG_STATE_HOME", root.join("state").into()),
        ("TZ", "UTC".into()),
        ("LSTY_NOW", NOW.into()),
    ]
}

/// Runs each command once on the starting files and fails unless it
/// worked and did something: a write changes a file, a list prints.
fn verify(
    root: &Path,
    commands: &[Named],
    base: (&str, &str),
) -> Result<(), String> {
    let (todo, done) = (root.join("run/todo.txt"), root.join("run/done.txt"));
    for (name, args) in commands {
        for (p, bytes) in [(&todo, base.0), (&done, base.1)] {
            write(p, bytes)?;
        }
        let out = Command::new(env!("CARGO_BIN_EXE_lsty"))
            .env_clear()
            .envs(scrubbed(root))
            .arg("--human")
            .arg("--file")
            .arg(&todo)
            .args(args)
            .output()
            .map_err(|e| format!("{name}: {e}"))?;
        let now = |p: &Path| fs::read_to_string(p).ok();
        let changed = now(&todo).as_deref() != Some(base.0)
            || now(&done).as_deref() != Some(base.1);
        let did = if name.starts_with("ls") {
            !out.stdout.is_empty()
        } else {
            changed
        };
        if !out.status.success() || !did {
            return Err(format!(
                "{name}: {} {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    Ok(())
}

/// The files in place and every command checked to work on them.
fn prepare(lines: usize, root: &Path) -> Result<Vec<Named>, String> {
    let fix = fixture(lines);
    let commands = fix.commands()?;
    for dir in ["base", "run", "home", "cfg", "state"] {
        fs::create_dir_all(root.join(dir)).map_err(|e| e.to_string())?;
    }
    let (todo, done) = (Fixture::text(&fix.todo), Fixture::text(&fix.done));
    write(&root.join("base/todo.txt"), &todo)?;
    write(&root.join("base/done.txt"), &done)?;
    verify(root, &commands, (&todo, &done))?;
    Ok(commands)
}

/// The hyperfine command for every command, on the prepared files.
fn hyperfine_command(
    hyperfine: PathBuf,
    set: &Settings,
    root: &Path,
    json_out: &Path,
    commands: &[Named],
) -> Result<Command, String> {
    let at = |name: &str| quote(&root.join(name).display().to_string());
    write(
        &root.join("restore.sh"),
        &format!(
            "cp {} {}\ncp {} {}\n",
            at("base/todo.txt"),
            at("run/todo.txt"),
            at("base/done.txt"),
            at("run/done.txt"),
        ),
    )?;
    let mut cmd = Command::new(hyperfine);
    cmd.env_clear()
        .envs(scrubbed(root))
        .args(["--shell=none", "--style=none", "--warmup"])
        .arg(WARMUP.to_string())
        .arg("--runs")
        .arg(set.runs.to_string())
        .arg("--prepare")
        .arg(format!("sh {}", at("restore.sh")))
        .arg("--export-json")
        .arg(json_out);
    let program = quote(env!("CARGO_BIN_EXE_lsty"));
    let file = at("run/todo.txt");
    for (name, args) in commands {
        let words: Vec<String> = args.iter().map(|a| quote(a)).collect();
        cmd.arg("-n").arg(name).arg(format!(
            "{program} --human --file {file} {}",
            words.join(" ")
        ));
    }
    Ok(cmd)
}

fn measure(set: &Settings) -> Result<(), String> {
    let hyperfine = host_var("PATH")
        .and_then(|p| {
            lsty_core::program::lookup(
                Path::new(""),
                p.as_ref(),
                "hyperfine".as_ref(),
            )
            .ok()
            .flatten()
        })
        .ok_or(
            "hyperfine is missing: install it (cargo install hyperfine, or \
             your package manager) and run again",
        )?;
    let tmp = work_dir()?;
    let json_out = tmp.path().join("hyperfine.json");
    let commands = prepare(set.lines, tmp.path())?;
    let mut cmd =
        hyperfine_command(hyperfine, set, tmp.path(), &json_out, &commands)?;
    let status = cmd.status().map_err(|e| format!("hyperfine: {e}"))?;
    if !status.success() {
        return Err(format!("hyperfine failed: {status}"));
    }
    let raw = fs::read_to_string(&json_out).map_err(|e| e.to_string())?;
    let raw: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let rows = raw
        .get("results")
        .and_then(Value::as_array)
        .ok_or("hyperfine wrote no results")?
        .iter()
        .zip(&commands)
        .map(|(row, (_, args))| {
            let mut sum = summary(row)?;
            sum["args"] = json!(args);
            Ok(sum)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let results = json!({ "lines": set.lines, "commands": rows });
    show(&format!(
        "{} lines in each file, {} timed runs after {WARMUP} warm-up runs\n{}",
        set.lines,
        set.runs,
        table(&results)?
    ))?;
    if let Some(name) = &set.save {
        let dir = bench_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{name}.json"));
        let text = serde_json::to_string_pretty(&results)
            .map_err(|e| e.to_string())?;
        write(&path, &text)?;
        show(&format!("saved {}\n", path.display()))?;
    }
    Ok(())
}

fn show(text: &str) -> Result<(), String> {
    let mut out = std::io::stdout().lock();
    write!(out, "\n{text}").map_err(|e| e.to_string())
}

fn load(name: &str) -> Result<Value, String> {
    let path = bench_dir().join(format!("{name}.json"));
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{name}: {e}"))
}

fn compare(names: &str) -> Result<(), String> {
    let mut it = names.split_whitespace();
    let (Some(a), Some(b), None) = (it.next(), it.next(), it.next()) else {
        return Err("compare needs two names: BASE NEW".to_owned());
    };
    let (base, new) = (load(a)?, load(b)?);
    show(&format!("{a} -> {b}\n{}", compare_table(&base, &new)?))
}

fn settings() -> Result<Settings, String> {
    Ok(Settings {
        lines: number("LSTY_BENCH_LINES")?.unwrap_or(10_000),
        runs: number("LSTY_BENCH_RUNS")?.unwrap_or(40),
        save: host_var("LSTY_BENCH_SAVE"),
    })
}

/// Checks that need no hyperfine: every command works on the measured
/// files, and results that differ are not compared.
fn check() -> Result<(), String> {
    let tmp = work_dir()?;
    prepare(settings()?.lines, tmp.path())?;
    let res = |lines, names: &[&str]| {
        let rows = names
            .iter()
            .map(|n| json!({"name": n, "args": [n], "p50_ms": 1.0}));
        json!({"lines": lines, "commands": rows.collect::<Vec<_>>()})
    };
    let (one, none) = (res(1, &["ls"]), res(1, &[]));
    let mut moved = res(1, &["ls"]);
    moved["commands"][0]["args"] = json!(["ls", "x"]);
    if compare_table(&one, &res(2, &["ls"])).is_ok()
        || compare_table(&one, &none).is_ok()
        || compare_table(&none, &one).is_ok()
        || compare_table(&one, &res(1, &["lsa"])).is_ok()
        || compare_table(&one, &json!({"lines": 1})).is_ok()
        || compare_table(&one, &moved).is_ok()
    {
        return Err("results that differ were compared".to_owned());
    }
    compare_table(&one, &one).map(drop)
}

fn main() -> std::process::ExitCode {
    let run = host_var("LSTY_BENCH").is_some();
    let diff = host_var("LSTY_BENCH_COMPARE");
    let comparing = diff.is_some();
    let measure_trial =
        Trial::test("measure every shared command", move || {
            measure(&settings()?).map_err(Failed::from)
        })
        .with_ignored_flag(!run);
    let compare_trial = Trial::test("compare two saved results", move || {
        compare(diff.as_deref().unwrap_or("")).map_err(Failed::from)
    })
    .with_ignored_flag(!comparing);
    let check = Trial::test("arguments", || check().map_err(Failed::from));
    libtest_mimic::run(
        &Arguments::from_args(),
        vec![measure_trial, compare_trial, check],
    )
    .exit_code()
}
