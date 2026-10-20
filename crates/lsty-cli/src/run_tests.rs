use std::{
    cell::RefCell,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    rc::Rc,
};

use jiff::{Timestamp, tz::TimeZone};
use lsty_core::{discover::FoundBy, links::HookResult};
use tempfile::TempDir;

use super::{
    dispatch::{USAGE_TEXT, no_file_hint, run},
    env::Env,
};

/// A fake process: a work folder, variables and terminals.
pub(crate) struct Fake {
    dir: TempDir,
    vars: Vec<(String, OsString)>,
    is_tty: [bool; 3],
    zone: TimeZone,
    editor: Option<Rc<EditorFn>>,
    pager_fails: bool,
    opener_fails: bool,
}

/// What the fake editor does to the file it is given.
pub(crate) type EditorFn = dyn Fn(&Path) -> io::Result<()>;

pub(crate) struct Ran {
    pub(crate) code: u8,
    pub(crate) out: String,
    pub(crate) out_bytes: Vec<u8>,
    pub(crate) err: String,
    pub(crate) tui: Vec<(PathBuf, bool)>,
    /// The program, arguments and file of each editor started.
    pub(crate) editor_words: Vec<Vec<OsString>>,
    /// The paths the opener was started on.
    pub(crate) opened: Vec<PathBuf>,
    /// The pagers started: their words and what they were given.
    pub(crate) paged: Vec<(Vec<OsString>, String)>,
}

impl Ran {
    /// The files the editor was started on: the last of each word list.
    pub(crate) fn edited(&self) -> Vec<PathBuf> {
        self.editor_words
            .iter()
            .filter_map(|words| words.last().map(PathBuf::from))
            .collect()
    }
}

impl Fake {
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        // No run reads the machine's /etc/todo/config.
        let global = (
            "TODOTXT_GLOBAL_CFG_FILE".to_owned(),
            dir.path().join("none").into(),
        );
        Self {
            dir,
            vars: vec![global],
            is_tty: [false; 3],
            zone: TimeZone::UTC,
            editor: None,
            pager_fails: false,
            opener_fails: false,
        }
    }

    /// Both terminals, and an editor that runs `edit` on its file.
    pub(crate) fn editing(
        mut self,
        edit: impl Fn(&Path) -> io::Result<()> + 'static,
    ) -> Self {
        self.editor = Some(Rc::new(edit));
        self
    }

    /// An opener that cannot be started.
    pub(crate) fn failing_opener(mut self) -> Self {
        self.opener_fails = true;
        self
    }

    /// A pager that cannot be started.
    pub(crate) fn failing_pager(mut self) -> Self {
        self.pager_fails = true;
        self
    }

    pub(crate) fn path(&self) -> PathBuf {
        fs::canonicalize(self.dir.path()).unwrap()
    }

    pub(crate) fn var(
        mut self,
        name: &str,
        value: impl Into<OsString>,
    ) -> Self {
        self.vars.push((name.to_owned(), value.into()));
        self
    }

    fn zone(mut self, name: &str) -> Self {
        self.zone = TimeZone::get(name).unwrap();
        self
    }

    /// Terminals that show no colour (`NO_COLOR`), so that output stays
    /// plain text.
    pub(crate) fn tty(mut self) -> Self {
        self.is_tty = [true; 3];
        self.var("NO_COLOR", "1")
    }

    /// Which of stdin, stdout and stderr are terminals.
    pub(crate) fn ttys(mut self, is_tty: [bool; 3]) -> Self {
        self.is_tty = is_tty;
        self
    }

    pub(crate) fn file(self, name: &str, text: impl AsRef<[u8]>) -> Self {
        let path = self.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }

    pub(crate) fn run(&self, args: &[&str]) -> Ran {
        let args: Vec<OsString> = args.iter().map(Into::into).collect();
        self.run_os(args)
    }

    /// Runs `args` with `--human --no-config` after them.
    pub(crate) fn human(&self, args: &[&str]) -> Ran {
        self.run(&[args, &["--human", "--no-config"]].concat())
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
        let opened = RefCell::new(Vec::new());
        let paged = RefCell::new(Vec::new());
        let editor_words = RefCell::new(Vec::new());
        let run_editor = |words: &[OsString]| -> HookResult {
            editor_words.borrow_mut().push(words.to_vec());
            let path = Path::new(words.last().unwrap());
            self.editor
                .as_ref()
                .map_or(Ok(()), |edit| edit(path).map_err(|e| e.to_string()))
        };
        let open_path = |path: &Path| -> HookResult {
            opened.borrow_mut().push(path.to_path_buf());
            let why = format!("open {}: not found", path.display());
            if self.opener_fails { Err(why) } else { Ok(()) }
        };
        let run_pager = |words: &[OsString], text: &[u8]| -> HookResult {
            if self.pager_fails {
                return Err("no such program".to_owned());
            }
            let text = String::from_utf8_lossy(text).into_owned();
            paged.borrow_mut().push((words.to_vec(), text));
            Ok(())
        };
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
            zone: self.zone.clone(),
            version: "9.9.9",
            run_editor: &run_editor,
            open_path: &open_path,
            run_pager: &run_pager,
            run_tui: Some(&run_tui),
        });
        Ran {
            code,
            out: String::from_utf8_lossy(&out).into_owned(),
            out_bytes: out,
            err: String::from_utf8(err).unwrap(),
            tui: tui.into_inner(),
            editor_words: editor_words.into_inner(),
            opened: opened.into_inner(),
            paged: paged.into_inner(),
        }
    }
}

