use std::{ffi::OsString, fs, path::Path};

use lsty_core::{
    keys::{Action, Mode, about},
    theme::Slot,
};
use ratatui::layout::Position;

use crate::{
    Deps,
    cmdline::own_names,
    help::{Help, listed},
    model::Model,
    msg::{Button, Cmd, Gesture, Mouse, Msg},
    test_util::{
        alone, deps, keys, press, screen, started, workdir, workdir_model,
    },
};

const TEXT_LINK: &str = "Read file:notes.md\n";

fn linked(editor: Option<&str>) -> (tempfile::TempDir, Model) {
    let dir = workdir(TEXT_LINK);
    fs::write(dir.path().join("notes.md"), "hello\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let deps = Deps {
        editor: editor.map(OsString::from),
        ..deps(dir.path(), alone())
    };
    (dir, started(deps))
}

/// `program` and the linked file, whose path has its links resolved
/// (on macOS the temp folder is under a link).
fn words(dir: &Path, program: &[&str]) -> Vec<OsString> {
    let mut words: Vec<OsString> =
        program.iter().map(OsString::from).collect();
    let real = fs::canonicalize(dir).unwrap_or_else(|e| panic!("{e}"));
    words.push(real.join("notes.md").into_os_string());
    words
}

#[test]
fn the_editor_setting_is_split_at_spaces() {
    let (dir, mut model) = linked(Some("code --wait"));
    let cmds = press(&mut model, "gx");
    assert_eq!(
        cmds,
        vec![Cmd::Edit(words(dir.path(), &["code", "--wait"]))]
    );
}

#[test]
fn without_an_editor_setting_the_program_is_vi() {
    for setting in [None, Some("")] {
        let (dir, mut model) = linked(setting);
        let cmds = press(&mut model, "gx");
        assert_eq!(cmds, vec![Cmd::Edit(words(dir.path(), &["vi"]))]);
    }
}

#[test]
fn a_text_link_asks_for_one_editor_and_changes_no_state() {
    let (_dir, mut model) = linked(None);
    let cmds = press(&mut model, "gx");
    assert_eq!(cmds.len(), 1);
    assert!(matches!(cmds.first(), Some(Cmd::Edit(_))));
    assert_eq!(model.message, None);
    assert!(model.opening.is_none());
}

#[test]
fn a_failed_editor_is_an_error_message() {
    let (_dir, mut model) = linked(None);
    model.update(Msg::Edited(Err("exit status 1".to_owned())));
    assert_eq!(
        model.message.as_deref(),
        Some("editor exited: exit status 1")
    );
    assert_eq!(model.tone, crate::messages::Tone::Alert);
    model.update(Msg::Edited(Ok(())));
    assert_eq!(
        model.message.as_deref(),
        Some("editor exited: exit status 1")
    );
}

#[test]
fn the_mouse_is_ignored_while_the_picker_is_open() {
    let (_dir, mut model) =
        workdir_model("Open file:a.md file:b.md\nSecond\n", alone());
    press(&mut model, "gx");
    assert!(model.opening.is_some());
    for y in [1, 2, 10] {
        for gesture in [Gesture::Down(Button::Left), Gesture::Up] {
            let shift = false;
            let cmds = model.update(Msg::Mouse(Mouse {
                gesture,
                at: Position::new(10, y),
                shift,
            }));
            assert_eq!(cmds, Vec::new());
        }
    }
    assert!(model.opening.is_some());
    assert_eq!(model.cursor, 0);
}

#[test]
fn the_picker_stops_at_both_ends() {
    let (_dir, mut model) =
        workdir_model("Open file:a.md file:b.md\n", alone());
    keys(&mut model, "gxkjjj");
    let lines = screen(&model, 80, 24);
    assert!(lines.iter().any(|l| l.contains("2 ✗ b.md")));
    press(&mut model, "q");
    assert!(model.opening.is_none());
}

