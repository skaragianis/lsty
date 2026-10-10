//! Running one transcript.

use std::{
    collections::BTreeMap,
    fmt::{Display, Write as _},
    fs,
    io::{self, Write as _},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    thread,
};

use regex::bytes::Regex;

use crate::{
    Outcome, diff, json, parse,
    placeholder::{Bindings, match_text},
    pty::Pty,
    txtar::Archive,
};

/// What every script of a run shares.
pub(crate) struct Config {
    /// The workspace's `testdata` folder.
    pub(crate) testdata: PathBuf,
    /// The scripts' `PATH`: the folder of `_bin`, then the host's `PATH`.
    pub(crate) path: String,
    /// A link to `lsty`; dropping it removes the link.
    pub(crate) _bin: tempfile::TempDir,
    /// Keep the work folder of a failed script.
    pub(crate) keep_work: bool,
}

/// Why a script stopped before its last line.
enum Halt {
    Fail(String),
    Skip(String),
    Stop,
}

type Step = Result<(), Halt>;

fn fail(message: impl Into<String>) -> Halt {
    Halt::Fail(message.into())
}

/// Why a finished background command fails its script, if it does.
fn background_failure(
    negated: bool,
    line: &str,
    output: &Output,
) -> Option<String> {
    if output.status.success() != negated {
        return None;
    }
    let what = if negated { "success" } else { "failure" };
    Some(format!("unexpected {what} of background {line}"))
}

/// Why `exists` fails for `path`, if it does.
fn exists_problem(
    path: &Path,
    negated: bool,
    readonly: bool,
) -> Option<String> {
    let shown = path.display();
    match (fs::metadata(path), negated) {
        (Ok(meta), true) => {
            let what = if meta.is_dir() { "directory" } else { "file" };
            Some(format!("{what} {shown} unexpectedly exists"))
        }
        (Err(_), false) => Some(format!("{shown} does not exist")),
        (Ok(meta), false)
            if readonly && meta.permissions().mode() & 0o222 != 0 =>
        {
            Some(format!("{shown} exists but is writable"))
        }
        (Ok(_), false) | (Err(_), true) => None,
    }
}

/// Creates the file of the archive section `name` at `path`.
fn create_section(name: &str, path: &Path) -> Result<fs::File, Halt> {
    fs::File::create_new(path).map_err(|e| {
        if e.kind() == io::ErrorKind::AlreadyExists {
            fail(format!("{name} would overwrite {}", path.display()))
        } else {
            fail(format!("{name}: {e}"))
        }
    })
}

fn check<T, E: Display>(result: Result<T, E>) -> Result<T, Halt> {
    result.map_err(|e| fail(e.to_string()))
}

struct Background {
    line: String,
    child: Child,
    negated: bool,
}

struct Script<'a> {
    config: &'a Config,
    work: PathBuf,
    cwd: PathBuf,
    env: Vec<(String, String)>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdin: Option<Vec<u8>>,
    ttyin: Option<Vec<u8>>,
    bindings: Bindings,
    background: Vec<Background>,
    log: String,
    /// The archive section each extracted file came from.
    sections: BTreeMap<PathBuf, usize>,
    /// New contents for archive sections, written back on success.
    updates: BTreeMap<usize, Vec<u8>>,
    /// Rewrite `cmp` sections that differ instead of failing.
    update: bool,
}

/// Runs `archive`, the transcript named `name`. With updates on, also
/// returns the archive rewritten when a `cmp` section changed.
pub(crate) fn run(
    config: &Config,
    name: &str,
    mut archive: Archive,
    update: bool,
) -> (Outcome, Option<Vec<u8>>) {
    let work = match tempfile::Builder::new().prefix("lsty-script-").tempdir()
    {
        Ok(work) => work,
        Err(e) => return (Outcome::Fail(format!("work folder: {e}")), None),
    };
    let path = fs::canonicalize(work.path())
        .unwrap_or_else(|_| work.path().to_path_buf());

    let mut script = Script::new(config, path, update);
    let result = script
        .setup(&archive)
        .and_then(|()| script.run_lines(name, &archive.comment));
    script.stop_background();

    let outcome = match result {
        Ok(()) | Err(Halt::Stop) => Outcome::Pass,
        Err(Halt::Skip(reason)) => Outcome::Skip(reason),
        Err(Halt::Fail(message)) => {
            let mut log = script.log;
            if config.keep_work {
                let kept = work.keep();
                writeln!(log, "work folder kept: {}", kept.display()).ok();
            }
            writeln!(log, "FAIL: {message}").ok();
            return (Outcome::Fail(log), None);
        }
    };

    if script.updates.is_empty() {
        return (outcome, None);
    }
    for (index, bytes) in script.updates {
        if let Some((_, data)) = archive.files.get_mut(index) {
            *data = bytes;
        }
    }
    (outcome, Some(archive.format()))
}

