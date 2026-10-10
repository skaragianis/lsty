use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt as _;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::walk::{git_root, is_regular};
use super::{FileState, Found, FoundBy, Input, find};
use crate::errs::{Code, Field, lossy_path};
use crate::fsio::{
    self,
    test_util::{Locked, is_root},
};

/// One row: files (a trailing `/` makes a folder), symlinks, and the
/// expected outcome. `@` in a text stands for the temp folder.
#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    files: &'static [&'static str],
    links: &'static [(&'static str, &'static str)],
    cwd: &'static str,
    home: &'static str,
    no_home: bool,
    explicit: &'static str,
    default_file: &'static str,
    env: &'static [(&'static str, &'static str)],
    todo: &'static str,
    done: &'static str,
    found_by: &'static str,
    state: FileState,
    err: Option<Code>,
    err_field: &'static str,
    err_contains: &'static str,
}

const BASE: Case = Case {
    name: "",
    files: &[],
    links: &[],
    cwd: "",
    home: "@/H",
    no_home: false,
    explicit: "",
    default_file: "",
    env: &[],
    todo: "",
    done: "",
    found_by: "",
    state: FileState::Missing,
    err: None,
    err_field: "",
    err_contains: "",
};

const CASES: &[Case] = &[
    Case {
        name: "06.1 explicit wins over TODO_FILE",
        files: &["w/x/list.txt", "t/"],
        cwd: "@/w",
        explicit: "x/list.txt",
        env: &[("TODO_FILE", "@/t/a.txt")],
        todo: "@/w/x/list.txt",
        done: "@/w/x/list.done.txt",
        found_by: "--file",
        state: FileState::Exists,
        ..BASE
    },
    Case {
        name: "06.2 explicit missing file",
        files: &["w/"],
        cwd: "@/w",
        explicit: "new.txt",
        todo: "@/w/new.txt",
        done: "@/w/new.done.txt",
        found_by: "--file",
        ..BASE
    },
    Case {
        name: "06.3 TODO_FILE missing beats walk-up",
        files: &["t/", "w/todo.txt"],
        cwd: "@/w",
        env: &[("TODO_FILE", "@/t/a.txt")],
        todo: "@/t/a.txt",
        done: "@/t/a.done.txt",
        found_by: "TODO_FILE",
        ..BASE
    },
    Case {
        name: "06.4 empty TODO_FILE is unset",
        files: &["w/todo.txt"],
        cwd: "@/w",
        env: &[("TODO_FILE", "")],
        todo: "@/w/todo.txt",
        done: "@/w/done.txt",
        found_by: "walk-up",
        state: FileState::Exists,
        ..BASE
    },
    Case {
        name: "06.5 walk up to git root",
        files: &["R/.git/", "R/a/b/", "R/todo.txt"],
        cwd: "@/R/a/b",
        todo: "@/R/todo.txt",
        done: "@/R/done.txt",
        found_by: "walk-up",
        state: FileState::Exists,
        ..BASE
    },
    Case {
        name: "06.6 nearest todo.txt wins",
        files: &["R/.git/", "R/a/b/", "R/a/todo.txt", "R/todo.txt"],
        cwd: "@/R/a/b",
        todo: "@/R/a/todo.txt",
        done: "@/R/a/done.txt",
        found_by: "walk-up",
        state: FileState::Exists,
        ..BASE
    },
    Case {
        name: "06.7 walk stops at git root",
        files: &["H/p/R/.git/", "H/p/R/a/", "H/p/todo.txt"],
        cwd: "@/H/p/R/a",
        todo: "@/H/p/R/todo.txt",
        done: "@/H/p/R/done.txt",
        found_by: "fallback",
        ..BASE
    },
    Case {
        name: "06.8 walk reaches home",
        files: &["H/a/b/", "H/todo.txt"],
        cwd: "@/H/a/b",
        todo: "@/H/todo.txt",
        done: "@/H/done.txt",
        found_by: "walk-up",
        state: FileState::Exists,
        ..BASE
    },
    Case {
        name: "06.9 walk stops at home",
        files: &["H/a/", "todo.txt"],
        cwd: "@/H/a",
        todo: "@/H/a/todo.txt",
        done: "@/H/a/done.txt",
        found_by: "fallback",
        ..BASE
    },
    Case {
        name: "walk stops at home with a trailing slash",
        home: "@/H/",
        files: &["H/a/", "todo.txt"],
        cwd: "@/H/a",
        todo: "@/H/a/todo.txt",
        done: "@/H/a/done.txt",
        found_by: "fallback",
        ..BASE
    },
    Case {
        name: "06.10 outside home",
        files: &["o/x/y/", "o/todo.txt"],
        cwd: "@/o/x/y",
        todo: "@/o/todo.txt",
        done: "@/o/done.txt",
        found_by: "walk-up",
        state: FileState::Exists,
        ..BASE
    },
    Case {
        name: "06.11 project list beats TODO_DIR",
        files: &["R/.git/", "R/a/", "R/todo.txt", "g/"],
        cwd: "@/R/a",
        env: &[("TODO_DIR", "@/g")],
        todo: "@/R/todo.txt",
        done: "@/R/done.txt",
        found_by: "walk-up",
        state: FileState::Exists,
        ..BASE
    },
    Case {
        name: "06.12 TODO_DIR when walk finds nothing",
        files: &["R/.git/", "R/a/", "g/"],
        cwd: "@/R/a",
        env: &[("TODO_DIR", "@/g")],
        todo: "@/g/todo.txt",
        done: "@/g/done.txt",
        found_by: "TODO_DIR",
        ..BASE
    },
    Case {
        name: "06.13 default_file with tilde",
        files: &["H/lists/", "H/x/"],
        cwd: "@/H/x",
        default_file: "~/lists/main.txt",
        todo: "@/H/lists/main.txt",
        done: "@/H/lists/main.done.txt",
        found_by: "default_file",
        ..BASE
    },
    Case {
        name: "06.14 fallback to git root",
        files: &["R/.git/", "R/a/b/"],
        cwd: "@/R/a/b",
        todo: "@/R/todo.txt",
        done: "@/R/done.txt",
        found_by: "fallback",
        ..BASE
    },
    Case {
        name: "06.15 fallback to current folder",
        files: &["H/x/"],
        cwd: "@/H/x",
        todo: "@/H/x/todo.txt",
        done: "@/H/x/done.txt",
        found_by: "fallback",
        ..BASE
    },
    Case {
        name: "06.16 git file marks the root",
        files: &["R/.git", "R/a/"],
        cwd: "@/R/a",
        todo: "@/R/todo.txt",
        done: "@/R/done.txt",
        found_by: "fallback",
        ..BASE
    },
    Case {
        name: "06.17 folder named todo.txt is skipped",
        files: &["H/c/todo.txt/", "H/todo.txt"],
        cwd: "@/H/c",
        todo: "@/H/todo.txt",
        done: "@/H/done.txt",
        found_by: "walk-up",
        state: FileState::Exists,
        ..BASE
    },
    Case {
        name: "06.18 DONE_FILE ignored for walk-up",
        files: &["R/.git/", "R/a/", "R/todo.txt", "g/"],
        cwd: "@/R/a",
        env: &[("DONE_FILE", "@/g/done.txt")],
        todo: "@/R/todo.txt",
        done: "@/R/done.txt",
        found_by: "walk-up",
        state: FileState::Exists,
        ..BASE
    },
    Case {
        name: "06.19 DONE_FILE with TODO_FILE",
        files: &["t/", "g/"],
        cwd: "@/t",
        env: &[("TODO_FILE", "@/t/a.txt"), ("DONE_FILE", "@/g/d.txt")],
        todo: "@/t/a.txt",
        done: "@/g/d.txt",
        found_by: "TODO_FILE",
        ..BASE
    },
    Case {
        name: "06.19 DONE_FILE with TODO_DIR",
        files: &["R/.git/", "g/", "h/"],
        cwd: "@/R",
        env: &[("TODO_DIR", "@/g"), ("DONE_FILE", "@/h/d.txt")],
        todo: "@/g/todo.txt",
        done: "@/h/d.txt",
        found_by: "TODO_DIR",
        ..BASE
    },
    Case {
        name: "06.20 TODO_DIR without DONE_FILE",
        files: &["R/.git/", "g/"],
        cwd: "@/R",
        env: &[("TODO_DIR", "@/g")],
        todo: "@/g/todo.txt",
        done: "@/g/done.txt",
        found_by: "TODO_DIR",
        ..BASE
    },
    Case {
        name: "06.21 named list",
        files: &["t/"],
        cwd: "@/t",
        env: &[("TODO_FILE", "@/t/work.txt")],
        todo: "@/t/work.txt",
        done: "@/t/work.done.txt",
        found_by: "TODO_FILE",
        ..BASE
    },
    Case {
        name: "06.22 no extension",
        files: &["t/"],
        cwd: "@/t",
        explicit: "@/t/tasks",
        todo: "@/t/tasks",
        done: "@/t/tasks.done.txt",
        found_by: "--file",
        ..BASE
    },
    Case {
        name: "06.22 other extension",
        files: &["t/"],
        cwd: "@/t",
        explicit: "@/t/tasks.md",
        todo: "@/t/tasks.md",
        done: "@/t/tasks.md.done.txt",
        found_by: "--file",
        ..BASE
    },
    Case {
        name: "06.23 DONE_FILE ignored for explicit",
        files: &["t/", "g/"],
        cwd: "@/t",
        explicit: "@/t/a.txt",
        env: &[("DONE_FILE", "@/g/d.txt")],
        todo: "@/t/a.txt",
        done: "@/t/a.done.txt",
        found_by: "--file",
        ..BASE
    },
    Case {
        name: "06.24 symlinked todo",
        files: &["L/proj.txt", "P/"],
        links: &[("P/todo.txt", "L/proj.txt")],
        cwd: "@/P",
        todo: "@/L/proj.txt",
        done: "@/L/proj.done.txt",
        found_by: "walk-up",
        state: FileState::Exists,
        ..BASE
    },
    Case {
        name: "06.25 relative TODO_FILE",
        files: &["w/rel/"],
        cwd: "@/w",
        env: &[("TODO_FILE", "rel/a.txt")],
        todo: "@/w/rel/a.txt",
        done: "@/w/rel/a.done.txt",
        found_by: "TODO_FILE",
        ..BASE
    },
    Case {
        name: "06.26 explicit names a folder",
        files: &["w/d/"],
        cwd: "@/w",
        explicit: "d",
        err: Some(Code::Io),
        err_contains: "@/w/d",
        ..BASE
    },
    Case {
        name: "06.27 DONE_FILE equals todo",
        files: &["t/"],
        cwd: "@/t",
        env: &[("TODO_FILE", "@/t/a.txt"), ("DONE_FILE", "@/t/a.txt")],
        err: Some(Code::InvalidValue),
        err_field: "DONE_FILE",
        err_contains: "DONE_FILE",
        ..BASE
    },
    Case {
        name: "06.28 TODOTXT variables are ignored",
        files: &["w/"],
        cwd: "@/w",
        env: &[
            ("TODOTXT_DEFAULT_ACTION", "ls"),
            ("TODOTXT_CFG_FILE", "@/x.cfg"),
        ],
        todo: "@/w/todo.txt",
        done: "@/w/done.txt",
        found_by: "fallback",
        ..BASE
    },
    Case {
        name: "06.29 missing folder",
        files: &["t/"],
        cwd: "@/t",
        env: &[("TODO_FILE", "@/t/nodir/a.txt")],
        todo: "@/t/nodir/a.txt",
        done: "@/t/nodir/a.done.txt",
        found_by: "TODO_FILE",
        state: FileState::DirMissing,
        ..BASE
    },
    Case {
        name: "06.30 missing folder is cleaned",
        files: &["w/"],
        cwd: "@/w",
        explicit: "nodir/../x/list.txt",
        todo: "@/w/x/list.txt",
        done: "@/w/x/list.done.txt",
        found_by: "--file",
        state: FileState::DirMissing,
        ..BASE
    },
    Case {
        name: "default_file ~user is not expanded",
        files: &["w/"],
        cwd: "@/w",
        default_file: "~user/x.txt",
        todo: "@/w/~user/x.txt",
        done: "@/w/~user/x.done.txt",
        found_by: "default_file",
        state: FileState::DirMissing,
        ..BASE
    },
    Case {
        name: "default_file ~ alone is the home folder",
        files: &["H/", "w/"],
        cwd: "@/w",
        default_file: "~",
        err: Some(Code::Io),
        err_contains: "@/H is a folder",
        ..BASE
    },
    Case {
        name: "default_file ~ without a home is the current folder",
        files: &["w/"],
        cwd: "@/w",
        no_home: true,
        default_file: "~",
        err: Some(Code::Io),
        err_contains: "@/w is a folder",
        ..BASE
    },
    Case {
        name: "relative DONE_FILE",
        files: &["t/rel/"],
        cwd: "@/t",
        env: &[("TODO_FILE", "@/t/a.txt"), ("DONE_FILE", "rel/d.txt")],
        todo: "@/t/a.txt",
        done: "@/t/rel/d.txt",
        found_by: "TODO_FILE",
        ..BASE
    },
    Case {
        name: "folder is a regular file counts as missing",
        files: &["t/f"],
        cwd: "@/t",
        explicit: "f/a.txt",
        todo: "@/t/f/a.txt",
        done: "@/t/f/a.done.txt",
        found_by: "--file",
        state: FileState::DirMissing,
        ..BASE
    },
];

