use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::*;

/// Finds the names it is given, as `name` itself.
fn finds<'a>(names: &'a [&'a str]) -> impl Fn(&OsStr) -> Option<PathBuf> + 'a {
    move |name| {
        names
            .iter()
            .any(|n| name == *n)
            .then(|| PathBuf::from(name))
    }
}

fn words(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

fn program(name: &str, args: &[&str]) -> Program {
    Program {
        name: name.into(),
        args: words(args),
    }
}

/// Output of a program and the text read from it.
type Bytes = (&'static [u8], &'static [u8]);

/// Text and its base64.
type Encoded = (&'static [u8], &'static str);

struct Choice {
    case: &'static str,
    paste: &'static str,
    path: &'static [&'static str],
    want: Option<(&'static str, &'static [&'static str])>,
}

const CHOICES: [Choice; 5] = [
    Choice {
        case: "43.25 paste_command",
        paste: "tmux save-buffer -",
        path: &[],
        want: Some(("tmux", &["save-buffer", "-"])),
    },
    Choice {
        case: "43.26 first found",
        paste: "",
        path: &["xclip", "pbpaste"],
        want: Some(("xclip", &["-selection", "clipboard", "-o"])),
    },
    Choice {
        case: "43.26 wl-paste first",
        paste: "",
        path: &["pbpaste", "wl-paste"],
        want: Some(("wl-paste", &["--no-newline"])),
    },
    Choice {
        case: "43.26 pbpaste",
        paste: "  ",
        path: &["pbpaste"],
        want: Some(("pbpaste", &[])),
    },
    Choice {
        case: "43.27 nothing found",
        paste: "",
        path: &[],
        want: None,
    },
];

#[test]
fn the_reader_program_is_chosen() {
    for c in CHOICES {
        let got = choose(c.paste, &finds(c.path));
        let want = c.want.map(|(name, args)| program(name, args));
        assert_eq!(got, want, "{}", c.case);
    }
}

#[test]
fn a_paste_command_is_looked_up_in_the_path() {
    let find = |name: &OsStr| {
        (name == "tmux").then(|| PathBuf::from("/opt/bin/tmux"))
    };
    assert_eq!(
        choose("tmux save-buffer -", &find),
        Some(program("/opt/bin/tmux", &["save-buffer", "-"]))
    );
    assert_eq!(
        choose("./mine -x", &find),
        Some(program("./mine", &["-x"])),
        "a name with a slash is used as it is"
    );
}

/// A runner that answers with what it is told and records what it ran.
struct Fake {
    answer: fn() -> io::Result<Vec<u8>>,
    ran: RefCell<Vec<(Program, Duration)>>,
}

impl Fake {
    fn new(answer: fn() -> io::Result<Vec<u8>>) -> Self {
        Self {
            answer,
            ran: RefCell::new(Vec::new()),
        }
    }
}

impl Runner for &Fake {
    fn run(&self, program: &Program, limit: Duration) -> io::Result<Vec<u8>> {
        self.ran.borrow_mut().push((program.clone(), limit));
        (self.answer)()
    }
}

fn reader(fake: &Fake) -> Clipboard<&Fake> {
    Clipboard::new(Some(program("p", &["a"])), fake)
}

#[test]
fn the_program_runs_with_a_limit_of_two_seconds() {
    let fake = Fake::new(|| Ok(b"x".to_vec()));
    assert_eq!(reader(&fake).read().ok(), Some(b"x".to_vec()));
    let ran = fake.ran.borrow();
    assert_eq!(*ran, [(program("p", &["a"]), READ_LIMIT)]);
    assert_eq!(READ_LIMIT, Duration::from_secs(2));
}

#[test]
fn one_final_newline_is_removed() {
    let rows: [Bytes; 4] = [
        (b"text\n", b"text"),
        (b"a\n\n", b"a\n"),
        (b"text", b"text"),
        (b"two\r\n", b"two\r"),
    ];
    for (out, want) in rows {
        let got =
            Clipboard::new(Some(program("p", &[])), TextRunner(out)).read();
        assert_eq!(got.ok().as_deref(), Some(want), "{out:?}");
    }
}

struct TextRunner(&'static [u8]);

impl Runner for TextRunner {
    fn run(&self, _: &Program, _: Duration) -> io::Result<Vec<u8>> {
        Ok(self.0.to_vec())
    }
}

#[test]
fn a_failing_or_timed_out_program_is_an_error() {
    let failing = Fake::new(|| Err(io::Error::other("exit status 1")));
    let err = reader(&failing).read().err();
    assert_eq!(
        err.map(|e| e.message().to_owned()),
        Some("read the clipboard with p: run p: exit status 1".to_owned())
    );
    let slow = Fake::new(|| Err(io::ErrorKind::TimedOut.into()));
    assert!(reader(&slow).read().is_err());
}

#[test]
fn without_a_program_every_read_fails() {
    let fake = Fake::new(|| Ok(Vec::new()));
    let clip = Clipboard::new(None, &fake);
    let err = clip.read().err();
    assert_eq!(
        err.map(|e| e.message().to_owned()),
        Some("no clipboard reader found".to_owned())
    );
    assert!(fake.ran.borrow().is_empty());
}

fn sh(script: &str) -> Program {
    Program {
        name: "sh".into(),
        args: words(&["-c", script]),
    }
}

#[test]
fn a_program_that_never_exits_is_killed_at_the_limit() {
    let marker = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let flag = marker.path().join("survived");
    let script = format!("sleep 1; touch {}", flag.display());
    let start = Instant::now();
    let got = SystemRunner.run(&sh(&script), Duration::from_millis(50));
    let took = start.elapsed();
    assert_eq!(got.err().map(|e| e.kind()), Some(io::ErrorKind::TimedOut));
    assert!(took < Duration::from_millis(900), "took {took:?}");
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!flag.exists(), "the program was not stopped");
}

#[test]
fn a_program_that_closes_its_output_and_keeps_running_is_killed() {
    let marker = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let flag = marker.path().join("survived");
    let script = format!("exec >&-; sleep 1; touch {}", flag.display());
    let start = Instant::now();
    let got = SystemRunner.run(&sh(&script), Duration::from_millis(50));
    let took = start.elapsed();
    assert_eq!(got.err().map(|e| e.kind()), Some(io::ErrorKind::TimedOut));
    assert!(took < Duration::from_millis(900), "took {took:?}");
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!flag.exists(), "the program was not stopped");
}

#[test]
fn a_program_that_closes_its_output_is_waited_for() {
    let got = SystemRunner.run(&sh("exec >&-; sleep 0.2"), READ_LIMIT);
    assert_eq!(got.map_err(|e| e.to_string()), Ok(Vec::new()));
}

#[test]
fn a_program_that_prints_late_gives_its_output() {
    let got = SystemRunner.run(&sh("sleep 0.2; printf late"), READ_LIMIT);
    assert_eq!(got.map_err(|e| e.to_string()), Ok(b"late".to_vec()));
}

#[test]
fn output_held_briefly_after_the_program_ended_is_kept() {
    assert_eq!(system::GRACE, Duration::from_millis(100));
    let script = "(sleep 0.3; printf late) & exit 0";
    let got = system::run_within(&sh(script), READ_LIMIT, READ_LIMIT);
    assert_eq!(got.map_err(|e| e.to_string()), Ok(b"late".to_vec()));
}

#[test]
fn a_limit_too_large_for_a_deadline_means_no_limit() {
    let got = SystemRunner.run(&sh("printf ok"), Duration::MAX);
    assert_eq!(got.map_err(|e| e.to_string()), Ok(b"ok".to_vec()));
}

#[test]
fn a_program_that_prints_gives_its_output() {
    let got = SystemRunner.run(&sh("printf 'text\\n'"), READ_LIMIT);
    assert_eq!(got.map_err(|e| e.to_string()), Ok(b"text\n".to_vec()));
}

#[test]
fn a_program_that_fails_is_an_error() {
    let got = SystemRunner.run(&sh("echo out; exit 3"), READ_LIMIT);
    assert_eq!(
        got.err().map(|e| e.to_string()),
        Some("exit status 3".to_owned())
    );
    let missing = Program {
        name: "no-such-clipboard-program".into(),
        args: Vec::new(),
    };
    assert_eq!(
        SystemRunner
            .run(&missing, READ_LIMIT)
            .err()
            .map(|e| e.to_string()),
        Some(
            "exec: \"no-such-clipboard-program\": executable file not found \
             in $PATH"
                .to_owned()
        )
    );
    let killed = SystemRunner.run(&sh("kill -9 $$"), READ_LIMIT);
    assert_eq!(
        killed.err().map(|e| e.to_string()),
        Some("signal: killed".to_owned())
    );
}

#[test]
fn osc52_is_base64_between_the_escape_and_a_bell() {
    let rows: [Encoded; 6] = [
        (b"", ""),
        (b"f", "Zg=="),
        (b"fo", "Zm8="),
        (b"foo", "Zm9v"),
        (b"foob", "Zm9vYg=="),
        (b"\xff\xfe\n", "//4K"),
    ];
    for (text, encoded) in rows {
        let want = format!("\x1b]52;c;{encoded}\x07");
        assert_eq!(osc52(text), want.as_bytes(), "{text:?}");
    }
}

#[test]
fn a_grandchild_holding_the_output_fails_the_read_soon() {
    let start = Instant::now();
    let got =
        SystemRunner.run(&sh("echo x; sleep 3 &"), Duration::from_secs(2));
    assert!(got.is_err());
    assert!(start.elapsed() < Duration::from_millis(1000));
}

#[test]
fn a_reader_tells_the_program_it_runs() {
    let fake = Fake::new(|| Ok(Vec::new()));
    assert_eq!(reader(&fake).program(), Some(&program("p", &["a"])));
    assert_eq!(Clipboard::new(None, &fake).program(), None);
}
