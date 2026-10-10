use std::cell::RefCell;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{Kind, Opener, classify, editor_command, open};
use crate::errs::Code;

/// What [`classify`] says, without the text of a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Refused,
    Missing,
    Text,
    Other,
}

fn class(path: &Path) -> Class {
    match classify(path) {
        Kind::Refused(_) => Class::Refused,
        Kind::Missing(_) => Class::Missing,
        Kind::Text => Class::Text,
        Kind::Other => Class::Other,
    }
}

fn dir() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    (tmp, root)
}

fn file(root: &Path, name: &str, content: &[u8], mode: u32) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, content).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    path
}

#[test]
fn plain_paths() {
    let (_tmp, root) = dir();
    file(&root, "notes.md", b"# notes\n", 0o644);
    file(&root, "photo.png", b"\x89PNG\r\n\x1a\n\x00\x00", 0o644);
    fs::create_dir(root.join("folder")).unwrap();
    let rows = [
        ("gone.md", Class::Missing),
        ("notes.md", Class::Text),
        ("photo.png", Class::Other),
        ("folder", Class::Other),
    ];
    for (name, want) in rows {
        assert_eq!(class(&root.join(name)), want, "{name}");
    }
}

#[test]
fn missing_says_so() {
    let (_tmp, root) = dir();
    let want = format!("{}/gone.md does not exist", root.display());
    assert_eq!(classify(&root.join("gone.md")), Kind::Missing(want));
}

#[test]
fn programs_are_refused() {
    let (_tmp, root) = dir();
    file(&root, "run.sh", b"#!/bin/sh\n", 0o755);
    file(&root, "tool.desktop", b"[Desktop Entry]\n", 0o644);
    file(&root, "Tool.EXE", b"MZ", 0o644);
    file(&root, "x.jar", b"PK", 0o644);
    fs::create_dir(root.join("My.app")).unwrap();
    file(&root, "dir.app.txt", b"x", 0o644);
    for name in ["run.sh", "tool.desktop", "Tool.EXE", "x.jar", "My.app"] {
        let path = root.join(name);
        let reason = format!("won't open executable: {}", path.display());
        assert_eq!(classify(&path), Kind::Refused(reason), "{name}");
    }
    assert_eq!(class(&root.join("dir.app.txt")), Class::Text);
}

#[test]
fn every_launcher_extension_is_refused_in_any_case() {
    let (_tmp, root) = dir();
    for ext in super::LAUNCHER_EXTS {
        for name in [format!("x{ext}"), format!("x{}", ext.to_uppercase())] {
            file(&root, &name, b"x", 0o644);
            let kind = class(&root.join(&name));
            assert_eq!(kind, Class::Refused, "{name}");
        }
    }
    file(&root, ".desktop", b"x", 0o644);
    assert_eq!(class(&root.join(".desktop")), Class::Refused);
    file(&root, "x.app\u{130}mage", b"x", 0o644);
    assert_eq!(class(&root.join("x.app\u{130}mage")), Class::Refused);
}

#[test]
fn a_script_without_execute_bits_and_an_empty_file_are_text() {
    let (_tmp, root) = dir();
    file(&root, "run.sh", b"#!/bin/sh\n", 0o644);
    file(&root, "empty", b"", 0o644);
    assert_eq!(class(&root.join("run.sh")), Class::Text);
    assert_eq!(class(&root.join("empty")), Class::Text);
}

#[test]
fn a_symlink_is_judged_by_its_target() {
    let (_tmp, root) = dir();
    let tool = file(&root, "tool", b"#!/bin/sh\n", 0o755);
    file(&root, "notes.md", b"n", 0o644);
    symlink(&tool, root.join("link")).unwrap();
    symlink(root.join("notes.md"), root.join("notes-link")).unwrap();
    symlink(root.join("notes.md"), root.join("go.desktop")).unwrap();
    symlink(root.join("nowhere"), root.join("dangling")).unwrap();
    let rows = [
        ("link", Class::Refused),
        ("notes-link", Class::Text),
        ("go.desktop", Class::Refused),
        ("dangling", Class::Missing),
    ];
    for (name, want) in rows {
        assert_eq!(class(&root.join(name)), want, "{name}");
    }
}