fn mkdir(path: &Path) {
    fs::create_dir_all(path).expect("mkdir");
}

fn build(root: &Path, case: &Case) {
    for name in case.files {
        let path = root.join(name.trim_end_matches('/'));
        if name.ends_with('/') {
            mkdir(&path);
            continue;
        }
        mkdir(path.parent().expect("parent"));
        fs::write(&path, b"").expect("write");
    }
    for (link, target) in case.links {
        let path = root.join(link);
        mkdir(path.parent().expect("parent"));
        symlink(root.join(target), path).expect("symlink");
    }
}

fn has_ancestor_marker(root: &Path) -> bool {
    git_root(root).is_some()
        || root.ancestors().any(|d| is_regular(&d.join("todo.txt")))
}

/// A temp folder with no git folder or todo file above it, so a search
/// from inside stops at what the row built: in the usual
/// temp folder when that is clean, else in a system one that is.
fn bare_temp() -> tempfile::TempDir {
    let usual = tempfile::tempdir().expect("temp dir");
    if !has_ancestor_marker(&canonical(usual.path())) {
        return usual;
    }
    ["/tmp", "/var/tmp", "/dev/shm"]
        .iter()
        .find_map(|base| {
            let dir = tempfile::tempdir_in(base).ok()?;
            (!has_ancestor_marker(&canonical(dir.path()))).then_some(dir)
        })
        .expect("a temp folder with no .git or todo.txt above it")
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).expect("canonicalize")
}

