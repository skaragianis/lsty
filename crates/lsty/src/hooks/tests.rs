use std::{
    ffi::{OsStr, OsString},
    fs,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
};

use lsty_core::links::editor_command;

use super::{can_show, command_on, run_editor, run_pager};

/// Held by every test that writes a script and starts it: a program
/// forked by another test meanwhile would keep the script busy.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A script in `dir` that runs `body`.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[test]
fn the_editor_gets_its_arguments_and_the_file() {
    let _serial = serial();
    let tmp = tempfile::tempdir().unwrap();
    let args = tmp.path().join("args");
    let editor = script(
        tmp.path(),
        "fake-editor",
        &format!(
            "echo \"$1\" > {}\necho 'Ring client' > \"$2\"",
            args.display()
        ),
    );
    let file = tmp.path().join("task.txt");
    fs::write(&file, "Call client\n").unwrap();
    let setting = OsString::from(format!("{} --wait", editor.display()));
    run_editor(&editor_command(Some(&setting), &file)).unwrap();
    assert_eq!(fs::read_to_string(&args).unwrap().trim(), "--wait");
    assert_eq!(fs::read_to_string(&file).unwrap(), "Ring client\n");
}

#[test]
fn an_editor_that_fails_is_an_error_naming_it() {
    let _serial = serial();
    let tmp = tempfile::tempdir().unwrap();
    let editor = script(tmp.path(), "bad-editor", "exit 3");
    let run = |setting: &str| {
        let setting = OsString::from(setting);
        run_editor(&editor_command(Some(&setting), &tmp.path().join("x")))
            .unwrap_err()
    };
    assert_eq!(
        run(&editor.display().to_string()),
        format!("editor {}: exit status 3", editor.display())
    );
    assert_eq!(
        run("nosuched-editor-xyz"),
        "editor nosuched-editor-xyz: exec: \"nosuched-editor-xyz\": \
         executable file not found in $PATH"
    );
    assert_eq!(
        run("/nonexistent/editor"),
        "editor /nonexistent/editor: fork/exec /nonexistent/editor: \
         no such file or directory"
    );
    let killed = script(tmp.path(), "killed-editor", "kill -9 $$");
    assert_eq!(
        run(&killed.display().to_string()),
        format!("editor {}: signal: killed", killed.display())
    );
}

#[test]
fn the_pager_reads_the_text() {
    let _serial = serial();
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    let pager =
        script(tmp.path(), "pager", &format!("cat > {}", out.display()));
    let words = [pager.into_os_string()];
    run_pager(&words, b"line one\nline two\n").unwrap();
    assert_eq!(fs::read(&out).unwrap(), b"line one\nline two\n");
}

#[test]
fn a_pager_that_cannot_start_is_an_error() {
    let _serial = serial();
    let words = [OsString::from("/nonexistent/pager")];
    assert!(run_pager(&words, b"x").is_err());
    assert!(run_pager(&[], b"x").is_err());
}

#[test]
fn a_pager_that_quits_early_is_no_failure() {
    let _serial = serial();
    let tmp = tempfile::tempdir().unwrap();
    let pager = script(tmp.path(), "quit", "exit 0");
    let words = [pager.into_os_string()];
    let text = vec![b'x'; 1 << 20];
    assert!(run_pager(&words, &text).is_ok());
}

#[test]
fn a_program_found_through_a_relative_entry_is_not_started() {
    let _serial = serial();
    let tmp = tempfile::tempdir().unwrap();
    script(tmp.path(), "tool", "exit 0");
    let cwd = std::env::current_dir().unwrap();
    let up = "../".repeat(cwd.components().count());
    let relative = Path::new(&up).join(tmp.path().strip_prefix("/").unwrap());
    let want = "exec: \"tool\": cannot run executable found relative to \
                current directory";
    let err = command_on(OsStr::new("tool"), relative.as_os_str());
    assert_eq!(err.unwrap_err(), want);
    assert!(command_on(OsStr::new("tool"), tmp.path().as_os_str()).is_ok());
}

#[test]
fn the_opener_needs_a_display_or_a_browser() {
    assert!(!can_show("xdg-open", |_| None));
    for name in ["DISPLAY", "WAYLAND_DISPLAY", "BROWSER"] {
        assert!(can_show("xdg-open", |n| (n == name).then(|| "x".into())));
        assert!(!can_show("xdg-open", |n| (n == name).then(OsString::new)));
    }
    assert!(can_show("open", |_| None));
}
