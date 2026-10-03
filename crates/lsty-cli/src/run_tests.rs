use std::cell::RefCell;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use tempfile::TempDir;

use super::dispatch::{USAGE_TEXT, no_file_hint, run};
use super::env::Env;
use lsty_core::discover::FoundBy;

/// A fake process: a work folder, variables and terminals.
struct Fake {
    dir: TempDir,
    vars: Vec<(String, OsString)>,
    is_tty: [bool; 3],
}

struct Ran {
    code: u8,
    out: String,
    err: String,
    tui: Vec<(PathBuf, bool)>,
}

impl Fake {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            vars: Vec::new(),
            is_tty: [false; 3],
        }
    }

    fn path(&self) -> PathBuf {
        fs::canonicalize(self.dir.path()).unwrap()
    }

    fn var(mut self, name: &str, value: impl Into<OsString>) -> Self {
        self.vars.push((name.to_owned(), value.into()));
        self
    }

    fn tty(mut self) -> Self {
        self.is_tty = [true; 3];
        self
    }

    fn file(self, name: &str, text: &str) -> Self {
        let path = self.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }

    fn run(&self, args: &[&str]) -> Ran {
        let args: Vec<OsString> = args.iter().map(Into::into).collect();
        self.run_os(args)
    }

    fn run_os(&self, args: Vec<OsString>) -> Ran {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let tui = RefCell::new(Vec::new());
        let vars = |name: &str| {
            let found = self.vars.iter().find(|(k, _)| k == name);
            found.map(|(_, v)| v.clone())
        };
        let run_tui = |a: super::TuiArgs| {
            tui.borrow_mut().push((a.found.todo, a.config.no_config));
            0
        };
        let hook = |_: &Path| -> io::Result<()> { Ok(()) };
        let code = run(Env {
            args,
            stdin: &mut io::empty(),
            stdout: &mut out,
            stderr: &mut err,
            vars: &vars,
            cwd: self.path(),
            home: None,
            is_tty: self.is_tty,
            now: "2026-09-29T12:00:00Z".parse::<Timestamp>().unwrap(),
            zone: TimeZone::UTC,
            version: "9.9.9",
            run_editor: &hook,
            open_path: &hook,
            run_tui: Some(&run_tui),
        });
        Ran {
            code,
            out: String::from_utf8(out).unwrap(),
            err: String::from_utf8(err).unwrap(),
            tui: tui.into_inner(),
        }
    }
}

fn todo_paths(ran: &Ran) -> Vec<&Path> {
    ran.tui.iter().map(|(p, _)| p.as_path()).collect()
}

#[test]
fn version_anywhere() {
    let fake = Fake::new();
    for args in [&["-V"][..], &["ls", "--version"], &["--version"]] {
        let ran = fake.run(args);
        assert_eq!((ran.code, &*ran.out, &*ran.err), (0, "lsty 9.9.9\n", ""));
    }
}

#[test]
fn version_beats_help_and_errors() {
    let ran = Fake::new().run(&["--frob", "-h", "-V"]);
    assert_eq!((ran.code, &*ran.out), (0, "lsty 9.9.9\n"));
}

#[test]
fn help_prints_the_usage_text() {
    let fake = Fake::new();
    for args in [&["-h"][..], &["frobnicate", "--help"], &["help"]] {
        let ran = fake.run(args);
        assert_eq!(ran.code, 0, "{args:?}");
        assert_eq!(ran.out, format!("{USAGE_TEXT}\n"), "{args:?}");
        assert_eq!(ran.err, "", "{args:?}");
    }
}

/// 10.23: a file named like a command opens with `./`.
#[test]
fn a_dot_slash_file_opens_the_tui() {
    let fake = Fake::new().tty().file("add", "x\n");
    let ran = fake.run(&["./add"]);
    assert_eq!(ran.code, 0);
    assert_eq!(todo_paths(&ran), [fake.path().join("add")]);
}

/// 10.24: a missing `.txt` file opens too.
#[test]
fn a_missing_txt_file_opens_the_tui() {
    let fake = Fake::new().tty();
    let ran = fake.run(&["new.txt"]);
    assert_eq!(todo_paths(&ran), [fake.path().join("new.txt")]);
}

#[test]
fn an_existing_file_without_txt_opens_the_tui() {
    let fake = Fake::new().tty().file("notes", "x\n");
    let ran = fake.run(&["notes"]);
    assert_eq!(todo_paths(&ran), [fake.path().join("notes")]);
}