impl<'a> Script<'a> {
    fn new(config: &'a Config, work: PathBuf, update: bool) -> Self {
        Self {
            config,
            cwd: work.clone(),
            work,
            env: Vec::new(),
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdin: None,
            ttyin: None,
            bindings: Bindings::new(),
            background: Vec::new(),
            log: String::new(),
            sections: BTreeMap::new(),
            updates: BTreeMap::new(),
            update,
        }
    }

    /// testscript's environment, the archive's files, then lsty's test
    /// environment.
    fn setup(&mut self, archive: &Archive) -> Step {
        let work = self.work.display().to_string();
        let tmp = self.work.join(".tmp");
        check(fs::create_dir_all(&tmp))?;
        for (key, value) in [
            ("WORK", work.as_str()),
            ("PATH", self.config.path.as_str()),
            ("HOME", "/no-home"),
            ("TMPDIR", &tmp.display().to_string()),
            ("devnull", "/dev/null"),
            ("/", "/"),
            (":", ":"),
            ("$", "$"),
            ("exe", ""),
        ] {
            self.setenv(key, value);
        }

        for (index, (name, data)) in archive.files.iter().enumerate() {
            let path = self.abs(&self.expand(name));
            if let Some(dir) = path.parent() {
                check(fs::create_dir_all(dir))?;
            }
            let mut file = create_section(name, &path)?;
            check(file.write_all(data))?;
            self.sections.insert(path, index);
        }

        let home = self.work.join("home");
        let themes = self.config.testdata.join("themes");
        for (key, value) in [
            ("THEMES", themes),
            ("XDG_CONFIG_HOME", home.join(".config")),
            ("XDG_STATE_HOME", home.join(".local/state")),
            ("HOME", home),
        ] {
            self.setenv(key, &value.display().to_string());
        }
        self.setenv("TZ", "UTC");
        self.setenv("LSTY_NOW", "2026-09-29T0900Z");
        self.setenv("TERM", "xterm-256color");
        Ok(())
    }

    fn run_lines(&mut self, name: &str, script: &str) -> Step {
        for (index, line) in script.lines().enumerate() {
            self.run_line(line).map_err(|halt| match halt {
                Halt::Fail(message) => {
                    fail(format!("{name}:{}: {message}", index + 1))
                }
                other @ (Halt::Skip(_) | Halt::Stop) => other,
            })?;
        }
        Ok(())
    }

    fn run_line(&mut self, line: &str) -> Step {
        let words = parse::words(line, |s| self.expand(s)).map_err(fail)?;
        if words.is_empty() {
            return Ok(());
        }
        writeln!(self.log, "> {line}").ok();

        let mut words = words.as_slice();
        while let Some(cond) = words
            .first()
            .and_then(|w| w.strip_prefix('[')?.strip_suffix(']'))
        {
            words = &words[1..];
            if words.is_empty() {
                return Err(fail("missing command after condition"));
            }
            let cond = cond.trim();
            let (want, cond) = match cond.strip_prefix('!') {
                Some(rest) => (false, rest.trim()),
                None => (true, cond),
            };
            if self.condition(cond)? != want {
                return Ok(());
            }
        }

        let negated = words.first().is_some_and(|w| w == "!");
        if negated {
            words = &words[1..];
        }
        let Some((command, args)) = words.split_first() else {
            return Err(fail("! on line by itself"));
        };
        self.dispatch(command, negated, args)
    }

