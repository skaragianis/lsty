//! Every command, run on a hostile todo file and done file, writes no
//! control character: a command added to the table is run too.

use std::fs;

use super::{commands::COMMANDS, run_tests::Fake};

/// Text that a terminal would act on: ESC sequences, BEL, DEL, a C1
/// character (`C2 9B`), a tab and a carriage return.
const PAYLOAD: &str = "\x1b[2J\x1b]0;t\x07\u{9b}31m\x7f\t\r";

/// Bytes that are not UTF-8, among them C1 controls on their own.
const RAW: &[u8] = b"\xff\x9b\x80";

/// The argument shapes every command is run with, to reach its results
/// and its errors. `P` stands for the payload.
const SHAPES: [&[&str]; 14] = [
    &[],
    &["1"],
    &["P"],
    &["1", "P"],
    &["2", "1"],
    &["1", "on", "2"],
    &["1", "2026-10-01"],
    &["A", "1"],
    &["1", "A"],
    &["x", "--print-id"],
    &["--start", "--print-id"],
    &["--after", "1", "2"],
    &["4", "tag:P"],
    &["zz"],
];

/// The todo file: the payload in a text, project, context, tag, link,
/// id, dependency and owner, and raw bytes.
fn todo() -> Vec<u8> {
    let p = PAYLOAD;
    let mut text = format!(
        "(A) 2026-09-20 call {p} +pro{p} @ctx{p} k{p}:v{p} due:2026-10-01 \
         id:i{p}\ndep:i{p} Read https://x.org/{p} file:f{p}.md owner:o{p} \
         started:2026-09-29T0700Z\nx 2026-09-28 done {p} {p}:1\nplain "
    )
    .into_bytes();
    text.extend_from_slice(RAW);
    text.extend_from_slice(b" t:2026-10-05 rec:+1w\n");
    text
}

fn fake(tty: bool) -> Fake {
    let more = format!(" more {}\n", PAYLOAD.replace('\r', ""));
    let fake = Fake::new();
    // the editor's file stays when the edit is refused
    let tmp = fake.path();
    let fake = fake
        .var("TMPDIR", tmp)
        .file("todo.txt", todo())
        .file("done.txt", format!("x 2026-09-01 old {PAYLOAD} +p\n"))
        .editing(move |path| {
            let mut text = fs::read(path)?;
            text.retain(|b| *b != b'\r' && *b != b'\n');
            text.extend_from_slice(more.as_bytes());
            fs::write(path, text)
        })
        .var("TODO_FILE", "todo.txt");
    if tty { fake.tty() } else { fake }
}

/// `out` and `err` hold nothing a terminal acts on once the colours of
/// the painter are taken out.
fn assert_clean(what: &str, code: u8, out: &[u8], err: &str) {
    let mut bytes = out.to_vec();
    bytes.extend_from_slice(err.as_bytes());
    assert!(std::str::from_utf8(&bytes).is_ok(), "{what}: not UTF-8");
    let text = String::from_utf8_lossy(&bytes);
    let mut left = text.as_ref();
    while let Some(at) = left.find('\u{1b}') {
        let sgr = left[at + 1..].strip_prefix('[').and_then(|rest| {
            let end = rest.find(|c: char| !c.is_ascii_digit() && c != ';')?;
            rest[end..].starts_with('m').then_some(at + 3 + end)
        });
        let Some(next) = sgr else {
            panic!("{what} (exit {code}): a raw ESC in {text:?}");
        };
        left = &left[next..];
    }
    let acts = |c: char| c.is_control() && c != '\n' && c != '\u{1b}';
    assert!(!text.contains(acts), "{what} (exit {code}): {text:?}");
}

fn run(what: &str, fake: &Fake, args: &[&str]) {
    let ran = fake.run(args);
    assert_clean(what, ran.code, &ran.out_bytes, &ran.err);
}

#[test]
fn no_command_writes_a_control_character() {
    let variants: [(bool, &[&str]); 5] = [
        (false, &["--human", "--color=never"]),
        (false, &["--human", "--color=always"]),
        (false, &["--human", "--json"]),
        (false, &["--owner", "agent1", "--color=always"]),
        // an editor and the other terminals
        (true, &["--human"]),
    ];
    for command in &COMMANDS {
        for shape in SHAPES {
            for (tty, extra) in variants {
                let mut args = vec![command.name];
                args.extend(shape.iter().map(|a| match *a {
                    "P" => PAYLOAD,
                    a if a.starts_with("tag:") => "tag:\x1b[2J",
                    a => a,
                }));
                args.push("--no-config");
                args.extend(extra);
                run(&format!("{args:?}"), &fake(tty), &args);
            }
        }
    }
}

#[test]
fn text_from_the_environment_and_the_files_is_drawn_too() {
    let p = PAYLOAD;
    let cases: [(&str, &str); 6] = [
        ("TODO_FILE", "gone"),
        ("DONE_FILE", "gone"),
        ("LSTY_TODAY", "q"),
        ("LSTY_NOW", "q"),
        ("LSTY_OWNER", "agent"),
        ("LSTY_CONFIG", "gone"),
    ];
    for (name, value) in cases {
        for args in [&["ls"][..], &["add", "x"], &["next", "--start"]] {
            let fake =
                fake(false).var(name, format!("{value}{p}\nlsty: forged"));
            let mut args = args.to_vec();
            args.push("--human");
            let ran = fake.run(&args);
            assert_clean(name, ran.code, &ran.out_bytes, &ran.err);
        }
    }
    let ran =
        fake(false).run(&["ls", "--file", &format!("gone{p}"), "--no-config"]);
    assert_clean("--file", ran.code, &ran.out_bytes, &ran.err);
}

#[test]
fn a_line_break_in_echoed_text_never_starts_a_line() {
    let forged = "x\nlsty: forged";
    let file = format!("{forged}/t.txt");
    let named = fake(false)
        .var("LSTY_TODAY", forged)
        .run(&["ls", "--human"]);
    let flagged =
        fake(false).run(&["ls", "--file", &file, "--no-config", "--human"]);
    for ran in [named, flagged] {
        let out = String::from_utf8_lossy(&ran.out_bytes).into_owned();
        assert!(ran.err.contains("x\u{240a}lsty: forged"), "{}", ran.err);
        for line in out.lines().chain(ran.err.lines()) {
            assert!(!line.starts_with("lsty: forged"), "{line:?}");
        }
    }
}