#[test]
fn a_folder_is_not_a_file() {
    let fake = Fake::new().tty().file("dir/x", "");
    let ran = fake.run(&["dir"]);
    assert_eq!(ran.code, 2);
    assert_eq!(ran.tui.len(), 0);
}

fn assert_unknown(ran: &Ran, name: &str) {
    assert_eq!(ran.code, 2);
    assert_eq!(ran.out, "");
    assert_eq!(
        ran.err,
        format!(
            "lsty: unknown command \"{name}\"\n\
             hint: run \"lsty help\" for the commands\n"
        )
    );
    assert_eq!(ran.tui.len(), 0);
}

/// 10.25: a missing file without `.txt` is an unknown command.
#[test]
fn a_missing_file_without_txt_is_an_unknown_command() {
    let ran = Fake::new().tty().run(&["newfile"]);
    assert_unknown(&ran, "newfile");
}

/// 10.26: only a single argument opens the TUI.
#[test]
fn two_files_are_an_unknown_command() {
    let ran = Fake::new().tty().run(&["a.txt", "b.txt"]);
    assert_unknown(&ran, "a.txt");
}

#[test]
fn bare_lsty_opens_the_tui_on_the_discovered_file() {
    let fake = Fake::new().tty().var("TODO_FILE", "/nowhere/t.txt");
    let ran = fake.run(&[]);
    assert_eq!(ran.code, 0);
    assert_eq!(todo_paths(&ran), [Path::new("/nowhere/t.txt")]);
}

#[test]
fn tui_takes_a_file_or_the_file_option() {
    let fake = Fake::new().tty();
    let ran = fake.run(&["tui", "x.txt"]);
    assert_eq!(todo_paths(&ran), [fake.path().join("x.txt")]);
    let ran = fake.run(&["tui", "--file", "y.txt"]);
    assert_eq!(todo_paths(&ran), [fake.path().join("y.txt")]);
    let ran = fake.run(&["--file=z.txt"]);
    assert_eq!(todo_paths(&ran), [fake.path().join("z.txt")]);
    let ran = fake.run(&["tui", "", "--file=z.txt"]);
    assert_eq!(todo_paths(&ran), [fake.path().join("z.txt")]);
}

#[test]
fn no_config_reaches_the_tui() {
    let ran = Fake::new().tty().run(&["--no-config"]);
    assert!(ran.tui[0].1);
}

#[test]
fn tui_with_two_files_is_a_usage_error() {
    let ran = Fake::new().tty().run(&["tui", "a", "b"]);
    assert_eq!(ran.code, 2);
    assert_eq!(
        ran.err,
        "lsty: tui takes at most one file\nhint: usage: lsty tui [FILE]\n"
    );
}

#[test]
fn bare_lsty_takes_only_the_options_of_tui() {
    let fake = Fake::new().tty();
    for (args, name) in [
        (&["--human"][..], "human"),
        (&["--owner", "a"], "owner"),
        (&["x.txt", "--raw"], "raw"),
    ] {
        let ran = fake.run(args);
        assert_eq!(ran.code, 2, "{args:?}");
        assert_eq!(
            ran.err,
            format!(
                "lsty: tui does not take --{name}\n\
                 hint: usage: lsty tui [FILE]\n"
            ),
            "{args:?}"
        );
        assert_eq!(ran.tui.len(), 0);
    }
}

#[test]
fn an_option_error_without_a_command_gets_the_generic_hint() {
    let ran = Fake::new().run(&["--json", "--raw"]);
    assert_eq!(ran.code, 2);
    assert_eq!(
        ran.out.trim_end(),
        concat!(
            r#"{"version":1,"file":null,"done_file":null,"today":"2026-09-29","#,
            r#""error":{"code":"usage","exit":2,"#,
            r#""message":"--json and --raw cannot be used together","#,
            r#""hint":"run \"lsty help\""}}"#
        )
    );
}

/// 10.18: bare `lsty` without a terminal.
#[test]
fn bare_lsty_without_a_terminal() {
    let ran = Fake::new().run(&[]);
    assert_eq!(ran.code, 2);
    assert_eq!(ran.out, "");
    assert_eq!(
        ran.err,
        format!("{USAGE_TEXT}\nlsty: the TUI needs a terminal\n")
    );
}

