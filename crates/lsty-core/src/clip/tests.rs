use std::{
    cell::RefCell,
    ffi::OsString,
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

use super::*;

/// Finds the names it is given, as `name` itself.
fn finds<'a>(
    names: &'a [&'a str],
) -> impl Fn(&OsStr) -> Result<PathBuf, String> + 'a {
    move |name| {
        name.to_str()
            .filter(|n| names.contains(n))
            .map(PathBuf::from)
            .ok_or_else(|| "not found".to_owned())
    }
}

fn words(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

fn program(name: &str, args: &[&str]) -> Program {
    Program {
        file: name.into(),
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
        path: &["tmux"],
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
        let got = choose(c.paste, &READ, &finds(c.path));
        let want = c.want.map(|(name, args)| program(name, args));
        assert_eq!(got.ok(), want, "{}", c.case);
    }
}

#[test]
fn a_paste_command_is_looked_up_in_the_path() {
    let find = |name: &OsStr| match name.to_str() {
        Some("tmux") => Ok(PathBuf::from("/opt/bin/tmux")),
        _ => Err("refused".to_owned()),
    };
    assert_eq!(
        choose("tmux save-buffer -", &READ, &find),
        Ok(Program {
            file: "/opt/bin/tmux".into(),
            ..program("tmux", &["save-buffer", "-"])
        })
    );
    assert_eq!(
        choose("planted -x", &READ, &find),
        Err("read the clipboard with planted: run planted: refused".into()),
        "a name the lookup refuses is never run, found or not"
    );
}

/// What a fake runner answers to a program.
type Answer = fn(&Program) -> io::Result<Vec<u8>>;

/// A runner that answers with what it is told and records what it ran.
struct Fake {
    answer: Answer,
    inputs: RefCell<Vec<Option<Vec<u8>>>>,
    ran: RefCell<Vec<(Program, Duration)>>,
}

impl Fake {
    fn new(answer: Answer) -> Self {
        Self {
            answer,
            inputs: RefCell::new(Vec::new()),
            ran: RefCell::new(Vec::new()),
        }
    }
}

impl Runner for &Fake {
    fn run(
        &self,
        program: &Program,
        input: Option<&[u8]>,
        limit: Duration,
    ) -> io::Result<Vec<u8>> {
        self.ran.borrow_mut().push((program.clone(), limit));
        self.inputs.borrow_mut().push(input.map(<[u8]>::to_vec));
        (self.answer)(program)
    }
}

fn reader(fake: &Fake) -> Clipboard<&Fake> {
    Clipboard::new(Ok(program("p", &["a"])), fake)
}

#[test]
fn the_program_runs_with_a_limit_of_two_seconds() {
    let fake = Fake::new(|_| Ok(b"x".to_vec()));
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
            Clipboard::new(Ok(program("p", &[])), TextRunner(out)).read();
        assert_eq!(got.ok().as_deref(), Some(want), "{out:?}");
    }
}