    fn condition(&self, cond: &str) -> Result<bool, Halt> {
        if let Some(program) = cond.strip_prefix("exec:") {
            return Ok(self.resolve(program).is_ok());
        }
        match cond {
            "unix" => Ok(cfg!(unix)),
            "linux" => Ok(cfg!(target_os = "linux")),
            "darwin" => Ok(cfg!(target_os = "macos")),
            _ => Err(fail(format!("unknown condition {cond:?}"))),
        }
    }

    fn dispatch(
        &mut self,
        command: &str,
        negated: bool,
        args: &[String],
    ) -> Step {
        match command {
            "exec" => self.cmd_exec(negated, args),
            "stdout" => self.cmd_match_output(negated, args, "stdout"),
            "stderr" => self.cmd_match_output(negated, args, "stderr"),
            "cmp" => self.cmd_cmp(negated, args, false),
            "cmpenv" => self.cmd_cmp(negated, args, true),
            "exists" => self.cmd_exists(negated, args),
            _ if negated => Err(fail(format!("unsupported: ! {command}"))),
            "cd" => self.cmd_cd(args),
            "cp" => self.cmd_cp(args),
            "env" => {
                self.cmd_env(args);
                Ok(())
            }
            "mkdir" => self.cmd_mkdir(args),
            "rm" => self.cmd_rm(args),
            "skip" => Self::cmd_skip(args),
            "stdin" => self.cmd_stdin(args),
            "stop" => Err(Halt::Stop),
            "ttyin" => self.cmd_ttyin(args),
            "wait" => self.cmd_wait(args),
            "exit" => self.cmd_exit(args),
            "match" => self.cmd_placeholder_match(args),
            "cmpjson" => self.cmd_cmpjson(args),
            _ => Err(fail(format!("unknown command {command:?}"))),
        }
    }

    // ----- environment and paths

    fn getenv(&self, key: &str) -> String {
        self.env
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }

    fn setenv(&mut self, key: &str, value: &str) {
        match self.env.iter_mut().find(|(k, _)| k == key) {
            Some(pair) => value.clone_into(&mut pair.1),
            None => self.env.push((key.to_owned(), value.to_owned())),
        }
    }

    fn expand(&self, text: &str) -> String {
        parse::expand(text, |key| self.getenv(key))
    }

    fn abs(&self, path: &str) -> PathBuf {
        self.cwd.join(path)
    }

    /// A file's bytes; `stdout` and `stderr` name the last command's output.
    fn read_file(&self, name: &str) -> Result<Vec<u8>, Halt> {
        match name {
            "stdout" => Ok(self.stdout.clone()),
            "stderr" => Ok(self.stderr.clone()),
            _ => fs::read(self.abs(name))
                .map_err(|e| fail(format!("{name}: {e}"))),
        }
    }

    /// A file's text, for the commands that parse it.
    fn read_text(&self, name: &str) -> Result<String, Halt> {
        String::from_utf8(self.read_file(name)?)
            .map_err(|_| fail(format!("{name}: not UTF-8")))
    }

    // ----- running programs

    fn resolve(&self, name: &str) -> Result<PathBuf, Halt> {
        if name.contains('/') {
            return Ok(self.abs(name));
        }
        self.getenv("PATH")
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(|dir| Path::new(dir).join(name))
            .find(|path| is_executable(path))
            .ok_or_else(|| {
                fail(format!(
                    "exec: {name:?}: executable file not found in $PATH"
                ))
            })
    }