/// 10.19, 10.32: the TUI needs stdin and stdout.
#[test]
fn the_tui_needs_two_terminals() {
    for tty in [[true, false, true], [false, true, true], [false; 3]] {
        let mut fake = Fake::new();
        fake.is_tty = tty;
        for args in [&["tui"][..], &["notes.txt"]] {
            let ran = fake.run(args);
            assert_eq!(ran.code, 2, "{tty:?} {args:?}");
            assert_eq!(ran.err, "lsty: the TUI needs a terminal\n");
            assert_eq!(ran.out, "");
        }
    }
}

/// 10.31: `LSTY_NONINTERACTIVE=1` beats a terminal.
#[test]
fn non_interactive_means_no_tui() {
    let fake = Fake::new().tty().var("LSTY_NONINTERACTIVE", "1");
    let ran = fake.run(&[]);
    assert_eq!(ran.code, 2);
    assert!(ran.err.ends_with("lsty: the TUI needs a terminal\n"));
    assert!(ran.err.starts_with(USAGE_TEXT));
    assert_eq!(ran.tui.len(), 0);
}

#[test]
fn a_dumb_terminal_means_no_tui() {
    let ran = Fake::new().tty().var("TERM", "dumb").run(&["tui"]);
    assert_eq!(ran.code, 2);
    assert_eq!(ran.err, "lsty: the TUI needs a terminal\n");
}

#[test]
fn without_a_tui_the_stub_fails() {
    let fake = Fake::new().tty();
    let hook = |_: &Path| -> io::Result<()> { Ok(()) };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let vars = |_: &str| None;
    let code = run(Env {
        args: vec!["tui".into()],
        stdin: &mut io::empty(),
        stdout: &mut out,
        stderr: &mut err,
        vars: &vars,
        cwd: fake.path(),
        home: None,
        is_tty: [true; 3],
        now: Timestamp::UNIX_EPOCH,
        zone: TimeZone::UTC,
        version: "0",
        run_editor: &hook,
        open_path: &hook,
        run_tui: None,
    });
    assert_eq!(code, 1);
    assert_eq!(err, b"lsty: the TUI is not built yet\n");
}

#[test]
fn edit_and_open_need_two_terminals() {
    let fake = Fake::new().file("todo.txt", "x\n");
    for name in ["edit", "open"] {
        let ran = fake.run(&[name, "1"]);
        assert_eq!(ran.code, 2, "{name}");
        assert_eq!(ran.err, format!("lsty: {name} needs a terminal\n"));
    }
    let ran = fake.run(&["edit", "1", "--json"]);
    assert_eq!(ran.code, 2);
    assert!(ran.out.contains("edit does not take --json"));
}

