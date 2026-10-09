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
//! `bench/NAME.json` in the target folder.
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

use libtest_mimic::{Arguments, Failed, Trial};
use serde_json::{Value, json};

#[expect(dead_code, reason = "shared with todo_sh, which uses the rest")]
#[path = "todo_sh/random.rs"]
mod random;

use random::{Rng, file_line};

/// The seed of the files.
const SEED: u64 = 1;
/// The day `lsty` believes it is.
const NOW: &str = "2026-10-07T1200Z";
/// Runs before the timed ones, to warm the caches.
const WARMUP: usize = 5;
/// The `PATH` of every program: the system's tools.
const SYSTEM_PATH: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin";

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
            } else if !ids.is_empty() && rng.one_in(20) {
                let id = ids.get(rng.below(ids.len())).cloned();
                line = format!("{line} dep:{}", id.unwrap_or_default());
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

    /// The first line from the middle on that is open and satisfies `ok`.
    fn middle(&self, ok: impl Fn(&str) -> bool) -> Option<(usize, &str)> {
        let mid = self.todo.len() / 2;
        self.todo
            .iter()
            .enumerate()
            .skip(mid)
            .find(|(_, l)| is_open(l) && ok(l))
            .map(|(i, l)| (i + 1, l.as_str()))
    }

    /// Every command: its name and arguments, naming real items.
    fn commands(&self) -> Result<Vec<Named>, String> {
        let plain = |w: &str| {
            w.len() >= 3 && w.chars().all(|c| c.is_ascii_alphabetic())
        };
        let (nr, line) = self
            .middle(|l| l.split(' ').any(plain))
            .ok_or("no item with a plain word")?;
        let word = line.split(' ').find(|w| plain(w)).unwrap_or("");
        let (pri, _) = self
            .middle(|l| {
                let b = l.as_bytes();
                b.first() == Some(&b'(')
                    && b.get(1).is_some_and(u8::is_ascii_uppercase)
                    && b.get(2) == Some(&b')')
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

/// The nearest-rank percentile of sorted `times`.
fn percentile(times: &[f64], p: usize) -> f64 {
    let rank = (p * times.len()).div_ceil(100).max(1);
    times.get(rank - 1).copied().unwrap_or(0.0)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a number of runs is far below 2^52"
)]
const fn count(n: usize) -> f64 {
    n as f64
}

/// Summary of one command's run times in seconds, as milliseconds.
fn summary(name: &str, times: &[f64]) -> Value {
    let mut t = times.to_vec();
    t.sort_by(f64::total_cmp);
    let ms = |s: f64| (s * 1e6).round() / 1e3;
    let mean = t.iter().sum::<f64>() / count(t.len().max(1));
    json!({
        "name": name,
        "runs": t.len(),
        "mean_ms": ms(mean),
        "p50_ms": ms(percentile(&t, 50)),
        "p90_ms": ms(percentile(&t, 90)),
        "min_ms": ms(t.first().copied().unwrap_or(0.0)),
        "max_ms": ms(t.last().copied().unwrap_or(0.0)),
    })
}

fn field(row: &Value, key: &str) -> f64 {
    row.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn name_of(row: &Value) -> &str {
    row.get("name").and_then(Value::as_str).unwrap_or("")
}

fn table(results: &Value) -> String {
    let mut out = format!(
        "{:<12} {:>8} {:>8} {:>8} {:>8} {:>8} {:>5}\n",
        "command", "mean ms", "p50 ms", "p90 ms", "min ms", "max ms", "runs"
    );
    let rows = results.get("commands").and_then(Value::as_array);
    for row in rows.into_iter().flatten() {
        writeln!(
            out,
            "{:<12} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>5}",
            name_of(row),
            field(row, "mean_ms"),
            field(row, "p50_ms"),
            field(row, "p90_ms"),
            field(row, "min_ms"),
            field(row, "max_ms"),
            field(row, "runs"),
        )
        .unwrap_or_default();
    }
    out
}

fn compare_table(base: &Value, new: &Value) -> String {
    let mut out = format!(
        "{:<12} {:>9} {:>9} {:>8}\n",
        "command", "base p50", "new p50", "change"
    );
    let rows = |v: &Value| {
        v.get("commands")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let old = rows(base);
    for row in rows(new) {
        let Some(was) = old.iter().find(|o| name_of(o) == name_of(&row))
        else {
            continue;
        };
        let (a, b) = (field(was, "p50_ms"), field(&row, "p50_ms"));
        writeln!(
            out,
            "{:<12} {:>9.2} {:>9.2} {:>+7.1}%",
            name_of(&row),
            a,
            b,
            (b - a) / a.max(f64::MIN_POSITIVE) * 100.0
        )
        .unwrap_or_default();
    }
    out
}

// ---------------------------------------------------------------- driving

struct Settings {
    lines: usize,
    runs: usize,
    save: Option<String>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the test, not lsty, reads its settings from the host"
)]
fn host_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn number(name: &str, default: usize) -> Result<usize, String> {
    match host_var(name) {
        None => Ok(default),
        Some(v) => v.parse().map_err(|_| format!("{name}: not a number: {v}")),
    }
}

/// The program `name` on the host's `PATH`.
fn find_program(name: &str) -> Option<PathBuf> {
    let path = host_var("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
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

/// `bench` in the target folder, wherever the build put it.
fn bench_dir() -> PathBuf {
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    tmp.parent().unwrap_or(tmp).join("bench")
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
fn verify(root: &Path, commands: &[Named]) -> Result<(), String> {
    let (todo, done) = (root.join("run/todo.txt"), root.join("run/done.txt"));
    let first = |p: &Path| {
        fs::read(root.join("base").join(p.file_name().unwrap_or_default()))
    };
    for (name, args) in commands {
        for p in [&todo, &done] {
            let bytes = first(p).map_err(|e| e.to_string())?;
            fs::write(p, bytes).map_err(|e| e.to_string())?;
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
        let changed = [&todo, &done]
            .iter()
            .any(|p| fs::read(p).ok() != first(p).ok());
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

/// The summary of each command in hyperfine's results.
fn summaries(raw: &Value) -> Vec<Value> {
    let results = raw.get("results").and_then(Value::as_array);
    results
        .into_iter()
        .flatten()
        .map(|r| {
            let times: Vec<f64> = r
                .get("times")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_f64)
                .collect();
            let name = r.get("command").and_then(Value::as_str);
            summary(name.unwrap_or(""), &times)
        })
        .collect()
}

/// The hyperfine command for every command, with the files in place.
fn hyperfine_command(
    hyperfine: PathBuf,
    set: &Settings,
    root: &Path,
    json_out: &Path,
) -> Result<Command, String> {
    let fix = fixture(set.lines);
    let commands = fix.commands()?;
    for dir in ["base", "run", "home", "cfg", "state"] {
        fs::create_dir_all(root.join(dir)).map_err(|e| e.to_string())?;
    }
    write(&root.join("base/todo.txt"), &Fixture::text(&fix.todo))?;
    write(&root.join("base/done.txt"), &Fixture::text(&fix.done))?;
    let restore = root.join("restore.sh");
    let at =
        |name: &str| random::quote(&root.join(name).display().to_string());
    write(
        &restore,
        &format!(
            "cp {} {}\ncp {} {}\n",
            at("base/todo.txt"),
            at("run/todo.txt"),
            at("base/done.txt"),
            at("run/done.txt"),
        ),
    )?;
    verify(root, &commands)?;
    let mut cmd = Command::new(hyperfine);
    cmd.env_clear()
        .envs(scrubbed(root))
        .args(["--shell=none", "--style=none", "--warmup"])
        .arg(WARMUP.to_string())
        .arg("--runs")
        .arg(set.runs.to_string())
        .arg("--prepare")
        .arg(format!(
            "sh {}",
            random::quote(&restore.display().to_string())
        ))
        .arg("--export-json")
        .arg(json_out);
    let program = random::quote(env!("CARGO_BIN_EXE_lsty"));
    let file = random::quote(&root.join("run/todo.txt").display().to_string());
    for (name, args) in &commands {
        let words: Vec<String> =
            args.iter().map(|a| random::quote(a)).collect();
        cmd.arg("-n").arg(name).arg(format!(
            "{program} --human --file {file} {}",
            words.join(" ")
        ));
    }
    Ok(cmd)
}

fn measure(set: &Settings) -> Result<(), String> {
    let hyperfine = find_program("hyperfine").ok_or(
        "hyperfine is missing: install it (cargo install hyperfine, or your \
         package manager) and run again",
    )?;
    let tmp = work_dir()?;
    let json_out = tmp.path().join("hyperfine.json");
    let mut cmd = hyperfine_command(hyperfine, set, tmp.path(), &json_out)?;
    let status = cmd.status().map_err(|e| format!("hyperfine: {e}"))?;
    if !status.success() {
        return Err(format!("hyperfine failed: {status}"));
    }
    let raw = fs::read_to_string(&json_out).map_err(|e| e.to_string())?;
    let raw: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let rows = summaries(&raw);
    let results = json!({ "lines": set.lines, "commands": rows });
    show(&format!(
        "{} lines in each file, {} timed runs after {WARMUP} warm-up runs\n{}",
        set.lines,
        set.runs,
        table(&results)
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
    show(&format!("{a} -> {b}\n{}", compare_table(&base, &new)))
}

fn settings() -> Result<Settings, String> {
    Ok(Settings {
        lines: number("LSTY_BENCH_LINES", 10_000)?,
        runs: number("LSTY_BENCH_RUNS", 40)?,
        save: host_var("LSTY_BENCH_SAVE"),
    })
}

/// Checks that need no hyperfine: the statistics and the arguments.
fn check() -> Result<(), String> {
    let times: Vec<f64> = (1..=10).map(f64::from).collect();
    let got = (percentile(&times, 50), percentile(&times, 90));
    if got != (5.0, 9.0) {
        return Err(format!("nearest-rank percentiles: {got:?}"));
    }
    let commands = fixture(200).commands()?;
    if commands.len() != 17 {
        return Err(format!("{} commands", commands.len()));
    }
    let del = commands.iter().find(|(n, _)| *n == "del N TERM");
    if del.is_none_or(|(_, a)| a.len() != 3) {
        return Err("del N TERM has no term".to_owned());
    }
    Ok(())
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
    let check = Trial::test("percentiles and arguments", || {
        check().map_err(Failed::from)
    });
    libtest_mimic::run(
        &Arguments::from_args(),
        vec![measure_trial, compare_trial, check],
    )
    .exit_code()
}