struct TextRunner(&'static [u8]);

impl Runner for TextRunner {
    fn run(
        &self,
        _: &Program,
        _: Option<&[u8]>,
        _: Duration,
    ) -> io::Result<Vec<u8>> {
        Ok(self.0.to_vec())
    }
}

#[test]
fn a_failing_or_timed_out_program_is_an_error() {
    let failing = Fake::new(|_| Err(io::Error::other("exit status 1")));
    let err = reader(&failing).read().err();
    assert_eq!(
        err.map(|e| e.message().to_owned()),
        Some("read the clipboard with p: run p: exit status 1".to_owned())
    );
    let slow = Fake::new(|_| Err(io::ErrorKind::TimedOut.into()));
    assert!(reader(&slow).read().is_err());
}

#[test]
fn a_failure_names_the_program_as_typed() {
    let failing = Fake::new(|_| Err(io::Error::other("exit status 1")));
    let find = |_: &OsStr| Ok(PathBuf::from("/opt/bin/tmux"));
    let clip = Clipboard::new(choose("tmux -x", &READ, &find), &failing);
    assert_eq!(
        clip.read().err().map(|e| e.message().to_owned()),
        Some(
            "read the clipboard with tmux: run tmux: exit status 1".to_owned()
        )
    );
}

#[test]
fn a_program_runs_from_the_file_found_under_the_name_typed() {
    let program = Program {
        file: "/bin/sh".into(),
        name: "typed".into(),
        args: words(&["-c", "echo $0"]),
    };
    let got = OsRunner.run(&program, None, READ_LIMIT);
    assert_eq!(got.map_err(|e| e.to_string()), Ok(b"typed\n".to_vec()));
}

#[test]
fn without_a_program_every_read_fails() {
    let fake = Fake::new(|_| Ok(Vec::new()));
    let clip = Clipboard::new(choose("", &READ, &finds(&[])), &fake);
    let err = clip.read().err();
    assert_eq!(
        err.map(|e| e.message().to_owned()),
        Some("no clipboard reader found".to_owned())
    );
    assert!(fake.ran.borrow().is_empty());
}

fn sh(script: &str) -> Program {
    Program {
        file: "/bin/sh".into(),
        name: "sh".into(),
        args: words(&["-c", script]),
    }
}

#[test]
fn a_program_past_the_limit_is_killed() {
    let marker = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let prefixes = ["", "exec >&-; "];
    for (i, prefix) in prefixes.iter().enumerate() {
        let flag = marker.path().join(i.to_string());
        let script = format!("{prefix}sleep 1; touch {}", flag.display());
        let start = Instant::now();
        let got = OsRunner.run(&sh(&script), None, Duration::from_millis(50));
        let took = start.elapsed();
        let kind = got.err().map(|e| e.kind());
        assert_eq!(kind, Some(io::ErrorKind::TimedOut), "{prefix:?}");
        assert!(took < Duration::from_millis(900), "{prefix:?}: {took:?}");
    }
    std::thread::sleep(Duration::from_millis(1200));
    for (i, prefix) in prefixes.iter().enumerate() {
        let flag = marker.path().join(i.to_string());
        assert!(!flag.exists(), "{prefix:?}: the program was not stopped");
    }
}

#[test]
fn a_program_that_closes_its_output_is_waited_for() {
    let got =
        OsRunner.run(&sh("exec >&-; sleep 0.2; exit 3"), None, READ_LIMIT);
    assert_eq!(
        got.err().map(|e| e.to_string()),
        Some("exit status 3".to_owned())
    );
}

#[test]
fn a_program_that_prints_late_gives_its_output() {
    let got = OsRunner.run(&sh("sleep 0.2; printf late"), None, READ_LIMIT);
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
    let got = OsRunner.run(&sh("printf ok"), None, Duration::MAX);
    assert_eq!(got.map_err(|e| e.to_string()), Ok(b"ok".to_vec()));
}

#[test]
fn a_program_that_prints_gives_its_output() {
    let got = OsRunner.run(&sh("printf 'text\\n'"), None, READ_LIMIT);
    assert_eq!(got.map_err(|e| e.to_string()), Ok(b"text\n".to_vec()));
}

#[test]
fn a_program_that_fails_is_an_error() {
    let got = OsRunner.run(&sh("echo out; exit 3"), None, READ_LIMIT);
    assert_eq!(
        got.err().map(|e| e.to_string()),
        Some("exit status 3".to_owned())
    );
    let missing = program("no-such-clipboard-program", &[]);
    assert_eq!(
        OsRunner
            .run(&missing, None, READ_LIMIT)
            .err()
            .map(|e| e.to_string()),
        Some(
            "exec: \"no-such-clipboard-program\": executable file not found \
             in $PATH"
                .to_owned()
        )
    );
    let killed = OsRunner.run(&sh("kill -9 $$"), None, READ_LIMIT);
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
        OsRunner.run(&sh("echo x; sleep 3 &"), None, Duration::from_secs(2));
    assert!(got.is_err());
    assert!(start.elapsed() < Duration::from_millis(1000));
}

#[test]
fn a_reader_tells_the_program_it_runs() {
    let fake = Fake::new(|_| Ok(Vec::new()));
    assert_eq!(reader(&fake).program, Ok(program("p", &["a"])));
    assert!(
        Clipboard::new(choose("", &READ, &finds(&[])), &fake)
            .program
            .is_err()
    );
}

/// A runner answer that fails with `why`.
fn failing(why: &'static str) -> io::Error {
    io::Error::other(why)
}

/// The clipboard that the settings give, with programs found in `path`.
fn board<'a>(
    fake: &'a Fake,
    (paste, copy): (&str, &str),
    tmux: bool,
    path: &[&str],
) -> Clipboard<&'a Fake> {
    Clipboard::with(fake, paste, copy, tmux, &finds(path))
}

/// What the fake ran, as the words of each command line.
fn ran(fake: &Fake) -> Vec<String> {
    let line = |(p, _): &(Program, Duration)| {
        let words = [p.name.clone()].into_iter().chain(p.args.clone());
        let words: Vec<String> =
            words.map(|w| w.to_string_lossy().into_owned()).collect();
        words.join(" ")
    };
    fake.ran.borrow().iter().map(line).collect()
}

