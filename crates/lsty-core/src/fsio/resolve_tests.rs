use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::test_util::{Locked, dir, is_root, put};
use super::{is_no_dir, resolve};
use crate::errs::Code;

/// A setup returns the path to resolve and the path the result or error
/// names.
type Setup = fn(&Path) -> (PathBuf, PathBuf);

fn relative(d: &Path) -> (PathBuf, PathBuf) {
    put(&d.join("todo.txt"), "", 0o644);
    ("todo.txt".into(), d.join("todo.txt"))
}

fn missing_folder(d: &Path) -> (PathBuf, PathBuf) {
    (d.join("missing-dir/todo.txt"), d.join("missing-dir"))
}

fn symlinked_folder(d: &Path) -> (PathBuf, PathBuf) {
    fs::create_dir(d.join("real")).unwrap();
    symlink(d.join("real"), d.join("dirlink")).unwrap();
    (d.join("dirlink/todo.txt"), d.join("real/todo.txt"))
}

fn under_a_regular_file(d: &Path) -> (PathBuf, PathBuf) {
    put(&d.join("f"), "", 0o644);
    (d.join("f/todo.txt"), d.join("f"))
}

fn link_through_a_regular_file(d: &Path) -> (PathBuf, PathBuf) {
    put(&d.join("f"), "", 0o644);
    symlink("f/todo.txt", d.join("link")).unwrap();
    (d.join("link"), d.join("f"))
}

fn dotdot_link_through_a_regular_file(d: &Path) -> (PathBuf, PathBuf) {
    put(&d.join("f"), "", 0o644);
    fs::create_dir(d.join("d")).unwrap();
    symlink("../f/todo.txt", d.join("d/l2")).unwrap();
    (d.join("d/l2"), d.join("f"))
}

fn dangling_dotdot_link(d: &Path) -> (PathBuf, PathBuf) {
    fs::create_dir_all(d.join("x/y")).unwrap();
    symlink("x/y", d.join("a")).unwrap();
    symlink("../c.txt", d.join("a/link")).unwrap();
    (d.join("a/link"), d.join("x/c.txt"))
}

fn dangling_link(d: &Path) -> (PathBuf, PathBuf) {
    symlink("real.txt", d.join("link")).unwrap();
    (d.join("link"), d.join("real.txt"))
}

fn link_to_a_file(d: &Path) -> (PathBuf, PathBuf) {
    fs::create_dir(d.join("real")).unwrap();
    put(&d.join("real/list.txt"), "", 0o644);
    symlink("real/list.txt", d.join("link")).unwrap();
    (d.join("link"), d.join("real/list.txt"))
}

const OK_ROWS: [(&str, Setup); 5] = [
    ("04.16 relative path", relative),
    ("04.18 missing file in a symlinked folder", symlinked_folder),
    ("dangling link with dotdot target", dangling_dotdot_link),
    ("04.15 dangling link", dangling_link),
    ("04.13 link to a file", link_to_a_file),
];

const NO_DIR_ROWS: [(&str, Setup); 4] = [
    ("04.17 missing folder", missing_folder),
    ("under a regular file", under_a_regular_file),
    ("link through a regular file", link_through_a_regular_file),
    (
        "dotdot link through a file",
        dotdot_link_through_a_regular_file,
    ),
];

#[test]
fn resolves() {
    for (name, setup) in OK_ROWS {
        let (_tmp, d) = dir();
        let (input, want) = setup(&d);
        let got = resolve(&d, &input).unwrap();
        assert_eq!(got, want, "{name}");
        assert!(got.is_absolute(), "{name}");
    }
}

#[test]
fn resolves_a_missing_folder_as_no_dir() {
    for (name, setup) in NO_DIR_ROWS {
        let (_tmp, d) = dir();
        let (input, folder) = setup(&d);
        let err = resolve(&d, &input).unwrap_err();
        assert!(is_no_dir(&err), "{name}: {err}");
        assert_eq!(err.code(), Code::Io, "{name}");
        let tail = format!(": {}", folder.display());
        assert!(err.message().ends_with(&tail), "{name}: {err}");
    }
}

#[test]
fn a_link_loop_is_an_io_error() {
    let (_tmp, d) = dir();
    symlink("b", d.join("a")).unwrap();
    symlink("a", d.join("b")).unwrap();
    let err = resolve(&d, &d.join("a")).unwrap_err();
    assert_eq!(err.code(), Code::Io);
    assert!(!is_no_dir(&err));
    let want = format!(
        "resolve {}: EvalSymlinks: too many links",
        d.join("a").display()
    );
    assert_eq!(err.message(), want);
}

/// The path to resolve and the prefix whose lstat is refused, with
/// `d/locked` (mode 000, holding `sub/`) in place.
const LOCKED_ROWS: [(&str, Setup); 4] = [
    ("the failing file", |d| {
        (d.join("locked/todo.txt"), d.join("locked/todo.txt"))
    }),
    ("the failing prefix", |d| {
        (d.join("locked/sub/new.txt"), d.join("locked/sub"))
    }),
    ("under a link", |d| {
        symlink("locked/sub/f.txt", d.join("link")).unwrap();
        (d.join("link"), d.join("locked/sub"))
    }),
    ("behind a linked folder", |d| {
        symlink("locked", d.join("folder-link")).unwrap();
        (d.join("folder-link/sub/new.txt"), d.join("locked/sub"))
    }),
];

#[test]
fn an_unsearchable_folder_names_the_failing_prefix() {
    if is_root() {
        return;
    }
    for (name, setup) in LOCKED_ROWS {
        let (_tmp, d) = dir();
        fs::create_dir_all(d.join("locked/sub")).unwrap();
        let _locked = Locked::new(&d.join("locked"));
        let (path, prefix) = setup(&d);
        let err = resolve(&d, &path).unwrap_err();
        assert_eq!(err.code(), Code::Io, "{name}");
        assert!(!is_no_dir(&err), "{name}");
        let want = format!(
            "resolve {}: lstat {}: permission denied",
            path.display(),
            prefix.display()
        );
        assert_eq!(err.message(), want, "{name}");
    }
}