#[test]
fn text_is_valid_utf8_without_nul() {
    let (_tmp, root) = dir();
    file(&root, "latin", b"caf\xe9", 0o644);
    file(&root, "nul", b"a\0b", 0o644);
    file(&root, "utf8", "caf\u{e9} \u{1f600}".as_bytes(), 0o644);
    assert_eq!(class(&root.join("latin")), Class::Other);
    assert_eq!(class(&root.join("nul")), Class::Other);
    assert_eq!(class(&root.join("utf8")), Class::Text);
}

#[test]
fn a_rune_cut_by_the_sample_does_not_matter() {
    let (_tmp, root) = dir();
    let mut cut = vec![b'a'; super::SNIFF_SIZE - 1];
    cut.extend_from_slice("\u{e9}".as_bytes());
    file(&root, "cut", &cut, 0o644);
    assert_eq!(class(&root.join("cut")), Class::Text);
    let mut bad = vec![b'a'; super::SNIFF_SIZE - 3];
    bad.extend_from_slice(b"\xff\xfe\xfd");
    file(&root, "bad", &bad, 0o644);
    assert_eq!(class(&root.join("bad")), Class::Other);
    let mut late = vec![b'a'; super::SNIFF_SIZE];
    late.push(0xff);
    file(&root, "late", &late, 0o644);
    assert_eq!(class(&root.join("late")), Class::Text);
}

/// What `open` started.
#[derive(Default)]
struct Started {
    edited: RefCell<Vec<PathBuf>>,
    opened: RefCell<Vec<PathBuf>>,
}

fn open_with(path: &Path) -> (crate::errs::Result<()>, Started) {
    let started = Started::default();
    let edit = |p: &Path| {
        started.edited.borrow_mut().push(p.to_path_buf());
        Ok(())
    };
    let other = |p: &Path| {
        started.opened.borrow_mut().push(p.to_path_buf());
        Ok(())
    };
    let result = open(
        path,
        &Opener {
            run_editor: &edit,
            open_path: &other,
        },
    );
    (result, started)
}

#[test]
fn text_goes_to_the_editor_and_the_rest_to_the_opener() {
    let (_tmp, root) = dir();
    let notes = file(&root, "notes.md", b"n", 0o644);
    let photo = file(&root, "photo.png", b"\0", 0o644);
    let (result, started) = open_with(&notes);
    assert!(result.is_ok());
    assert_eq!(*started.edited.borrow(), [notes]);
    assert!(started.opened.borrow().is_empty());
    let (result, started) = open_with(&photo);
    assert!(result.is_ok());
    assert_eq!(*started.opened.borrow(), [photo]);
    assert!(started.edited.borrow().is_empty());
}

#[test]
fn refused_and_missing_start_nothing() {
    let (_tmp, root) = dir();
    let tool = file(&root, "tool.sh", b"#!/bin/sh\n", 0o755);
    let (result, started) = open_with(&tool);
    let err = result.unwrap_err();
    assert_eq!(err.code(), Code::InvalidValue);
    assert_eq!(
        err.message(),
        format!("won't open executable: {}", tool.display())
    );
    let fields: Vec<_> = err.fields().map(|(k, _)| k).collect();
    assert_eq!(fields, ["field", "value"]);
    let (result, started2) = open_with(&root.join("gone"));
    assert_eq!(result.unwrap_err().code(), Code::Io);
    for s in [started, started2] {
        assert!(s.edited.borrow().is_empty() && s.opened.borrow().is_empty());
    }
}

#[test]
fn a_failing_program_is_an_io_error() {
    let (_tmp, root) = dir();
    let notes = file(&root, "notes.md", b"n", 0o644);
    let fail = |_: &Path| Err("editor vi: exit status 1".to_owned());
    let err = open(
        &notes,
        &Opener {
            run_editor: &fail,
            open_path: &fail,
        },
    )
    .unwrap_err();
    assert_eq!(
        (err.code(), err.message()),
        (Code::Io, "editor vi: exit status 1")
    );
}

