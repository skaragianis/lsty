//! `edit` through the whole command line, with a fake editor.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use lsty_core::errs::{Code, Error};
use lsty_core::lock::{Cancel, Mode, Want, acquire};

use super::editor::keep_text;
use super::run_tests::{Fake, Ran};

const FIXTURE: &str = "Call client +work\nBuy milk\n";

fn fake(todo: impl AsRef<[u8]>) -> Fake {
    let fake = Fake::new();
    let tmp = fake.path().join("tmp");
    fake.tty()
        .file("todo.txt", todo)
        .file("tmp/.keep", "")
        .var("TODO_FILE", "todo.txt")
        .var("TMPDIR", tmp)
}

fn typing(fake: Fake, text: &'static str) -> Fake {
    fake.editing(move |path| fs::write(path, text))
}

fn todo(fake: &Fake) -> String {
    fs::read_to_string(fake.path().join("todo.txt")).unwrap()
}

fn temp_files(fake: &Fake) -> Vec<PathBuf> {
    let mut found: Vec<_> = fs::read_dir(fake.path().join("tmp"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("lsty-edit-") && name.ends_with(".txt")
        })
        .collect();
    found.sort();
    found
}

fn edit(fake: &Fake, args: &[&str]) -> Ran {
    let mut line = vec!["edit"];
    line.extend(args);
    line.push("--no-config");
    fake.run(&line)
}