/// What the help shows: for each heading its entries' keys and texts.
fn shown(help: &Help) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut heading = String::new();
    for line in help.lines() {
        let text: String = line.iter().map(|s| s.text.as_ref()).collect();
        match line.as_slice() {
            [one] if !text.starts_with(' ') => text.clone_into(&mut heading),
            [_, keys, _, desc] if keys.slot == Some(Slot::HintKey) => {
                out.push((
                    heading.clone(),
                    keys.text.to_string(),
                    desc.text.to_string(),
                ));
            }
            _ => {}
        }
    }
    out
}

#[test]
fn every_bound_action_is_listed_once_in_each_of_its_modes() {
    let dir = workdir("");
    let found = deps(dir.path(), alone());
    let help = Help::new(&found.keys, true);
    let mut want = Vec::new();
    for &action in Action::ALL {
        for mode in Mode::ALL {
            let written = listed(&found.keys, mode, action);
            if written.is_empty() {
                continue;
            }
            want.push((
                mode.as_str().to_owned(),
                action.category().as_str().to_owned(),
                written.join(" , "),
                about(mode, action).to_owned(),
            ));
        }
    }
    let mut got: Vec<(String, String, String, String)> = help
        .listing()
        .into_iter()
        .filter(|(_, category, ..)| !category.is_empty())
        .map(|(h, c, k, t)| {
            (h.to_owned(), c.to_owned(), k.to_owned(), t.to_owned())
        })
        .collect();
    got.sort();
    want.sort();
    assert_eq!(got, want);
    assert!(want.len() > 100);
}

#[test]
fn the_picker_marks_are_worked_out_when_drawn() {
    let dir = workdir("Two file:a.md file:b.md\n");
    fs::write(dir.path().join("a.md"), "a\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut model = started(deps(dir.path(), alone()));
    press(&mut model, "gx");
    let before = screen(&model, 80, 24);
    assert!(before.iter().any(|l| l.contains("1 ✓ a.md")));
    assert!(before.iter().any(|l| l.contains("2 ✗ b.md")));
    fs::write(dir.path().join("b.md"), "b\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let after = screen(&model, 80, 24);
    assert!(after.iter().any(|l| l.contains("2 ✓ b.md")));
}

#[test]
fn a_missing_target_is_an_error_message() {
    let (_dir, mut model) = workdir_model("Read file:gone.md\n", alone());
    let cmds = press(&mut model, "gx");
    assert_eq!(cmds, Vec::new());
    assert!(model.message.is_some());
    assert_eq!(model.tone, crate::messages::Tone::Alert);
}

#[test]
fn the_first_entry_is_the_first_motion() {
    let dir = workdir("");
    let found = deps(dir.path(), alone());
    let help = Help::new(&found.keys, true);
    let first = shown(&help).into_iter().next();
    assert_eq!(
        first,
        Some((
            "normal".to_owned(),
            "j , <Down>".to_owned(),
            "Move the cursor down".to_owned()
        ))
    );
}

#[test]
fn the_help_lists_mouse_lines_only_while_the_mouse_is_on() {
    let dir = workdir("");
    let found = deps(dir.path(), alone());
    let heads = |on| {
        let help = Help::new(&found.keys, on);
        help.listing().iter().any(|e| e.0 == "Mouse")
    };
    assert!(heads(true));
    assert!(!heads(false));
}

#[test]
fn the_help_lists_every_command_of_the_screen() {
    let dir = workdir("");
    let help = Help::new(&deps(dir.path(), alone()).keys, true);
    let listed: Vec<String> = help
        .listing()
        .into_iter()
        .filter(|e| e.0 == "Command line")
        .flat_map(|e| {
            e.2.split([',', ' ']).map(str::to_owned).collect::<Vec<_>>()
        })
        .collect();
    for name in own_names().chain(["add", "ls", "archive"]) {
        assert!(listed.contains(&format!(":{name}")), "{name}");
    }
}