/// `text` with each four-character id after `id:` replaced by `ID`, for
/// output that holds a random id.
pub(crate) fn hide_ids(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("id:") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let len = tail.bytes().take_while(u8::is_ascii_alphanumeric).count();
        if len == 4 {
            out.push_str("ID");
        } else {
            out.push_str(&tail[..len]);
        }
        rest = &tail[len..];
    }
    out + rest
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
    for args in [&["-h"][..], &["frobnicate", "--help"]] {
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
            r#"{"version":3,"file":null,"done_file":null,"today":"2026-09-29","#,
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
        let fake = Fake::new().ttys(tty).var("NO_COLOR", "1");
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
    let editor = |_: &[OsString]| -> HookResult { Ok(()) };
    let hook = |_: &Path| -> HookResult { Ok(()) };
    let pager = |_: &[OsString], _: &[u8]| -> HookResult { Ok(()) };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let none = fake.path().join("none");
    let vars = |name: &str| match name {
        "NO_COLOR" => Some(OsString::from("1")),
        "TODOTXT_GLOBAL_CFG_FILE" => Some(none.clone().into()),
        _ => None,
    };
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
        run_editor: &editor,
        open_path: &hook,
        run_pager: &pager,
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
    let ran = fake.human(&["add", "X", "--create"]);
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
    // A folder of its own that discovery stops at, wherever the temp
    // folder is.
    let fake = Fake::new().file(".git/x", "");
    let ran = fake.human(&["add", "X", "--create"]);
    assert_eq!((ran.code, ran.err.as_str()), (0, ""));
    let created = fs::read_to_string(fake.path().join("todo.txt"));
    assert_eq!(hide_ids(&created.unwrap()), "2026-09-29 X id:ID\n");
    let fake = Fake::new().file(".git/x", "");
    let ran = fake.human(&["add", "X"]);
    assert_eq!(ran.code, 2);
    assert!(ran.err.starts_with("lsty: no todo file at "));
}

