//! A dim key against what it does, over every action.

use std::{cell::Cell, fs, path::Path, rc::Rc};

use lsty_core::{
    config::Config,
    keys::{Action, Mode},
};
use tempfile::TempDir;

use crate::{
    deps::Deps,
    focus::Focus,
    hints::dim,
    model::Model,
    msg::{Cmd, Msg},
    press_tests::{before_the_key, files},
    test_util::{Board, alone, deps, keys, screen, started, workdir},
};

/// A folder with `todo` and the key bindings `toml`.
fn bound(todo: &str, toml: &str) -> TempDir {
    let dir = workdir(todo);
    fs::write(dir.path().join("keys.toml"), toml)
        .unwrap_or_else(|e| panic!("{e}"));
    dir
}

/// All a press can change, but for what it says, taken off before this.
type Seen = ([Vec<u8>; 2], Vec<String>, Focus, Mode, String, bool, usize);

fn seen(dir: &Path, model: &Model, size: (u16, u16), opened: usize) -> Seen {
    let field = model.edit.is_some() || model.line.is_some();
    (
        files(dir),
        screen(model, size.0, size.1),
        model.focus,
        model.key_mode(),
        format!("{:?}", model.clip),
        field,
        opened,
    )
}

/// What a walk presses every action over: the config, the width and the
/// list with the keys before it.
type Place = (Config, u16, &'static str, &'static str);

const TAGGED: &str = "A +p @c\nB +q @d\n";

fn with_panels() -> Config {
    Config {
        show_sidebar: true,
        show_detail: true,
        ..alone()
    }
}

fn no_sidebar() -> Config {
    Config {
        show_detail: true,
        ..alone()
    }
}

fn the_empty_lists() -> Vec<Place> {
    let mut places = Vec::new();
    for (config, width) in [
        (with_panels(), 120),
        (with_panels(), 60),
        (no_sidebar(), 120),
        (no_sidebar(), 60),
        (alone(), 120),
    ] {
        places.push((config.clone(), width, "", ""));
        places.push((config, width, TAGGED, "/zzz<CR>"));
    }
    places
}

fn the_panels() -> Vec<Place> {
    let at = |first| (with_panels(), 120, TAGGED, first);
    vec![
        at("/zzz<CR><C-w>3"),
        at("/zzz<CR><C-w>4"),
        at("/zzz<CR><C-w>5"),
        at("<C-w>2"),
        at("<C-w>3"),
        at("<C-w>4"),
        at("<C-w>5"),
    ]
}

/// A model of `place` with `action` bound to `<F9>` in every mode.
fn walked(place: &Place, action: Action) -> (TempDir, Model, Rc<Cell<usize>>) {
    let (config, width, todo, first) = place.clone();
    let toml = Mode::ALL.iter().fold(String::new(), |toml, mode| {
        format!("{toml}[{}]\n{action} = [\"<F9>\"]\n", mode.as_str())
    });
    let dir = bound(todo, &toml);
    let opened = Rc::new(Cell::new(0));
    let runs = Rc::clone(&opened);
    let deps = Deps {
        clipboard: Box::new(Board::default()),
        opener: Box::new(move |_| {
            runs.set(runs.get() + 1);
            Ok(())
        }),
        ..deps(dir.path(), config)
    };
    let mut model = started(deps);
    model.update(Msg::Resize(width, 24));
    before_the_key(&mut model, first);
    (dir, model, opened)
}

/// Each action as its hint would show over `place`, pressed, with what it
/// did. `None` for an action that has no key where the place is.
fn walk(place: &Place, action: Action) -> Option<(bool, bool, bool)> {
    let (dir, mut model, opened) = walked(place, action);
    let mode = model.key_mode();
    if !model.deps.keys.keys(mode, action).contains(&"<F9>") {
        return None;
    }
    let size = (place.1, 24);
    let is_dim = dim(model.answers()(action));
    model.message = None;
    let before = seen(dir.path(), &model, size, opened.get());
    let cmds = keys(&mut model, "<F9>");
    let said = model.message.take().is_some();
    let after = seen(dir.path(), &model, size, opened.get());
    let ran = cmds.iter().any(|cmd| {
        !matches!(
            cmd,
            Cmd::After {
                msg: Msg::Expire(_),
                ..
            }
        )
    });
    Some((is_dim, before != after || ran, said))
}

/// Lit on an empty list, and pressing shows nothing: `gw` changes what the
/// next task looks like, and `<Esc>` has a hint only while it clears a search.
const LIT_AND_UNSEEN: [Action; 2] = [Action::ToggleWrap, Action::ClearSearch];

/// Lit on an empty list, and pressing only says why not.
const SAYS_WHY_NOT: [Action; 3] =
    [Action::PasteBelow, Action::PasteAbove, Action::ClearFilters];

#[test]
fn on_an_empty_list_a_key_is_dim_when_it_does_nothing() {
    let mut pressed = 0;
    let mut wrong = Vec::new();
    for place in the_empty_lists() {
        for &action in Action::ALL {
            let Some((is_dim, changed, said)) = walk(&place, action) else {
                continue;
            };
            pressed += 1;
            let at = (action.to_string(), place.1, place.2, place.3);
            if is_dim && changed {
                wrong.push(format!("dim and acts: {at:?}"));
            }
            let unseen = LIT_AND_UNSEEN.contains(&action);
            if is_dim && unseen {
                wrong.push(format!("dim and acts unseen: {at:?}"));
            }
            let says = said && SAYS_WHY_NOT.contains(&action);
            if !is_dim && !changed && !says && !unseen {
                wrong.push(format!("lit and does nothing: {at:?} {said}"));
            }
        }
    }
    assert!(pressed > 600, "{pressed}");
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
fn in_a_panel_no_key_that_works_is_dim() {
    let mut wrong = Vec::new();
    for place in the_panels() {
        for &action in Action::ALL {
            if let Some((true, true, _)) = walk(&place, action) {
                wrong.push(format!("{action}: {:?}", place.3));
            }
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}