fn sub(root: &Path, text: &str) -> String {
    text.replace('@', &root.to_string_lossy())
}

fn run(root: &Path, case: &Case) -> crate::errs::Result<Found> {
    let env: HashMap<String, String> = case
        .env
        .iter()
        .map(|(k, v)| ((*k).to_owned(), sub(root, v)))
        .collect();
    let vars = |k: &str| env.get(k).map(OsString::from);
    let explicit = PathBuf::from(sub(root, case.explicit));
    let cwd = PathBuf::from(sub(root, case.cwd));
    let home = PathBuf::from(sub(root, case.home));
    find(&Input {
        explicit: (!case.explicit.is_empty()).then_some(explicit.as_path()),
        vars: &vars,
        cwd: &cwd,
        home: (!case.no_home).then_some(home.as_path()),
        default_file: (!case.default_file.is_empty())
            .then_some(case.default_file),
    })
}

fn want(root: &Path, case: &Case) -> (PathBuf, PathBuf) {
    (
        PathBuf::from(sub(root, case.todo)),
        PathBuf::from(sub(root, case.done)),
    )
}

fn check_ok(root: &Path, case: &Case, got: &Found) {
    let (todo, done) = want(root, case);
    assert_eq!(got.todo, todo, "{}", case.name);
    assert_eq!(got.done, done, "{}", case.name);
    assert_eq!(got.found_by.as_str(), case.found_by, "{}", case.name);
    assert_eq!(got.state, case.state, "{}", case.name);
}