#[test]
fn an_empty_file_option_counts_as_unset() {
    let fake = Fake::new().file("todo.txt", "x\n");
    let ran = fake.human(&["next", "--file", "", "--json"]);
    assert_eq!(ran.code, 0, "{}", ran.out);
    assert!(ran.out.contains("/todo.txt"), "{}", ran.out);
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
    let mut fake = Fake::new().file(".git/x", "").file("d/x", "");
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
    let pinned = fake.vars.len();
    for (name, vars, hint, found_by) in rows {
        fake.vars.truncate(pinned);
        fake.vars.extend(
            vars.into_iter()
                .map(|(k, v)| (k.to_owned(), OsString::from(v))),
        );
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
        "lsty: argument \"caf\\xe9\" is not valid UTF-8\n\
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
    let fake = fake.var("TODO_FILE", os(&path));
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
        ran.out.starts_with(r#"{"version":3,"file":""#),
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
            "lsty: warning: config.toml: {} not found\n",
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

/// A line that is not UTF-8 is drawn as U+FFFD, in JSON too; `--raw`
/// prints it as it is.
#[test]
fn text_that_is_not_utf8_is_drawn_and_raw_keeps_it() {
    let fake = Fake::new()
        .file("todo.txt", b"caf\xe9 +p\r\nplain\n")
        .var("TODO_FILE", "todo.txt");
    let ran = fake.run(&["ls", "--no-config"]);
    assert_eq!(ran.out, "1 caf\u{fffd} +p\n2 plain\n-- 2 of 2 shown\n");
    let ran = fake.run(&["ls", "--raw", "--no-config"]);
    assert_eq!(ran.out_bytes, b"caf\xe9 +p\nplain\n");
    let ran = fake.run(&["ls", "--json", "--no-config"]);
    assert!(
        ran.out.contains("\"raw\":\"caf\u{fffd} +p\""),
        "{}",
        ran.out
    );
    assert!(ran.out.contains(r#""projects":["p"]"#), "{}", ran.out);
}

const FACTS_TODO: &str = "\
(A) 2026-09-20 Write parser id:parser rec:+1w
(B) 2026-09-21 Fix thing id:fx dep:parser dep:zz due:2026-09-30 t:2026-10-08 \
started:2026-09-29T0700Z owner:agent1 color:red due:2026-11-01
Mine started:2026-09-29T1100Z
x 2026-09-28 2026-09-20 Shipped owner:agent2 pri:C
";

/// The whole `show` block, with the claim in a zone that is not UTC.
#[test]
fn show_prints_every_fact_in_local_time() {
    let fake = Fake::new()
        .zone("America/New_York")
        .file("todo.txt", FACTS_TODO)
        .file("done.txt", "x 2026-09-01 2026-08-30 Old\n")
        .var("TODO_FILE", "todo.txt")
        .var("HOME", "/h");
    let show = |nr: &str| fake.run(&["show", nr, "--no-config"]).out;
    assert_eq!(
        show("1"),
        "1 (A) 2026-09-20 Write parser id:parser rec:+1w\n  \
        text: Write parser\n  state: todo\n  priority: A\n  \
        created: 2026-09-20\n  rec: +1w\n  id: parser\n"
    );
    let second = show("2");
    let want = [
        "  text: Fix thing",
        "  state: doing",
        "  priority: B",
        "  created: 2026-09-21",
        "  due: 2026-09-30",
        "  t: 2026-10-08 (later)",
        "  id: fx",
        "  claimed by agent1 since 2026-09-29 03:00 (5h, stale)",
        "  blocked by: 1 Write parser (id:parser)",
        "  missing prerequisite: zz",
        "  tag: color:red",
        "  extra: due:2026-11-01 (ignored, the first due: counts)",
    ];
    assert_eq!(second.lines().skip(1).collect::<Vec<_>>(), want);
    assert!(
        show("3").contains("  claimed (by you) since 2026-09-29 07:00 (1h)\n")
    );
    let done = show("4");
    assert!(done.contains("  completed: 2026-09-28\n"), "{done}");
    assert!(done.contains("  priority: C\n"), "{done}");
    assert!(done.contains("  finished by agent2\n"), "{done}");
    assert!(!done.contains("tag:"), "{done}");
    let archived = show("done:1");
    assert!(archived.starts_with("done:1 x 2026-09-01"), "{archived}");
}