#[test]
fn a_command_without_a_handler_fails_with_io() {
    let fake = Fake::new().file("todo.txt", "x\n").var("HOME", "/h");
    let ran = fake.run(&["ls", "--no-config"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.err, "lsty: ls is not built yet\n");
    assert_eq!(ran.out, "");
}

#[test]
fn a_command_without_a_handler_gives_an_envelope() {
    let fake = Fake::new().file("todo.txt", "x\n");
    let ran = fake.run(&["done", "1", "--json", "--no-config"]);
    assert_eq!(ran.code, 1);
    let dir = fake.path();
    let want = format!(
        concat!(
            r#"{{"version":1,"file":"{0}/todo.txt","#,
            r#""done_file":"{0}/done.txt","today":"2026-09-29","#,
            r#""error":{{"code":"io","exit":1,"#,
            r#""message":"do is not built yet","hint":null}}}}"#,
        ),
        dir.display()
    );
    assert_eq!(ran.out.trim_end(), want);
    assert_eq!(ran.err, "");
}

/// 10.35: an error without `--json`.
#[test]
fn human_errors_are_on_stderr_only() {
    let fake = Fake::new();
    let ran = fake.run(&["ls", "--frob"]);
    assert_eq!(ran.code, 2);
    assert_eq!(ran.out, "");
    assert_eq!(
        ran.err,
        "lsty: unknown option --frob\nhint: usage: lsty ls [TERM…]\n"
    );
    let ran = fake.run(&["ls", "--sort", "size"]);
    assert_eq!(ran.out, "");
    assert!(!ran.err.contains("hint:"));
    assert_eq!(ran.err.lines().count(), 1);
}

/// 10.37: `--create` never makes a folder.
#[test]
fn create_does_not_make_a_missing_folder() {
    let fake = Fake::new();
    let target = fake.path().join("nodir/t.txt");
    let fake = fake.var("TODO_FILE", target.to_str().unwrap());
    let ran = fake.run(&["add", "X", "--create", "--human", "--no-config"]);
    assert_eq!(ran.code, 1);
    assert_eq!(
        ran.err,
        format!(
            "lsty: folder {}/nodir does not exist\n",
            fake.path().display()
        )
    );
    assert!(!fake.path().join("nodir").exists());
}

#[test]
fn create_lets_a_missing_file_through_to_the_handler() {
    let fake = Fake::new();
    let ran = fake.run(&["add", "X", "--create", "--no-config"]);
    assert_eq!(ran.err, "lsty: add is not built yet\n");
    let ran = fake.run(&["add", "X", "--no-config"]);
    assert_eq!(ran.code, 2);
    assert!(ran.err.starts_with("lsty: no todo file at "));
}

#[test]
fn an_empty_file_option_counts_as_unset() {
    let fake = Fake::new().file("todo.txt", "x\n").file(".git/x", "");
    let ran = fake.run(&["ls", "--file", "", "--no-config", "--json"]);
    assert!(ran.out.contains("is not built yet"), "{}", ran.out);
    assert!(ran.out.contains("/todo.txt"));
}

/// 10.38: the hint of each `found_by`.
#[test]
fn no_file_hints() {
    let rows = [
        (FoundBy::File, Some("check the path given with --file")),
        (FoundBy::TodoFile, Some("check TODO_FILE")),
        (FoundBy::TodoDir, Some("check TODO_DIR")),
        (
            FoundBy::DefaultFile,
            Some("check default_file in config.toml"),
        ),
        (
            FoundBy::Fallback,
            Some(
                "no todo.txt was found from here upwards; \
                 set TODO_FILE or pass --file",
            ),
        ),
        (FoundBy::WalkUp, None),
    ];
    for (found_by, want) in rows {
        let hint = no_file_hint(found_by);
        assert_eq!(hint, want, "{found_by}");
        assert!(!hint.unwrap_or("").contains("--create"));
    }
}

fn no_file_run(fake: &Fake, args: &[&str]) -> (String, String) {
    let ran = fake.run(args);
    assert_eq!(ran.code, 2, "{args:?}: {}", ran.err);
    (ran.err, ran.out)
}

type Source = (
    &'static str,
    Vec<(&'static str, String)>,
    &'static str,
    &'static str,
);

/// 10.38 again, through `run`, for each way a path is chosen.
#[test]
fn a_missing_file_names_where_the_path_came_from() {
    let fake = Fake::new().file(".git/x", "").file("d/x", "");
    let dir = fake.path();
    let config = dir.join("c.toml");
    fs::write(&config, "default_file = \"lists/t.txt\"\n").unwrap();
    let file_var = dir.join("x/t.txt");
    let todo_dir = dir.join("d");
    let rows: [Source; 4] = [
        (
            "--file",
            vec![],
            "check the path given with --file",
            "--file",
        ),
        (
            "TODO_FILE",
            vec![("TODO_FILE", file_var.display().to_string())],
            "check TODO_FILE",
            "TODO_FILE",
        ),
        (
            "TODO_DIR",
            vec![("TODO_DIR", todo_dir.display().to_string())],
            "check TODO_DIR",
            "TODO_DIR",
        ),
        (
            "default_file",
            vec![("LSTY_CONFIG", config.display().to_string())],
            "check default_file in config.toml",
            "default_file",
        ),
    ];
    let mut fake = fake;
    for (name, vars, hint, found_by) in rows {
        fake.vars = vars
            .into_iter()
            .map(|(k, v)| (k.to_owned(), OsString::from(v)))
            .collect();
        let args = if name == "--file" {
            vec!["ls", "--file", "q.txt"]
        } else {
            vec!["ls"]
        };
        let (err, _) = no_file_run(&fake, &args);
        assert!(err.contains(&format!("(chosen by {found_by})")), "{err}");
        assert!(err.contains(&format!("hint: {hint}\n")), "{err}");
        assert!(!err.contains("--create"), "{err}");
    }
}

fn os(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStringExt as _;
    OsString::from_vec(bytes.to_vec())
}

fn os_args(args: &[&[u8]]) -> Vec<OsString> {
    args.iter().map(|a| os(a)).collect()
}

#[test]
fn a_non_utf8_argument_is_a_usage_error() {
    let ran = Fake::new().run_os(os_args(&[b"add", b"caf\xe9"]));
    assert_eq!(ran.code, 2);
    assert_eq!(ran.out, "");
    assert_eq!(
        ran.err,
        "lsty: argument \"caf\u{fffd}\" is not valid UTF-8\n\
         hint: usage: lsty add TEXT…\n"
    );
}

#[test]
fn a_non_utf8_argument_with_json_is_an_envelope() {
    let ran = Fake::new().run_os(os_args(&[b"ls", b"--json", b"\xff"]));
    assert_eq!(ran.code, 2);
    assert_eq!(ran.err, "");
    assert!(ran.out.contains(r#""code":"usage""#), "{}", ran.out);
    assert!(ran.out.contains("is not valid UTF-8"), "{}", ran.out);
    assert_eq!(ran.out.lines().count(), 1);
}

#[test]
fn version_and_help_still_win_over_a_non_utf8_argument() {
    let fake = Fake::new();
    let ran = fake.run_os(os_args(&[b"-V", b"\xff"]));
    assert_eq!((ran.code, &*ran.out), (0, "lsty 9.9.9\n"));
    let ran = fake.run_os(os_args(&[b"\xff", b"-h"]));
    assert_eq!((ran.code, ran.out), (0, format!("{USAGE_TEXT}\n")));
}

#[test]
fn a_non_utf8_first_argument_is_refused() {
    let ran = Fake::new().tty().run_os(os_args(&[b"\xff.txt"]));
    assert_eq!(ran.code, 2);
    assert!(ran.err.contains("is not valid UTF-8"), "{}", ran.err);
    assert!(
        ran.err.ends_with("hint: run \"lsty help\"\n"),
        "{}",
        ran.err
    );
    assert_eq!(ran.tui.len(), 0);
}

/// A path variable that is not UTF-8 is still used, byte for byte.
#[test]
fn a_non_utf8_todo_file_is_not_unset() {
    let fake = Fake::new();
    let mut path = fake.path().into_os_string().into_encoded_bytes();
    path.extend_from_slice(b"/caf\xe9.txt");
    let fake = fake.var("TODO_FILE", os(&path)).file(".git/x", "");
    let ran = fake.run(&["ls", "--no-config", "--json"]);
    assert_eq!(ran.code, 2);
    assert!(ran.out.contains(r#""found_by":"TODO_FILE""#), "{}", ran.out);
    assert!(ran.out.contains("caf\u{fffd}.txt"), "{}", ran.out);
}

#[test]
fn a_non_utf8_clock_override_is_an_invalid_value() {
    let fake = Fake::new().var("LSTY_TODAY", os(b"\xe9"));
    let ran = fake.run(&["ls", "--no-config", "--json"]);
    assert_eq!(ran.code, 2);
    assert!(ran.out.contains(r#""today":null"#), "{}", ran.out);
    assert!(ran.out.contains(r#""field":"LSTY_TODAY""#), "{}", ran.out);
}

/// 10.33: config warnings go to stderr, the result to stdout.
#[test]
fn config_warnings_are_printed_on_stderr() {
    let fake = Fake::new().file("todo.txt", "x\n");
    let config = fake.path().join("c.toml");
    fs::write(&config, "auto_archive = \"yes\"\n").unwrap();
    let fake = fake.var("LSTY_CONFIG", config.as_os_str());
    let ran = fake.run(&["ls", "--json"]);
    assert_eq!(
        ran.err,
        "lsty: warning: config.toml: auto_archive: expected bool\n"
    );
    assert_eq!(ran.out.lines().count(), 1);
    assert!(
        ran.out.starts_with(r#"{"version":1,"file":""#),
        "{}",
        ran.out
    );
    assert!(ran.out.ends_with("}\n"));
    let ran = fake.run(&["ls", "--json", "--no-config"]);
    assert_eq!(ran.err, "");
    let ran = fake.run(&["-V"]);
    assert_eq!(ran.err, "");
}

#[test]
fn a_missing_explicit_config_warns() {
    let fake = Fake::new().file("todo.txt", "x\n");
    let missing = fake.path().join("missing.toml");
    let fake = fake.var("LSTY_CONFIG", missing.as_os_str());
    let ran = fake.run(&["ls"]);
    assert_eq!(
        ran.err,
        format!(
            "lsty: warning: config.toml: {} not found\n\
             lsty: ls is not built yet\n",
            missing.display()
        )
    );
}

/// Without a home folder, the config is looked for from the working
/// folder.
#[test]
fn no_home_means_a_config_relative_to_the_working_folder() {
    let fake = Fake::new()
        .file("todo.txt", "x\n")
        .file(".config/lsty/config.toml", "auto_archive = \"yes\"\n");
    let ran = fake.run(&["ls", "--json"]);
    assert!(
        ran.err
            .starts_with("lsty: warning: config.toml: auto_archive"),
        "{}",
        ran.err
    );
}