/// The file at the start, the file after, the message.
type Change = (&'static str, &'static str, &'static str);

/// Name, what the editor writes, and the change.
const CHANGES: [(&str, &str, Change); 5] = [
    (
        "27.13 changed",
        "Ring client +work\n",
        (
            "Call client +work\nBuy milk\n",
            "Ring client +work\nBuy milk\n",
            "1 Ring client +work\n",
        ),
    ),
    (
        "27.14 nothing changed",
        "Call client +work\n",
        (FIXTURE, FIXTURE, "1 unchanged\n"),
    ),
    ("27.14 emptied", "", (FIXTURE, FIXTURE, "1 unchanged\n")),
    (
        "27.15 two lines are joined",
        "Ring\nclient\n",
        (FIXTURE, "Ring client\nBuy milk\n", "1 Ring client\n"),
    ),
    (
        "27.21 relative date",
        "Call client due:fri\n",
        (
            FIXTURE,
            "Call client due:2026-10-02\nBuy milk\n",
            "1 Call client due:2026-10-02\n",
        ),
    ),
];

#[test]
fn the_item_takes_what_the_editor_leaves() {
    for (name, typed, (start, want, message)) in CHANGES {
        let fake = typing(fake(start), typed);
        let ran = edit(&fake, &["1"]);
        assert_eq!(ran.code, 0, "{name}: {}", ran.err);
        assert_eq!(todo(&fake), want, "{name}");
        assert_eq!(ran.out, message, "{name}");
        assert_eq!(temp_files(&fake), Vec::<PathBuf>::new(), "{name}");
    }
}

#[test]
fn a_dumb_terminal_still_edits() {
    let fake = typing(fake(FIXTURE).var("TERM", "dumb"), "Ring client\n");
    let ran = edit(&fake, &["1"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(todo(&fake), "Ring client\nBuy milk\n");
}

#[test]
fn the_editor_sees_the_raw_line_in_a_private_file() {
    let seen = Rc::new(std::cell::RefCell::new(None));
    let spy = Rc::clone(&seen);
    let fake = fake(FIXTURE).editing(move |path| {
        let mode = fs::metadata(path)?.permissions().mode() & 0o777;
        *spy.borrow_mut() = Some((fs::read_to_string(path)?, mode));
        Ok(())
    });
    let ran = edit(&fake, &["1"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        *seen.borrow(),
        Some(("Call client +work\n".to_owned(), 0o600))
    );
    let edited = ran.edited();
    let [path] = edited.as_slice() else {
        panic!("{edited:?}");
    };
    assert_eq!(path.parent().unwrap(), fake.path().join("tmp"));
    let name = path.file_name().unwrap().to_string_lossy();
    assert!(
        name.starts_with("lsty-edit-") && name.ends_with(".txt"),
        "{name}"
    );
}

#[test]
fn the_editor_is_read_from_the_variables_and_split_into_words() {
    let with = |setting: Option<&str>| {
        let mut fake = Fake::new()
            .tty()
            .file("todo.txt", FIXTURE)
            .var("TODO_FILE", "todo.txt");
        if let Some(setting) = setting {
            fake = fake.var("EDITOR", setting);
        }
        let ran = edit(&fake, &["1"]);
        assert_eq!(ran.code, 0, "{}", ran.err);
        let words = ran.editor_words.into_iter().next().unwrap();
        let (_file, rest) = words.split_last().unwrap();
        rest.iter()
            .map(|w| w.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(with(Some("code --wait")), ["code", "--wait"]);
    assert_eq!(with(Some("  nano  ")), ["nano"]);
    assert_eq!(with(Some("")), ["vi"]);
    assert_eq!(with(None), ["vi"]);
}

#[test]
fn without_tmpdir_the_temp_file_is_in_tmp() {
    let fake = Fake::new()
        .tty()
        .file("todo.txt", FIXTURE)
        .var("TODO_FILE", "todo.txt")
        .var("TMPDIR", "");
    let ran = edit(&fake, &["1"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.edited()[0].parent().unwrap(), Path::new("/tmp"));
}

/// Name, the other writer's file, what the editor writes, the exit code,
/// a part of stderr, the file after.
type Failure = (
    &'static str,
    &'static str,
    &'static str,
    u8,
    &'static str,
    &'static str,
);

const FAILURES: [Failure; 6] = [
    (
        "27.16 changed by another writer",
        "Phone client\nBuy milk\n",
        "Ring client\n",
        3,
        "1 changed while it was being edited",
        "Phone client\nBuy milk\n",
    ),
    (
        "27.18 deleted",
        "Buy milk\n",
        "Ring client\n",
        3,
        "changed while it was being edited",
        "Buy milk\n",
    ),
    (
        "27.19 stamp",
        "",
        "x 2026-09-29 Call client\n",
        3,
        "a text edit can't change a stamp",
        FIXTURE,
    ),
    (
        "27.20 id changed",
        "",
        "Call client +work id:zzzz\n",
        3,
        "an id is never changed",
        FIXTURE,
    ),
    ("invalid value", "", "Call due:never\n", 2, "lsty:", FIXTURE),
    (
        "a cleared line",
        "\nBuy milk\n",
        "Ring\n",
        3,
        "changed while it was being edited",
        "\nBuy milk\n",
    ),
];

#[test]
fn a_failure_keeps_the_text_and_names_the_file() {
    for (name, other, typed, exit, message, after) in FAILURES {
        let start = if name.contains("27.20") {
            "Call client +work id:k3f9\nBuy milk\n"
        } else {
            FIXTURE
        };
        let after = if name.contains("27.20") { start } else { after };
        let fake = fake(start);
        let todo_path = fake.path().join("todo.txt");
        let fake = fake.editing(move |path| {
            if !other.is_empty() {
                fs::write(&todo_path, other)?;
            }
            fs::write(path, typed)
        });
        let ran = edit(&fake, &["1"]);
        assert_eq!(ran.code, exit, "{name}: {}", ran.err);
        assert_eq!(todo(&fake), after, "{name}");
        let files = temp_files(&fake);
        assert_eq!(files.len(), 1, "{name}");
        assert!(ran.err.contains(message), "{name}: {}", ran.err);
        let hint = format!("your text is in {}", files[0].display());
        assert!(ran.err.trim_end().ends_with(&hint), "{name}: {}", ran.err);
        assert_eq!(fs::read_to_string(&files[0]).unwrap(), typed, "{name}");
    }
}

#[test]
fn an_insert_above_moves_the_edit_with_the_item() {
    let fake = fake(FIXTURE);
    let todo_path = fake.path().join("todo.txt");
    let fake = fake.editing(move |path| {
        fs::write(&todo_path, "New\nCall client +work\nBuy milk\n")?;
        fs::write(path, "Ring client\n")
    });
    let ran = edit(&fake, &["1"]);
    assert_eq!((ran.code, ran.out.as_str()), (0, "1 Ring client\n"));
    assert_eq!(todo(&fake), "New\nRing client\nBuy milk\n");
    assert_eq!(temp_files(&fake).len(), 0);
}

#[test]
fn an_editor_that_fails_changes_nothing_and_keeps_the_file() {
    let fake =
        fake(FIXTURE).editing(|_| Err(io::Error::other("exit status 1")));
    let ran = edit(&fake, &["1"]);
    assert_eq!(ran.code, 1, "27.24");
    assert_eq!(todo(&fake), FIXTURE);
    let files = temp_files(&fake);
    assert_eq!(files.len(), 1);
    assert_eq!(
        ran.err,
        format!(
            "lsty: exit status 1\nhint: your text is in {}\n",
            files[0].display()
        )
    );
}

const HELD: &str =
    "Call client id:k3f9 started:2026-09-29T0800Z owner:agent2\n";
const HELD_TYPED: &str =
    "Ring client id:k3f9 started:2026-09-29T0800Z owner:agent2\n";

#[test]
fn a_held_claim_stops_an_agent_before_the_editor_starts() {
    let fake = typing(fake(HELD), HELD_TYPED);
    let ran = edit(&fake, &["id:k3f9", "--owner", "agent1"]);
    assert_eq!(ran.code, 3, "27.22: {}", ran.err);
    assert!(ran.err.contains("is claimed by agent2"), "{}", ran.err);
    assert_eq!(ran.edited().len(), 0);
    assert_eq!(temp_files(&fake).len(), 0);
    assert_eq!(todo(&fake), HELD);
}

#[test]
fn takeover_lets_the_agent_edit_a_held_item() {
    let fake = typing(fake(HELD), HELD_TYPED);
    let ran = edit(&fake, &["id:k3f9", "--owner", "agent1", "--takeover"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(todo(&fake), HELD_TYPED);
    assert!(ran.out.starts_with("k3f9 Ring client"), "{}", ran.out);
}

/// Arguments, the exit code and a part of the output.
type Usage = (&'static [&'static str], u8, &'static str);

const USAGE: [Usage; 5] = [
    (&["1,2"], 2, "edit takes one"),
    (&["1", "--json"], 2, "edit does not take --json"),
    (&[], 2, "missing argument"),
    (&["1", "2"], 2, "edit takes one"),
    (&["9"], 3, "no item 9"),
];

#[test]
fn bad_calls_never_start_the_editor() {
    for (args, exit, want) in USAGE {
        let fake = fake(FIXTURE);
        let ran = edit(&fake, args);
        assert_eq!(ran.code, exit, "{args:?}: {}", ran.err);
        let text = format!("{}{}", ran.err, ran.out);
        assert!(text.contains(want), "{args:?}: {text}");
        assert!(ran.edited().is_empty(), "{args:?}");
    }
}

#[test]
fn edit_needs_two_terminals() {
    let fake = Fake::new().file("todo.txt", FIXTURE);
    let ran = fake.run(&["edit", "1", "--human"]);
    assert_eq!(
        (ran.code, ran.err.as_str()),
        (2, "lsty: edit needs a terminal\n")
    );
}

#[test]
fn no_lock_is_held_while_the_editor_runs() {
    let fake = fake(FIXTURE);
    let todo_path = fake.path().join("todo.txt");
    let fake = fake.editing(move |path| {
        let want = [Want {
            file: todo_path.clone(),
            mode: Mode::Exclusive,
        }];
        let mut held =
            acquire(&want, Duration::from_millis(200), &Cancel::new())
                .map_err(io::Error::other)?;
        fs::write(&todo_path, "Call client +work\n(A) Buy milk\n")?;
        held.release().map_err(io::Error::other)?;
        fs::write(path, "Ring client +work\n")
    });
    let ran = edit(&fake, &["1"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(todo(&fake), "Ring client +work\n(A) Buy milk\n");
}

#[test]
fn a_file_that_cannot_be_read_is_kept_in_the_error() {
    let fake = fake(FIXTURE).editing(|path| fs::remove_file(path));
    let ran = edit(&fake, &["1"]);
    assert_eq!(ran.code, 1);
    assert!(
        ran.err.starts_with("lsty: read the edited text: open "),
        "{}",
        ran.err
    );
    assert!(ran.err.contains("hint: your text is in "), "{}", ran.err);
}

#[test]
fn keeping_the_text_extends_a_hint() {
    let path = Path::new("/tmp/x.txt");
    let plain = keep_text(Error::new(Code::EditConflict, "changed"), path);
    assert_eq!(plain.hint(), Some("your text is in /tmp/x.txt"));
    let hinted = keep_text(
        Error::new(Code::WrongState, "no").with_hint("reword it"),
        path,
    );
    assert_eq!(hinted.hint(), Some("reword it; your text is in /tmp/x.txt"));
}

#[test]
fn a_line_that_is_not_text_comes_back_as_it_was() {
    let fake = fake(b"Call caf\xe9\nBuy\n");
    let fake = fake.editing(|path| {
        assert_eq!(fs::read(path)?, b"Call caf\xe9\n");
        fs::write(path, b"Ring caf\xe9\n")
    });
    let ran = edit(&fake, &["1"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        fs::read(fake.path().join("todo.txt")).unwrap(),
        b"Ring caf\xe9\nBuy\n"
    );
    assert_eq!(ran.out_bytes, b"1 Ring caf\xe9\n");
}
