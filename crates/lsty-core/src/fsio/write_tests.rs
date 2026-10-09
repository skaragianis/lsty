use std::cell::RefCell;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, symlink};
use std::path::{Path, PathBuf};

use super::test_util::{chmod, dir, mode, put, temps, text};
use super::write::temp_path;
use super::{
    Crash, Hooks, OsHooks, clean_orphans, crash, folder_of, is_crash, read,
    resolve, write,
};
use crate::errs::Code;

/// Hooks that record every call and can fail at one point.
#[derive(Default)]
struct Spy {
    calls: RefCell<Vec<&'static str>>,
    crash_at: Option<&'static str>,
    fail_sync_file: bool,
    seen_temps: RefCell<Vec<PathBuf>>,
}

impl Spy {
    fn at(&self, point: &'static str) -> io::Result<()> {
        self.calls.borrow_mut().push(point);
        if self.crash_at == Some(point) {
            return Err(crash());
        }
        Ok(())
    }
}

impl Hooks for Spy {
    fn before_rename(&self, target: &Path) -> io::Result<()> {
        if let Some(dir) = target.parent() {
            for name in temps(dir) {
                self.seen_temps.borrow_mut().push(dir.join(name));
            }
        }
        self.at("before rename")
    }
    fn after_rename(&self, _: &Path) -> io::Result<()> {
        self.at("after rename")
    }
    fn after_dir_sync(&self, _: &Path) -> io::Result<()> {
        self.at("after folder sync")
    }
    fn sync_file(&self, _: &File) -> io::Result<()> {
        self.calls.borrow_mut().push("temp sync");
        if self.fail_sync_file {
            return Err(io::Error::other("sync failed"));
        }
        Ok(())
    }
    fn sync_dir(&self, _: &File) -> io::Result<()> {
        self.at("folder sync")
    }
}

fn crash_at(point: &'static str) -> Spy {
    Spy {
        crash_at: Some(point),
        ..Spy::default()
    }
}

/// The mode a new file gets in this process: `0666` less the umask.
fn new_file_mode(d: &Path) -> u32 {
    let probe = d.join("probe");
    drop(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o666)
            .open(&probe)
            .unwrap(),
    );
    let m = mode(&probe);
    fs::remove_file(&probe).unwrap();
    m
}

/// Whether the process ignores folder permissions (root).
fn ignores_permissions(d: &Path) -> bool {
    let probe = d.join("probe");
    let made = File::create(&probe).is_ok();
    drop(fs::remove_file(&probe));
    made
}

#[test]
fn write_then_read() {
    // 04.4, 04.23
    for data in ["a\r\nb\n", "", "\u{feff}no newline"] {
        let (_tmp, d) = dir();
        let path = d.join("todo.txt");
        write(&path, data.as_bytes(), &OsHooks).unwrap();
        assert_eq!(read(&path).unwrap().unwrap(), data.as_bytes());
    }
}

#[test]
fn a_new_file_gets_the_default_mode() {
    // 04.5
    let (_tmp, d) = dir();
    let want = new_file_mode(&d);
    write(&d.join("todo.txt"), b"x", &OsHooks).unwrap();
    assert_eq!(mode(&d.join("todo.txt")), want);
}

#[test]
fn an_existing_file_keeps_its_mode() {
    // 04.6, 04.7
    for m in [0o600, 0o640, 0o444, 0o755] {
        let (_tmp, d) = dir();
        let path = d.join("todo.txt");
        put(&path, "old", m);
        write(&path, b"new", &OsHooks).unwrap();
        assert_eq!(mode(&path), m);
        assert_eq!(text(&path), "new");
    }
}

#[test]
fn no_temp_file_is_left() {
    // 04.8
    let (_tmp, d) = dir();
    write(&d.join("todo.txt"), b"x", &OsHooks).unwrap();
    assert_eq!(temps(&d), Vec::<String>::new());
}

#[test]
fn a_crash_before_the_rename_keeps_the_old_content() {
    // 04.9
    let (_tmp, d) = dir();
    let path = d.join("todo.txt");
    put(&path, "old", 0o644);
    let err = write(&path, b"new", &crash_at("before rename")).unwrap_err();
    assert!(is_crash(&err), "{err}");
    assert_eq!(text(&path), "old");
}

#[test]
fn a_crash_after_the_rename_keeps_the_new_content() {
    // 04.10
    let (_tmp, d) = dir();
    let path = d.join("todo.txt");
    put(&path, "old", 0o644);
    let err = write(&path, b"new", &crash_at("after rename")).unwrap_err();
    assert!(is_crash(&err), "{err}");
    assert_eq!(text(&path), "new");
}