fn check_err(root: &Path, case: &Case, err: &crate::errs::Error) {
    assert_eq!(Some(err.code()), case.err, "{}", case.name);
    let wanted = sub(root, case.err_contains);
    assert!(err.message().contains(&wanted), "{}: {}", case.name, err);
    if !case.err_field.is_empty() {
        let found = err.text("field") == Some(case.err_field);
        assert!(found, "{}: no field {}", case.name, case.err_field);
    }
}

#[test]
fn find_cases() {
    for case in CASES {
        let tmp = bare_temp();
        let root = canonical(tmp.path());
        build(&root, case);
        match run(&root, case) {
            Ok(got) => {
                assert!(case.err.is_none(), "{}: no error", case.name);
                check_ok(&root, case, &got);
            }
            Err(err) => check_err(&root, case, &err),
        }
    }
}

#[test]
fn folder_error_keeps_the_path_in_the_message() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = fs::canonicalize(tmp.path()).expect("canonicalize");
    mkdir(&root.join("d"));
    let explicit = PathBuf::from("d");
    let err = find(&Input {
        explicit: Some(&explicit),
        vars: &|_| None,
        cwd: &root,
        home: None,
        default_file: None,
    })
    .expect_err("folder");
    assert_eq!(
        err.message(),
        format!("{} is a folder", root.join("d").display())
    );
}

