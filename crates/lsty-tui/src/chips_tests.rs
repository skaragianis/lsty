use std::{
    ffi::OsStr,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use crate::{
    chips::left_cut,
    deps::Start,
    model::Model,
    test_util::{alone, deps, found_at, started_from, workdir},
};

#[test]
fn a_path_is_cut_from_the_left_at_a_folder() {
    const ROWS: [(&str, usize, &str); 11] = [
        ("~/todo.txt", 10, "~/todo.txt"),
        ("~/todo.txt", 9, "todo.txt"),
        ("~/a/b/todo.txt", 14, "~/a/b/todo.txt"),
        ("~/a/b/todo.txt", 13, "…/b/todo.txt"),
        ("~/a/bb/todo.txt", 12, "…/todo.txt"),
        ("/home/dev/lsty-rs/todo.txt", 20, "…/lsty-rs/todo.txt"),
        ("/home/devx/todo.txt", 100, "/home/devx/todo.txt"),
        ("/srv/todo.txt", 8, "todo.txt"),
        ("/srv/todo.txt", 7, "…do.txt"),
        ("/srv/todo.txt", 1, "…"),
        ("/日本語/予定.txt", 10, "…/予定.txt"),
    ];
    for (path, room, want) in ROWS {
        assert_eq!(left_cut(path, room), want, "{path} in {room}");
    }
}

fn title_of(dir: &Path, file: &OsStr, home: Option<PathBuf>) -> String {
    let mut deps = deps(dir, alone());
    deps.home = home;
    let start = Start {
        found: found_at(dir, Path::new(file)),
        ..crate::test_util::start(dir)
    };
    let model: Model = started_from(start, deps);
    model.path
}

#[test]
fn the_title_path_writes_home_as_a_tilde() {
    let dir = workdir("a\n");
    let at = dir.path();
    let name = OsStr::new("todo.txt");
    let want = format!("{}/todo.txt", at.display());
    assert_eq!(title_of(at, name, Some(at.to_path_buf())), "~/todo.txt");
    assert_eq!(title_of(at, name, None), want);
    let other = PathBuf::from("/nowhere");
    assert_eq!(title_of(at, name, Some(other)), want);
    // a folder whose name merely starts like home is not inside it
    let text = at.to_string_lossy();
    let near = PathBuf::from(text.get(..text.len() - 1).unwrap_or_default());
    assert_eq!(title_of(at, name, Some(near)), want);
    let parent = at.parent().map(Path::to_path_buf);
    let inside = format!("~/{}/todo.txt", at.file_name().unwrap().display());
    assert_eq!(title_of(at, name, parent), inside);
}

#[test]
fn a_home_behind_a_symlink_is_still_a_tilde() {
    let dir = workdir("a\n");
    let link = dir.path().join("home-link");
    std::os::unix::fs::symlink(dir.path(), &link).unwrap();
    let got = title_of(dir.path(), OsStr::new("todo.txt"), Some(link));
    assert_eq!(got, "~/todo.txt");
}

#[test]
fn bytes_that_are_not_text_show_as_replacements() {
    let dir = workdir("a\n");
    let bytes = OsStr::from_bytes(b"to\xffdo.txt");
    fs::write(dir.path().join(bytes), "a\n").unwrap();
    let got = title_of(dir.path(), bytes, Some(dir.path().to_path_buf()));
    assert_eq!(got, "~/to\u{fffd}do.txt");
}