#[test]
fn a_crash_after_the_folder_sync_is_reported() {
    let (_tmp, d) = dir();
    let path = d.join("todo.txt");
    let err = write(&path, b"x", &crash_at("after folder sync")).unwrap_err();
    assert!(is_crash(&err));
    assert_eq!(text(&path), "x");
}

#[test]
fn the_order_of_the_steps() {
    // 04.11
    let (_tmp, d) = dir();
    let spy = Spy::default();
    write(&d.join("todo.txt"), b"x", &spy).unwrap();
    assert_eq!(
        *spy.calls.borrow(),
        [
            "temp sync",
            "before rename",
            "after rename",
            "folder sync",
            "after folder sync"
        ]
    );
}

#[test]
fn a_folder_without_write_permission() {
    // 04.12
    let (_tmp, d) = dir();
    let path = d.join("todo.txt");
    put(&path, "old", 0o644);
    chmod(&d, 0o500);
    let root = ignores_permissions(&d);
    let result = write(&path, b"new", &OsHooks);
    chmod(&d, 0o700);
    if root {
        return;
    }
    let err = result.unwrap_err();
    assert_eq!(err.code(), Code::Io);
    let want = format!(
        "write {}: open {}: permission denied",
        path.display(),
        temp_path(&path).display()
    );
    assert_eq!(err.message(), want);
    assert_eq!(text(&path), "old");
}

#[test]
fn a_write_through_a_symlink() {
    // 04.13, 04.14
    let (_tmp, d) = dir();
    fs::create_dir(d.join("real")).unwrap();
    put(&d.join("real/list.txt"), "old", 0o644);
    symlink("real/list.txt", d.join("link")).unwrap();
    let target = resolve(&d, &d.join("link")).unwrap();
    let spy = Spy::default();
    write(&target, b"new", &spy).unwrap();
    assert!(fs::symlink_metadata(d.join("link")).unwrap().is_symlink());
    assert_eq!(text(&d.join("real/list.txt")), "new");
    let seen = spy.seen_temps.borrow();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].parent(), Some(d.join("real").as_path()));
}

#[test]
fn a_write_to_a_dangling_link_creates_the_target() {
    // 04.15
    let (_tmp, d) = dir();
    symlink("made.txt", d.join("link")).unwrap();
    let target = resolve(&d, &d.join("link")).unwrap();
    write(&target, b"data", &OsHooks).unwrap();
    assert!(fs::symlink_metadata(d.join("link")).unwrap().is_symlink());
    assert_eq!(text(&d.join("made.txt")), "data");
}

#[test]
fn a_hard_link_keeps_the_old_content() {
    // 04.21
    let (_tmp, d) = dir();
    put(&d.join("todo.txt"), "old", 0o644);
    fs::hard_link(d.join("todo.txt"), d.join("other.txt")).unwrap();
    write(&d.join("todo.txt"), b"new", &OsHooks).unwrap();
    assert_eq!(text(&d.join("other.txt")), "old");
    assert_eq!(text(&d.join("todo.txt")), "new");
}

#[test]
fn hundred_sequential_writes() {
    // 04.24
    let (_tmp, d) = dir();
    let path = d.join("todo.txt");
    let mut content = String::new();
    for i in 0..100 {
        content = format!("{content}line {i}\n");
        write(&path, content.as_bytes(), &OsHooks).unwrap();
        assert_eq!(read(&path).unwrap().unwrap(), content.as_bytes(), "{i}");
    }
}

#[test]
fn a_failed_sync_leaves_the_target_unchanged() {
    for old in [Some("old"), None] {
        let (_tmp, d) = dir();
        let path = d.join("todo.txt");
        if let Some(old) = old {
            put(&path, old, 0o644);
        }
        let spy = Spy {
            fail_sync_file: true,
            ..Spy::default()
        };
        let err = write(&path, b"new", &spy).unwrap_err();
        assert_eq!(err.code(), Code::Io);
        let want = format!(
            "write {}: sync {}: sync failed",
            path.display(),
            temp_path(&path).display()
        );
        assert_eq!(err.message(), want);
        assert_eq!(temps(&d), Vec::<String>::new());
        let got = read(&path).unwrap();
        assert_eq!(got.as_deref(), old.map(str::as_bytes));
    }
}