#[test]
fn done_file_equal_to_the_todo_file_has_the_whole_shape() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = fs::canonicalize(tmp.path()).expect("canonicalize");
    let mut bytes = root.join("a.txt").into_os_string().into_vec();
    bytes.extend_from_slice(b"\xe2\x82");
    let path = OsString::from_vec(bytes);
    let shown = format!("{}/a.txt\u{fffd}\u{fffd}", lossy_path(&root));
    let vars = |k: &str| match k {
        "TODO_FILE" | "DONE_FILE" => Some(path.clone()),
        _ => None,
    };
    let err = find(&Input {
        explicit: None,
        vars: &vars,
        cwd: &root,
        home: None,
        default_file: None,
    })
    .expect_err("same file");
    assert_eq!(err.code(), Code::InvalidValue);
    assert_eq!(
        err.message(),
        format!("DONE_FILE is the todo file: {shown}")
    );
    let fields: Vec<_> = err.fields().collect();
    let accepted = vec!["a path other than the todo file".to_owned()];
    assert_eq!(
        fields,
        [
            ("accepted", &Field::Strs(accepted)),
            ("field", &Field::Str("DONE_FILE".to_owned())),
            ("value", &Field::Str(shown)),
        ]
    );
}

#[test]
fn an_unsearchable_folder_is_an_io_error() {
    if is_root() {
        return;
    }
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = fs::canonicalize(tmp.path()).expect("canonicalize");
    mkdir(&root.join("locked/sub"));
    let _locked = Locked::new(&root.join("locked"));
    let explicit = root.join("locked/sub/new.txt");
    let err = find(&Input {
        explicit: Some(&explicit),
        vars: &|_| None,
        cwd: &root,
        home: None,
        default_file: None,
    })
    .expect_err("unsearchable");
    assert_eq!(err.code(), Code::Io);
    assert!(!fsio::is_no_dir(&err));
    let want = format!(
        "resolve {}: lstat {}: permission denied",
        explicit.display(),
        root.join("locked/sub").display()
    );
    assert_eq!(err.message(), want);
}

#[test]
fn a_todo_file_variable_that_is_not_utf8_is_used() {
    use std::os::unix::ffi::OsStringExt as _;
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = fs::canonicalize(tmp.path()).expect("canonicalize");
    let mut bytes = root.clone().into_os_string().into_vec();
    bytes.extend_from_slice(b"/caf\xe9.txt");
    let vars = |k: &str| {
        (k == "TODO_FILE").then(|| OsString::from_vec(bytes.clone()))
    };
    let found = find(&Input {
        explicit: None,
        vars: &vars,
        cwd: &root,
        home: None,
        default_file: None,
    })
    .expect("found");
    assert_eq!(found.found_by, FoundBy::TodoFile);
    assert_eq!(found.todo.as_os_str().as_encoded_bytes(), &bytes[..]);
    assert_eq!(found.state, FileState::Missing);
}

#[test]
fn a_missing_todo_file_is_an_error_that_names_the_step() {
    for found_by in [
        FoundBy::File,
        FoundBy::TodoFile,
        FoundBy::TodoDir,
        FoundBy::DefaultFile,
        FoundBy::Fallback,
        FoundBy::WalkUp,
    ] {
        let found = Found {
            todo: PathBuf::from("/w/todo.txt"),
            done: PathBuf::from("/w/done.txt"),
            found_by,
            state: FileState::Missing,
        };
        let err = found.no_file();
        assert_eq!(err.code(), Code::NoFile);
        assert_eq!(
            err.message(),
            format!("no todo file at /w/todo.txt (chosen by {found_by})")
        );
        assert_eq!(err.hint(), None, "{found_by}");
        assert_eq!(err.text("found_by"), Some(found_by.as_str()));
    }
}