#[test]
fn the_editor_command_splits_at_spaces() {
    let path = Path::new("/t/x.txt");
    let words = |e: Option<&str>| -> Vec<String> {
        editor_command(e.map(OsStr::new), path)
            .iter()
            .map(|w| w.to_string_lossy().into_owned())
            .collect()
    };
    assert_eq!(words(None), ["vi", "/t/x.txt"]);
    assert_eq!(words(Some("")), ["vi", "/t/x.txt"]);
    assert_eq!(words(Some("  ")), ["vi", "/t/x.txt"]);
    assert_eq!(words(Some("nano")), ["nano", "/t/x.txt"]);
    assert_eq!(words(Some("code  --wait")), ["code", "--wait", "/t/x.txt"]);
}

type WordRow = (&'static [u8], &'static [&'static [u8]]);

const WORD_ROWS: [WordRow; 7] = [
    (b"", &[]),
    (b"  a \t b\n", &[b"a", b"b"]),
    (b"/bin/ed\xe9 -w", &[b"/bin/ed\xe9", b"-w"]),
    (b"a\xffb c", &[b"a\xffb", b"c"]),
    ("a\u{a0}b".as_bytes(), &[b"a", b"b"]),
    ("a\u{3000}\u{3000}b".as_bytes(), &[b"a", b"b"]),
    (b"\xe9\xa0 x", &[b"\xe9\xa0", b"x"]),
];

#[test]
fn settings_split_at_white_space_and_keep_invalid_bytes() {
    use std::os::unix::ffi::OsStrExt as _;
    for (input, want) in WORD_ROWS {
        let got: Vec<Vec<u8>> = super::split_words(OsStr::from_bytes(input))
            .iter()
            .map(|w| w.as_bytes().to_vec())
            .collect();
        let want: Vec<Vec<u8>> = want.iter().map(|w| w.to_vec()).collect();
        assert_eq!(got, want, "{input:?}");
    }
}

#[test]
fn a_path_through_a_file_is_not_missing() {
    let (_tmp, root) = dir();
    file(&root, "notes.md", b"n", 0o644);
    // The folder part is a file: the system says "not a directory", which
    // is no reason to call the path missing.
    assert_eq!(class(&root.join("notes.md/child")), Class::Other);
}

#[test]
fn only_the_last_name_gives_the_extension() {
    let (_tmp, root) = dir();
    fs::create_dir(root.join("My.app")).unwrap();
    fs::create_dir(root.join("tool.desktop")).unwrap();
    file(&root, "My.app/readme", b"text", 0o644);
    file(&root, "tool.desktop/notes.md", b"text", 0o644);
    assert_eq!(class(&root.join("My.app/readme")), Class::Text);
    assert_eq!(class(&root.join("tool.desktop/notes.md")), Class::Text);
    assert_eq!(super::extension(Path::new("a.d/name")), b"");
    assert_eq!(super::extension(Path::new("a/b.txt")), b".txt");
    assert_eq!(super::extension(Path::new("/x/.desktop")), b".desktop");
}

/// The link as written and the path it names.
type Link<'a> = (&'a [u8], PathBuf);

#[test]
fn linked_files_are_resolved_in_line_order() {
    let (_tmp, root) = dir();
    let text = "Read file:here.md file:~/mine.md file:gone.md";
    let snap = crate::list::test_util::snapshot_in(&root, &[text], &[]);
    let item = &snap.items()[0];
    let home = root.join("home");
    let links = super::linked_files(&snap, item, Some(&home));
    let got: Vec<Link<'_>> = links
        .iter()
        .map(|l| (l.raw.as_slice(), l.path.clone()))
        .collect();
    let want: Vec<Link<'_>> = vec![
        (b"here.md", root.join("here.md")),
        (b"~/mine.md", home.join("mine.md")),
        (b"gone.md", root.join("gone.md")),
    ];
    assert_eq!(got, want);
}