#[test]
fn a_stale_temp_file_is_replaced() {
    let (_tmp, d) = dir();
    let path = d.join("todo.txt");
    put(&path, "old", 0o644);
    put(&temp_path(&path), "stale", 0o600);
    write(&path, b"new", &OsHooks).unwrap();
    assert_eq!(text(&path), "new");
    assert_eq!(temps(&d), Vec::<String>::new());
}

#[test]
fn the_temp_file_is_named_for_the_target_and_process() {
    let want = format!(".todo.txt.lsty-{}.tmp", std::process::id());
    let got = temp_path(Path::new("/x/todo.txt"));
    assert_eq!(got, Path::new("/x").join(want));
}

type Names = &'static [&'static str];

const ORPHAN_ROWS: [(Names, Names); 3] = [
    // 04.19
    (
        &[".todo.txt.lsty-123.tmp", ".todo.txt.lsty-9.tmp"],
        &[".todo.txt.lsty-123.tmp", ".todo.txt.lsty-9.tmp"],
    ),
    // 04.20
    (
        &[
            ".done.txt.lsty-5.tmp",
            ".todo.txt.lsty-x.tmp",
            "todo.txt.bak",
        ],
        &[],
    ),
    (
        &[
            ".todo.txt.lsty-.tmp",
            ".todo.txt.lsty-1.tmp.bak",
            "todo.txt",
        ],
        &[],
    ),
];

#[test]
fn clean_orphans_removes_only_matching_names() {
    for (files, want) in ORPHAN_ROWS {
        let (_tmp, d) = dir();
        for f in files {
            put(&d.join(f), "", 0o644);
        }
        let mut removed = clean_orphans(&d.join("todo.txt")).unwrap();
        removed.sort();
        let want: Vec<PathBuf> = want.iter().map(|n| d.join(n)).collect();
        assert_eq!(removed, want, "{files:?}");
        for f in files {
            let gone = want.contains(&d.join(f));
            assert_eq!(d.join(f).exists(), !gone, "{f}");
        }
    }
}

#[test]
fn clean_orphans_leaves_a_folder_alone() {
    let (_tmp, d) = dir();
    fs::create_dir(d.join(".todo.txt.lsty-1.tmp")).unwrap();
    assert_eq!(
        clean_orphans(&d.join("todo.txt")).unwrap(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn clean_orphans_in_a_missing_folder_is_an_io_error() {
    let (_tmp, d) = dir();
    let err = clean_orphans(&d.join("gone/todo.txt")).unwrap_err();
    assert_eq!(err.code(), Code::Io);
}

#[test]
fn a_write_through_a_regular_file_names_the_temp_step() {
    let (_tmp, d) = dir();
    put(&d.join("f"), "", 0o644);
    let path = d.join("f/x");
    let err = write(&path, b"x", &OsHooks).unwrap_err();
    let want = format!(
        "write {}: remove {}: not a directory",
        path.display(),
        temp_path(&path).display()
    );
    assert_eq!(err.message(), want);
}

#[test]
fn a_write_over_a_folder_names_both_paths_of_the_rename() {
    let (_tmp, d) = dir();
    let path = d.join("sub");
    fs::create_dir(&path).unwrap();
    let err = write(&path, b"x", &OsHooks).unwrap_err();
    let temp = temp_path(&path);
    let want = format!(
        "rename {0}: rename {0} {1}: file exists",
        temp.display(),
        path.display()
    );
    assert_eq!(err.message(), want);
    assert_eq!(temps(&d), Vec::<String>::new());
}

#[test]
fn clean_orphans_in_a_missing_folder_names_both_layers() {
    let (_tmp, d) = dir();
    let gone = d.join("gone");
    let err = clean_orphans(&gone.join("todo.txt")).unwrap_err();
    let want = format!(
        "read {0}: open {0}: no such file or directory",
        gone.display()
    );
    assert_eq!(err.message(), want);
}

#[test]
fn a_bare_file_name_lives_in_the_current_folder() {
    let rows = [
        ("u.txt", "."),
        ("/u.txt", "/"),
        ("/a/u.txt", "/a"),
        ("a/u.txt", "a"),
    ];
    for (path, want) in rows {
        let got = folder_of(Path::new(path));
        assert_eq!(got, Path::new(want), "{path}");
    }
}

#[test]
fn a_crash_is_told_from_other_errors_and_worded() {
    assert_eq!(Crash.to_string(), "fsio: simulated crash");
    let plain =
        crate::errs::io("write", Path::new("/x"), io::Error::other("no"));
    assert!(!is_crash(&plain));
    assert!(!is_crash(&crate::errs::Error::new(Code::Io, "x")));
}
