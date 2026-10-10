use std::{
    ffi::OsStr,
    fs,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::Path,
};

use lsty_core::{config::Config, list::SortField};
use ratatui::layout::Position;

use crate::{
    model::Model,
    msg::{Button, Gesture, Mouse, Msg},
    session::keep,
    store::{Folds, Saved, Store, save},
    test_util::{
        alone, deps, keys, press, screen, start, started, started_from,
        state_of, workdir,
    },
};

const LIST: &str = "A +work\nB +work.acme\nC +home\n";

fn model_in(dir: &Path, config: Config) -> Model {
    started(deps(dir, config))
}

fn run_line(model: &mut Model, text: &str) {
    press(model, ":");
    press(model, text);
    keys(model, "<CR>");
}

fn text_of(model: &Model) -> String {
    model
        .line
        .as_ref()
        .map(|l| l.field.text())
        .unwrap_or_default()
}

fn history_file(dir: &Path) -> String {
    fs::read_to_string(state_of(dir).join("history.toml")).unwrap_or_default()
}

fn quote_all(lines: &[String]) -> String {
    let quoted: Vec<String> = lines.iter().map(|l| format!("{l:?}")).collect();
    format!("[{}]", quoted.join(", "))
}

#[test]
fn a_repeated_line_is_kept_once_at_the_end() {
    let dir = workdir(LIST);
    let mut model = model_in(dir.path(), alone());
    run_line(&mut model, "sort manual");
    run_line(&mut model, "sort due");
    run_line(&mut model, "sort manual");
    assert_eq!(
        history_file(dir.path()),
        "command = [\"sort due\", \"sort manual\"]\nsearch = []\n"
    );
}

#[test]
fn only_the_newest_two_hundred_lines_stay() {
    let mut lines = Vec::new();
    for i in 0..201 {
        keep(&mut lines, &format!("line {i}"));
    }
    let kept = format!("{lines:?}");
    assert!(!kept.contains("\"line 0\""));
    assert!(kept.contains("\"line 1\""));
    assert!(kept.contains("\"line 200\""));
    assert_eq!(kept.matches("line ").count(), 200);
}

#[test]
fn a_history_read_is_normalised_when_it_is_written_again() {
    let dir = workdir(LIST);
    let mut old: Vec<String> = (0..250).map(|i| format!("l{i}")).collect();
    old.push("l100".to_owned());
    fs::create_dir_all(state_of(dir.path())).unwrap_or_default();
    fs::write(
        state_of(dir.path()).join("history.toml"),
        format!("command = {}\nsearch = []\n", quote_all(&old)),
    )
    .unwrap_or_default();
    let mut model = model_in(dir.path(), alone());
    run_line(&mut model, "sort manual");
    let text = history_file(dir.path());
    assert_eq!(text.matches("\"l").count(), 199);
    assert!(text.contains("\"l100\", \"sort manual\"]"));
    assert!(!text.contains("\"l50\""));
    assert!(text.contains("\"l51\""));
}

#[test]
fn a_history_with_a_syntax_error_is_empty_and_replaced() {
    let dir = workdir(LIST);
    fs::create_dir_all(state_of(dir.path())).unwrap_or_default();
    fs::write(state_of(dir.path()).join("history.toml"), "command = [")
        .unwrap_or_default();
    let mut model = model_in(dir.path(), alone());
    keys(&mut model, ":<Up>");
    assert_eq!(text_of(&model), "");
    keys(&mut model, "<Esc>");
    run_line(&mut model, "sort manual");
    assert_eq!(
        history_file(dir.path()),
        "command = [\"sort manual\"]\nsearch = []\n"
    );
}

#[test]
fn a_failed_save_is_shown_once_and_the_screen_goes_on() {
    let dir = workdir(LIST);
    fs::write(dir.path().join("state"), "a file, not a folder")
        .unwrap_or_default();
    let mut model = model_in(dir.path(), alone());
    run_line(&mut model, "sort manual");
    let shown = model.message.clone().unwrap_or_default();
    assert!(shown.starts_with("can't save session state: "), "{shown}");
    run_line(&mut model, "sort due");
    assert_eq!(model.message, None);
    assert_eq!(Some(model.sort), SortField::parse("due"));
}

#[test]
fn the_state_folder_is_created_with_mode_0700() {
    let dir = workdir(LIST);
    let mut model = model_in(dir.path(), alone());
    assert!(!state_of(dir.path()).exists());
    run_line(&mut model, "sort manual");
    let mode = fs::metadata(state_of(dir.path()))
        .map(|m| m.permissions().mode() & 0o777)
        .unwrap_or_default();
    assert_eq!(mode, 0o700);
}