struct Write {
    case: &'static str,
    copy: &'static str,
    tmux: bool,
    path: &'static [&'static str],
    fails: bool,
    ran: &'static [&'static str],
    err: Option<&'static str>,
}

const WRITES: [Write; 11] = [
    Write {
        case: "pbcopy",
        copy: "",
        tmux: false,
        path: &["pbcopy", "wl-copy", "xclip"],
        fails: false,
        ran: &["pbcopy"],
        err: None,
    },
    Write {
        case: "wl-copy",
        copy: "",
        tmux: false,
        path: &["xclip", "wl-copy"],
        fails: false,
        ran: &["wl-copy"],
        err: None,
    },
    Write {
        case: "xclip",
        copy: "",
        tmux: false,
        path: &["xclip"],
        fails: false,
        ran: &["xclip -selection clipboard"],
        err: None,
    },
    Write {
        case: "none found",
        copy: "",
        tmux: false,
        path: &[],
        fails: false,
        ran: &[],
        err: None,
    },
    Write {
        case: "tmux only",
        copy: "",
        tmux: true,
        path: &["tmux"],
        fails: false,
        ran: &["tmux load-buffer -"],
        err: None,
    },
    Write {
        case: "a program, then tmux",
        copy: "",
        tmux: true,
        path: &["tmux", "wl-copy"],
        fails: false,
        ran: &["wl-copy", "tmux load-buffer -"],
        err: None,
    },
    Write {
        case: "tmux is not in PATH",
        copy: "",
        tmux: true,
        path: &[],
        fails: false,
        ran: &[],
        err: None,
    },
    Write {
        case: "copy_command alone, even inside tmux",
        copy: "mine -x",
        tmux: true,
        path: &["mine", "pbcopy", "tmux"],
        fails: false,
        ran: &["mine -x"],
        err: None,
    },
    Write {
        case: "a known program that fails is silent",
        copy: "",
        tmux: true,
        path: &["pbcopy", "tmux"],
        fails: true,
        ran: &["pbcopy", "tmux load-buffer -"],
        err: None,
    },
    Write {
        case: "copy_command that fails is an error",
        copy: "mine",
        tmux: true,
        path: &["mine", "tmux"],
        fails: true,
        ran: &["mine"],
        err: Some("write the clipboard with mine: run mine: boom"),
    },
    Write {
        case: "copy_command that is not found is an error",
        copy: "planted",
        tmux: false,
        path: &["pbcopy"],
        fails: false,
        ran: &[],
        err: Some("write the clipboard with planted: run planted: not found"),
    },
];

#[test]
fn the_text_goes_to_the_writers() {
    let text: &[u8] = b"a\nb \xff";
    for w in WRITES {
        let fake = if w.fails {
            Fake::new(|_| Err(failing("boom")))
        } else {
            Fake::new(|_| Ok(Vec::new()))
        };
        let got = board(&fake, ("", w.copy), w.tmux, w.path).write(text);
        let err = got.err().map(|e| e.message().to_owned());
        assert_eq!(err.as_deref(), w.err, "{}", w.case);
        assert_eq!(ran(&fake), w.ran, "{}", w.case);
        let inputs = fake.inputs.borrow();
        assert!(
            inputs.iter().all(|i| i.as_deref() == Some(text)),
            "{}",
            w.case
        );
        let limits = fake.ran.borrow().iter().all(|r| r.1 == WRITE_LIMIT);
        assert!(limits, "{}", w.case);
    }
    assert!(WRITE_LIMIT < READ_LIMIT);
}

struct Read {
    case: &'static str,
    paste: &'static str,
    tmux: bool,
    path: &'static [&'static str],
    answer: Answer,
    ran: &'static [&'static str],
    want: Result<&'static str, &'static str>,
}

/// Only tmux answers.
fn tmux_answers(p: &Program) -> io::Result<Vec<u8>> {
    if p.name == "tmux" {
        Ok(b"buffer\n".to_vec())
    } else {
        Err(failing("exit status 1"))
    }
}

/// Everything fails, tmux with another status.
fn all_fail(p: &Program) -> io::Result<Vec<u8>> {
    Err(failing(if p.name == "tmux" {
        "exit status 2"
    } else {
        "exit status 1"
    }))
}