    fn command(
        &self,
        program: &str,
        args: &[String],
    ) -> Result<Command, Halt> {
        let mut command = Command::new(self.resolve(program)?);
        command
            .args(args)
            .env_clear()
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .env("PWD", &self.cwd)
            .current_dir(&self.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::with_coverage(&mut command);
        Ok(command)
    }

    fn run_program(
        &mut self,
        program: &str,
        args: &[String],
    ) -> Result<ExitStatus, Halt> {
        let mut command = self.command(program, args)?;
        let stdin = self.stdin.take().unwrap_or_default();
        let pty = if let Some(input) = self.ttyin.take() {
            let (pty, tty) = check(Pty::open(input))?;
            command.stdin(Stdio::from(tty));
            Some(pty)
        } else {
            command.stdin(Stdio::piped());
            None
        };

        let spawned = command.spawn();
        // The command holds the parent's copy of the terminal side; the
        // pty sees end of input only once every copy is closed.
        drop(command);
        let mut child =
            spawned.map_err(|e| fail(format!("exec {program}: {e}")))?;

        let feeder = child.stdin.take().map(|mut pipe| {
            thread::spawn(move || {
                // The program may exit without reading its input.
                let fed = pipe.write_all(&stdin);
                drop(fed);
            })
        });
        let output = child.wait_with_output();
        if let Some(feeder) = feeder {
            let joined = feeder.join();
            drop(joined);
        }
        if let Some(pty) = pty {
            pty.finish();
        }
        let Output {
            status,
            stdout,
            stderr,
        } = check(output)?;
        if !stdout.is_empty() {
            write!(self.log, "[stdout]\n{}", lossy(&stdout)).ok();
        }
        if !stderr.is_empty() {
            write!(self.log, "[stderr]\n{}", lossy(&stderr)).ok();
        }
        self.stdout = stdout;
        self.stderr = stderr;
        Ok(status)
    }

    fn stop_background(&mut self) {
        for mut bg in self.background.drain(..) {
            let killed = bg.child.kill();
            drop(killed);
            let reaped = bg.child.wait();
            drop(reaped);
        }
    }

    // ----- commands

    fn cmd_exec(&mut self, negated: bool, args: &[String]) -> Step {
        let (args, background) = match args.split_last() {
            Some((last, rest)) if last == "&" => (rest, true),
            _ => (args, false),
        };
        let Some((program, args)) = args.split_first() else {
            return Err(fail("usage: exec program [args...] [&]"));
        };

        if background {
            if self.stdin.is_some() || self.ttyin.is_some() {
                return Err(fail(
                    "stdin and ttyin are not supported by background commands",
                ));
            }
            let mut command = self.command(program, args)?;
            command.stdin(Stdio::null());
            let child = command
                .spawn()
                .map_err(|e| fail(format!("exec {program}: {e}")))?;
            self.background.push(Background {
                line: format!("{program} {}", args.join(" ")),
                child,
                negated,
            });
            self.stdout.clear();
            self.stderr.clear();
            return Ok(());
        }

        let status = self.run_program(program, args)?;
        match (status.success(), negated) {
            (true, true) => Err(fail("unexpected command success")),
            (true, false) => Ok(()),
            (false, negated) => {
                writeln!(self.log, "[{status}]").ok();
                if negated {
                    Ok(())
                } else {
                    Err(fail("unexpected command failure"))
                }
            }
        }
    }

    /// `exit N program args…`: runs as `exec` does and requires exit code N.
    fn cmd_exit(&mut self, args: &[String]) -> Step {
        let [code, program, args @ ..] = args else {
            return Err(fail("usage: exit N program [args...]"));
        };
        let want: i32 = code.parse().map_err(|_| {
            fail(format!("exit: {code:?} is not an exit code"))
        })?;
        let status = self.run_program(program, args)?;
        match status.code() {
            Some(got) if got == want => Ok(()),
            Some(got) => Err(fail(format!("exit code is {got}, want {want}"))),
            None => Err(fail(format!("exit: {program}: {status}"))),
        }
    }

    fn cmd_wait(&mut self, args: &[String]) -> Step {
        if !args.is_empty() {
            return Err(fail("usage: wait"));
        }
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut failure = None;
        // Children not yet waited for stay listed, so a failed wait still
        // lets `stop_background` reap them.
        while !self.background.is_empty() {
            let bg = self.background.remove(0);
            let output = check(bg.child.wait_with_output())?;
            writeln!(self.log, "[background] {}: {}", bg.line, output.status)
                .ok();
            stdout.extend_from_slice(&output.stdout);
            stderr.extend_from_slice(&output.stderr);
            failure = failure
                .or_else(|| background_failure(bg.negated, &bg.line, &output));
        }
        self.stdout = stdout;
        self.stderr = stderr;
        failure.map_or(Ok(()), |message| Err(fail(message)))
    }

    fn cmd_match_output(
        &mut self,
        negated: bool,
        args: &[String],
        name: &str,
    ) -> Step {
        let (count, args) = match args.split_first() {
            Some((first, rest)) if first.starts_with("-count=") => {
                if negated {
                    return Err(fail("cannot use -count= with negated match"));
                }
                let n: usize = first["-count=".len()..]
                    .parse()
                    .map_err(|e| fail(format!("bad -count=: {e}")))?;
                if n == 0 {
                    return Err(fail("bad -count=: must be at least 1"));
                }
                (Some(n), rest)
            }
            _ => (None, args),
        };
        let [pattern] = args else {
            return Err(fail(format!("usage: {name} [-count=N] 'pattern'")));
        };
        let re = check(Regex::new(&format!("(?m){pattern}")))?;
        let out = if name == "stdout" {
            &self.stdout
        } else {
            &self.stderr
        };

        if negated {
            if let Some(m) = re.find(out) {
                return Err(fail(format!(
                    "unexpected match for `{pattern}` found in {name}: {}",
                    lossy(m.as_bytes())
                )));
            }
            return Ok(());
        }
        if !re.is_match(out) {
            return Err(fail(format!(
                "no match for `{pattern}` found in {name}"
            )));
        }
        if let Some(want) = count {
            let got = re.find_iter(out).count();
            if got != want {
                return Err(fail(format!(
                    "have {got} matches for `{pattern}`, want {want}"
                )));
            }
        }
        Ok(())
    }

    fn cmd_cmp(&mut self, negated: bool, args: &[String], env: bool) -> Step {
        let [name1, name2] = args else {
            return Err(fail("usage: cmp file1 file2"));
        };
        if name1 == name2 {
            return Err(fail("cmp: cannot compare a file against itself"));
        }
        let got = self.read_file(name1)?;
        let want = if env {
            self.expand(&self.read_text(name2)?).into_bytes()
        } else {
            self.read_file(name2)?
        };
        let equal = got == want;
        if negated {
            return if equal {
                Err(fail(format!("{name1} and {name2} do not differ")))
            } else {
                Ok(())
            };
        }
        if equal {
            return Ok(());
        }
        if self.update
            && !env
            && let Some(index) = self.sections.get(&self.abs(name2))
        {
            self.updates.insert(*index, got);
            return Ok(());
        }
        self.log.push_str(&diff::unified(
            name1,
            &lossy(&got),
            name2,
            &lossy(&want),
        ));
        Err(fail(format!("{name1} and {name2} differ")))
    }

    /// `match FILE WANT`: as `cmp`, but WANT may hold placeholders.
    fn cmd_placeholder_match(&mut self, args: &[String]) -> Step {
        let [name, want_name] = args else {
            return Err(fail("usage: match file want"));
        };
        let got = self.read_text(name)?;
        let want = self.read_text(want_name)?;
        let Some(found) = match_text(&want, &got, &self.bindings) else {
            self.log
                .push_str(&diff::unified(want_name, &want, name, &got));
            return Err(fail(format!("{name} does not match {want_name}")));
        };
        self.bind(found);
        Ok(())
    }

    fn cmd_cmpjson(&mut self, args: &[String]) -> Step {
        let [name, want_name] = args else {
            return Err(fail("usage: cmpjson file want"));
        };
        let got = self.read_text(name)?;
        let want = self.read_text(want_name)?;
        let work = self.getenv("WORK");
        let found = json::compare(&want, &got, &work, &self.bindings)
            .map_err(|reason| {
                fail(format!("{name} differs from {want_name}: {reason}"))
            })?;
        self.bind(found);
        Ok(())
    }

    fn bind(&mut self, found: Bindings) {
        for (name, value) in found {
            self.setenv(&name, &value);
            self.bindings.insert(name, value);
        }
    }

    fn cmd_cd(&mut self, args: &[String]) -> Step {
        let [dir] = args else {
            return Err(fail("usage: cd dir"));
        };
        let dir = self.abs(dir);
        if !dir.is_dir() {
            return Err(fail(format!("{} is not a directory", dir.display())));
        }
        self.cwd = dir;
        Ok(())
    }

    fn cmd_cp(&mut self, args: &[String]) -> Step {
        let Some((dst, srcs)) =
            args.split_last().filter(|(_, s)| !s.is_empty())
        else {
            return Err(fail("usage: cp src... dst"));
        };
        let dst = self.abs(dst);
        let into_dir = dst.is_dir();
        if srcs.len() > 1 && !into_dir {
            return Err(fail(format!(
                "cp: destination {} is not a directory",
                dst.display()
            )));
        }
        for src in srcs {
            let (data, mode) = match src.as_str() {
                "stdout" => (self.stdout.clone(), 0o666),
                "stderr" => (self.stderr.clone(), 0o666),
                _ => {
                    let path = self.abs(src);
                    let mode =
                        check(fs::metadata(&path))?.permissions().mode();
                    (check(fs::read(&path))?, mode & 0o777)
                }
            };
            let target = if into_dir {
                dst.join(Path::new(src).file_name().unwrap_or_default())
            } else {
                dst.clone()
            };
            check(fs::write(&target, data))?;
            check(fs::set_permissions(
                &target,
                fs::Permissions::from_mode(mode),
            ))?;
        }
        Ok(())
    }

    fn cmd_env(&mut self, args: &[String]) {
        if args.is_empty() {
            for (key, value) in &self.env {
                writeln!(self.log, "{key}={value}").ok();
            }
            return;
        }
        for arg in args {
            if let Some((key, value)) = arg.split_once('=') {
                self.setenv(key, value);
            } else {
                let value = self.getenv(arg);
                writeln!(self.log, "{arg}={value}").ok();
            }
        }
    }

    fn cmd_exists(&self, negated: bool, args: &[String]) -> Step {
        let (readonly, files) = match args.split_first() {
            Some((first, rest)) if first == "-readonly" => (true, rest),
            _ => (false, args),
        };
        if files.is_empty() {
            return Err(fail("usage: exists [-readonly] file..."));
        }
        for file in files {
            let path = self.abs(file);
            if let Some(problem) = exists_problem(&path, negated, readonly) {
                return Err(fail(problem));
            }
        }
        Ok(())
    }

    fn cmd_mkdir(&self, args: &[String]) -> Step {
        if args.is_empty() {
            return Err(fail("usage: mkdir dir..."));
        }
        for dir in args {
            check(fs::create_dir_all(self.abs(dir)))?;
        }
        Ok(())
    }

    fn cmd_rm(&self, args: &[String]) -> Step {
        if args.is_empty() {
            return Err(fail("usage: rm file..."));
        }
        for file in args {
            let path = self.abs(file);
            let removed = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            match removed {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    return Err(fail(format!("rm {}: {e}", path.display())));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn cmd_skip(args: &[String]) -> Step {
        if args.len() > 1 {
            return Err(fail("usage: skip [msg]"));
        }
        Err(Halt::Skip(args.first().cloned().unwrap_or_default()))
    }

    fn cmd_stdin(&mut self, args: &[String]) -> Step {
        let [file] = args else {
            return Err(fail("usage: stdin filename"));
        };
        if self.ttyin.is_some() {
            return Err(fail("conflicting use of 'stdin' and 'ttyin -stdin'"));
        }
        self.stdin = Some(self.read_file(file)?);
        Ok(())
    }

    /// `ttyin -stdin FILE`: the next `exec` gets a terminal on stdin that
    /// reads FILE.
    fn cmd_ttyin(&mut self, args: &[String]) -> Step {
        let [flag, file] = args else {
            return Err(fail("usage: ttyin -stdin filename"));
        };
        if flag != "-stdin" {
            return Err(fail("usage: ttyin -stdin filename"));
        }
        if self.stdin.is_some() {
            return Err(fail("conflicting use of 'stdin' and 'ttyin -stdin'"));
        }
        let input = self.read_file(file)?;
        if input.is_empty() {
            return Err(fail("tty input file is empty"));
        }
        self.ttyin = Some(input);
        Ok(())
    }
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn lossy(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}