fn fold_work(model: &mut Model) {
    keys(model, "<C-w>3");
    press(model, "jh");
}

#[test]
fn a_name_that_is_gone_is_dropped_from_the_folds_when_written() {
    let dir = workdir(LIST);
    let folds = state_of(dir.path()).join("folds.toml");
    fs::create_dir_all(state_of(dir.path())).unwrap_or_default();
    let key = dir.path().join("todo.txt");
    let key = fs::canonicalize(&key).unwrap_or(key);
    let text = format!(
        "[{:?}]\n  projects = [\"gone\", \"home\"]\n  contexts = []\n\n\
         [\"/other/todo.txt\"]\n  projects = [\"+x\"]\n  contexts = []\n",
        key.display().to_string()
    );
    fs::write(&folds, text).unwrap_or_default();
    let mut model = model_in(dir.path(), Config::default());
    fold_work(&mut model);
    let after = fs::read_to_string(&folds).unwrap_or_default();
    assert!(!after.contains("gone"), "{after}");
    assert!(after.contains("\"home\", \"work\""), "{after}");
    assert!(after.contains("[\"/other/todo.txt\"]"), "{after}");
}

#[test]
fn the_folds_are_kept_for_each_file() {
    let dir = workdir(LIST);
    let mut model = model_in(dir.path(), Config::default());
    fold_work(&mut model);
    let again = model_in(dir.path(), Config::default());
    assert_eq!(again.panels[0].folded_names(), ["work"]);
    fs::write(dir.path().join("other.txt"), "A +work\n").unwrap_or_default();
    let deps = deps(dir.path(), Config::default());
    let mut s = start(dir.path());
    s.found.todo = dir.path().join("other.txt");
    let other = started_from(s, deps);
    assert_eq!(other.panels[0].folded_names(), Vec::<String>::new());
}

#[test]
fn no_config_does_not_turn_the_history_off() {
    let dir = workdir(LIST);
    let config = Config {
        no_config: true,
        ..alone()
    };
    let mut model = model_in(dir.path(), config.clone());
    run_line(&mut model, "sort due");
    let mut again = model_in(dir.path(), config);
    keys(&mut again, ":<Up>");
    assert_eq!(text_of(&again), "sort due");
}

#[test]
fn a_todo_file_whose_name_is_not_text_keeps_no_folds() {
    let dir = workdir(LIST);
    let name = dir.path().join(OsStr::from_bytes(b"t\xff.txt"));
    fs::write(&name, LIST).unwrap_or_default();
    let deps = deps(dir.path(), Config::default());
    let mut s = start(dir.path());
    s.found.todo = name;
    let mut model = started_from(s, deps);
    fold_work(&mut model);
    assert!(!state_of(dir.path()).join("folds.toml").exists());
}

#[test]
fn the_prompts_of_items_have_no_history() {
    let dir = workdir(LIST);
    let mut model = model_in(dir.path(), alone());
    run_line(&mut model, "sort due");
    keys(&mut model, "gt<Up>");
    assert_eq!(text_of(&model), "");
    keys(&mut model, "x<CR>");
    assert_eq!(
        history_file(dir.path()),
        "command = [\"sort due\"]\nsearch = []\n"
    );
}

#[test]
fn a_full_history_and_many_folds_start_and_save_quickly() {
    use std::time::Instant;
    let names: Vec<String> = (0..400).map(|i| format!("p{i}")).collect();
    let todo = names
        .iter()
        .map(|n| format!("A +{n}/c"))
        .collect::<Vec<_>>()
        .join("\n");
    let dir = workdir(&todo);
    fs::create_dir_all(state_of(dir.path())).unwrap_or_default();
    let lines: Vec<String> = (0..200).map(|i| format!("l{i}")).collect();
    fs::write(
        state_of(dir.path()).join("history.toml"),
        format!("command = {0}\nsearch = {0}\n", quote_all(&lines)),
    )
    .unwrap_or_default();
    let key = dir.path().join("todo.txt");
    let key = fs::canonicalize(&key).unwrap_or(key);
    fs::write(
        state_of(dir.path()).join("folds.toml"),
        format!(
            "[{:?}]\n  projects = {}\n  contexts = []\n",
            key.display().to_string(),
            quote_all(&names)
        ),
    )
    .unwrap_or_default();
    let began = Instant::now();
    let mut model = model_in(dir.path(), Config::default());
    let start = began.elapsed();
    let began = Instant::now();
    fold_work(&mut model);
    press(&mut model, "l");
    let toggles = began.elapsed();
    let limit = std::time::Duration::from_secs(2);
    assert!(start < limit && toggles < limit, "{start:?} {toggles:?}");
}