/// Only the clipboard reader answers.
fn clipboard_answers(p: &Program) -> io::Result<Vec<u8>> {
    if p.name == "tmux" {
        Err(failing("exit status 2"))
    } else {
        Ok(b"clip".to_vec())
    }
}

const READS: [Read; 7] = [
    Read {
        case: "a reader that fails, then tmux answers",
        paste: "",
        tmux: true,
        path: &["xclip", "tmux"],
        answer: tmux_answers,
        ran: &["xclip -selection clipboard -o", "tmux save-buffer -"],
        want: Ok("buffer"),
    },
    Read {
        case: "no reader, tmux answers",
        paste: "",
        tmux: true,
        path: &["tmux"],
        answer: tmux_answers,
        ran: &["tmux save-buffer -"],
        want: Ok("buffer"),
    },
    Read {
        case: "no reader, no tmux",
        paste: "",
        tmux: false,
        path: &["tmux"],
        answer: tmux_answers,
        ran: &[],
        want: Err("no clipboard reader found"),
    },
    Read {
        case: "both fail: the first message",
        paste: "",
        tmux: true,
        path: &["xclip", "tmux"],
        answer: all_fail,
        ran: &["xclip -selection clipboard -o", "tmux save-buffer -"],
        want: Err("read the clipboard with xclip: run xclip: exit status 1"),
    },
    Read {
        case: "no reader, tmux fails",
        paste: "",
        tmux: true,
        path: &["tmux"],
        answer: all_fail,
        ran: &["tmux save-buffer -"],
        want: Err("no clipboard reader found"),
    },
    Read {
        case: "paste_command that fails, inside tmux: no fall-back",
        paste: "mine",
        tmux: true,
        path: &["mine", "tmux"],
        answer: tmux_answers,
        ran: &["mine"],
        want: Err("read the clipboard with mine: run mine: exit status 1"),
    },
    Read {
        case: "a reader that answers: tmux is not asked",
        paste: "",
        tmux: true,
        path: &["xclip", "tmux"],
        answer: clipboard_answers,
        ran: &["xclip -selection clipboard -o"],
        want: Ok("clip"),
    },
];

#[test]
fn a_read_falls_back_to_the_tmux_buffer() {
    for r in READS {
        let fake = Fake::new(r.answer);
        let got = board(&fake, (r.paste, ""), r.tmux, r.path).read();
        let got = got
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| e.message().to_owned());
        let want = r.want.map(str::to_owned).map_err(str::to_owned);
        assert_eq!(got, want, "{}", r.case);
        assert_eq!(ran(&fake), r.ran, "{}", r.case);
    }
}

fn wrote(dir: &std::path::Path, script: &str, text: &[u8]) -> io::Result<()> {
    let file = dir.join("in");
    let script = script.replace("FILE", &file.display().to_string());
    OsRunner.run(&sh(&script), Some(text), READ_LIMIT).map(drop)
}

#[test]
fn a_writer_is_given_the_text_and_waited_for() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let text = vec![b'x'; 1 << 20];
    assert!(wrote(dir.path(), "cat > FILE", &text).is_ok());
    let got = std::fs::read(dir.path().join("in")).unwrap_or_default();
    assert_eq!(got, text);
    let failed = OsRunner.run(&sh("exit 3"), Some(b"t"), READ_LIMIT);
    assert_eq!(
        failed.err().map(|e| e.to_string()),
        Some("exit status 3".to_owned())
    );
}

#[test]
fn a_writer_that_never_reads_is_stopped_at_the_limit() {
    let start = Instant::now();
    let text = vec![b'x'; 1 << 20];
    let got =
        OsRunner.run(&sh("sleep 3"), Some(&text), Duration::from_millis(50));
    assert_eq!(got.err().map(|e| e.kind()), Some(io::ErrorKind::TimedOut));
    assert!(start.elapsed() < Duration::from_millis(900));
}

#[test]
fn text_written_and_read_back_through_programs_is_the_same() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = dir.path().join("board").display().to_string();
    let board = Clipboard {
        program: Ok(sh(&format!("cat {file}"))),
        fallback: None,
        writers: Ok(vec![sh(&format!("cat > {file}"))]),
        loud: true,
        runner: OsRunner,
    };
    for text in ["one", "a\nb", "a\n\nb \u{e9}", "(A) x +p\n(B) y"] {
        assert!(board.write(text.as_bytes()).is_ok(), "{text:?}");
        assert_eq!(
            board.read().ok(),
            Some(text.as_bytes().to_vec()),
            "{text:?}"
        );
    }
}