#[test]
fn quoted_lines_and_keys_read_back_as_written() {
    let dir = workdir(LIST);
    let store = Store::new(Some(state_of(dir.path())));
    let line = "say \"hi\" \\ a\tb \u{7f}\u{1b} \u{8}".to_owned();
    let saved = Saved {
        command: vec![line.clone()],
        search: vec![line],
    };
    assert_eq!(store.save_history(&saved), Ok(()));
    assert_eq!(store.history(), saved);
    let text = history_file(dir.path());
    assert!(text.contains("\\u007f\\u001b \\b"), "{text}");
    let folds = Folds {
        projects: vec!["a \"b\"\t\\".to_owned()],
        contexts: Vec::new(),
    };
    assert_eq!(store.save_folds("/x y/\"t\".txt", folds.clone()), Ok(()));
    assert_eq!(store.folds("/x y/\"t\".txt"), folds);
}

#[test]
fn a_folder_failure_names_the_folder_that_failed() {
    let dir = workdir(LIST);
    let file = dir.path().join("f");
    fs::write(&file, "x").unwrap_or_default();
    let want = format!("{}: not a directory", file.display());
    let store = Store::new(Some(file.clone()));
    assert_eq!(store.save_history(&Saved::default()), Err(want.clone()));
    let store = Store::new(Some(file.join("lsty")));
    assert_eq!(store.save_history(&Saved::default()), Err(want));
}

#[test]
fn a_fold_by_the_mouse_is_kept_like_one_by_key() {
    let dir = workdir(LIST);
    let mut model = model_in(dir.path(), Config::default());
    let screen = screen(&model, 80, 24);
    let at = screen.iter().enumerate().find_map(|(y, line)| {
        let before = line.split_once('▾')?.0;
        Some((before.chars().count(), y))
    });
    let Some((x, y)) = at else {
        panic!("no node to fold in {screen:?}");
    };
    for gesture in [Gesture::Down(Button::Left), Gesture::Up] {
        model.update(Msg::Mouse(Mouse {
            gesture,
            at: Position::new(
                crate::layout::cells(x),
                crate::layout::cells(y),
            ),
            shift: false,
        }));
    }
    let folds = state_of(dir.path()).join("folds.toml");
    let text = fs::read_to_string(folds).unwrap_or_default();
    assert!(text.contains("\"work\""), "{text}");
}

#[test]
fn a_linked_file_is_written_through_its_links() {
    let dir = workdir("");
    let real = dir.path().join("kept").join("history.toml");
    fs::create_dir(dir.path().join("kept")).unwrap_or_else(|e| panic!("{e}"));
    let mid = dir.path().join("mid.toml");
    let link = dir.path().join("link.toml");
    std::os::unix::fs::symlink(&real, &mid).unwrap_or_else(|e| panic!("{e}"));
    std::os::unix::fs::symlink("mid.toml", &link)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(save(&link, b"x\n"), Ok(()));
    assert!(fs::symlink_metadata(&link).is_ok_and(|m| m.is_symlink()));
    assert!(fs::symlink_metadata(&mid).is_ok_and(|m| m.is_symlink()));
    assert_eq!(fs::read(&real).ok(), Some(b"x\n".to_vec()));
}

#[test]
fn a_chain_of_links_past_the_limit_is_refused_and_left_alone() {
    let dir = workdir("");
    let real = dir.path().join("real.toml");
    let name = |i: usize| dir.path().join(format!("l{i}.toml"));
    std::os::unix::fs::symlink(&real, name(0))
        .unwrap_or_else(|e| panic!("{e}"));
    for i in 1..41 {
        std::os::unix::fs::symlink(name(i - 1), name(i))
            .unwrap_or_else(|e| panic!("{e}"));
    }
    // 41 links to a missing file: one past the limit.
    let err = save(&name(40), b"x\n").unwrap_err();
    assert!(err.contains("too many levels"), "{err}");
    assert!(!real.exists());
    for i in 0..=40 {
        assert!(fs::symlink_metadata(name(i)).is_ok_and(|m| m.is_symlink()));
    }
    // 40 links reach the target.
    assert_eq!(save(&name(39), b"x\n"), Ok(()));
    assert_eq!(fs::read(&real).ok(), Some(b"x\n".to_vec()));
    assert!(fs::symlink_metadata(name(39)).is_ok_and(|m| m.is_symlink()));
}
