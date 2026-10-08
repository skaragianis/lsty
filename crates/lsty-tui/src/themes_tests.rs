use std::fs;

use lsty_core::config::Config;
use lsty_core::keys::{Named, Stroke};

use crate::Model;
use crate::deps::Deps;
use crate::msg::Msg;
use crate::test_util::{SAMPLE, alone, deps, press, started, workdir};
use crate::themes::{Unset, with_theme};

/// A config file and what it reads once `theme` is set to `nord`.
type Edit = (&'static str, &'static str);

const EDITS: &[Edit] = &[
    ("", "theme = \"nord\"\n"),
    ("theme = \"dracula\"\n", "theme = \"nord\"\n"),
    (
        "# mine\ntheme = 'dracula' # dark\nshow_sidebar = false\n",
        "# mine\ntheme = \"nord\" # dark\nshow_sidebar = false\n",
    ),
    (
        "show_sidebar = false\nshow_detail = false",
        "show_sidebar = false\nshow_detail = false\ntheme = \"nord\"\n",
    ),
    (
        "# mine\nshow_sidebar = false # off\n\n[hints]\nnormal = []\n",
        "# mine\nshow_sidebar = false # off\ntheme = \"nord\"\n\n[hints]\n\
         normal = []\n",
    ),
    (
        "# mine\n\n[hints]\nnormal = []\n",
        "# mine\n\ntheme = \"nord\"\n[hints]\nnormal = []\n",
    ),
    (
        "[hints]\ntheme = \"x\"\n",
        "theme = \"nord\"\n[hints]\ntheme = \"x\"\n",
    ),
    (
        "keys = [\n  [1, 2],\n]\n[[filter]]\nname = \"a\"\n",
        "keys = [\n  [1, 2],\n]\ntheme = \"nord\"\n[[filter]]\nname = \"a\"\n",
    ),
    ("theme = 3\n", "theme = \"nord\"\n"),
    ("a.b = [\n[1],\n]\n", "theme = \"nord\"\na.b = [\n[1],\n]\n"),
];

#[test]
fn the_theme_is_set_and_the_rest_of_the_file_kept() {
    for (before, after) in EDITS {
        assert_eq!(
            with_theme(before, "nord").as_deref(),
            Ok(*after),
            "{before:?}"
        );
    }
}

#[test]
fn a_file_that_is_not_toml_is_not_changed() {
    assert_eq!(with_theme("theme = \n", "nord"), Err(Unset::NotToml));
    assert_eq!(with_theme("[a\n", "nord"), Err(Unset::NotToml));
}

#[test]
fn a_theme_table_is_not_replaced() {
    assert_eq!(with_theme("[theme]\nx = 1\n", "nord"), Err(Unset::Table));
    assert_eq!(with_theme("theme.x = 1\n", "nord"), Err(Unset::Table));
    assert_eq!(with_theme("[[theme]]\nx = 1\n", "nord"), Err(Unset::Table));
}

#[test]
fn a_byte_order_mark_stays_first() {
    assert_eq!(
        with_theme("\u{feff}[hints]\n", "nord").as_deref(),
        Ok("\u{feff}theme = \"nord\"\n[hints]\n")
    );
}

#[test]
fn a_linked_config_is_written_through_the_link() {
    let dir = workdir(SAMPLE);
    let real = dir.path().join("dotfiles").join("lsty.toml");
    let link = dir.path().join("config.toml");
    fs::create_dir(dir.path().join("dotfiles"))
        .unwrap_or_else(|e| panic!("{e}"));
    fs::write(&real, "show_sidebar = false\n")
        .unwrap_or_else(|e| panic!("{e}"));
    std::os::unix::fs::symlink(&real, &link).unwrap_or_else(|e| panic!("{e}"));
    let mut model = started(Deps {
        config_file: Some(link.clone()),
        ..deps(dir.path(), alone())
    });
    open(&mut model);
    press(&mut model, "j");
    named(&mut model, Named::Cr);
    let name = model.deps.config.theme.clone();
    assert!(fs::symlink_metadata(&link).is_ok_and(|m| m.is_symlink()));
    assert_eq!(
        fs::read_to_string(&real).ok(),
        Some(format!("show_sidebar = false\ntheme = \"{name}\"\n"))
    );
}

#[test]
fn a_name_is_quoted_as_toml() {
    assert_eq!(
        with_theme("", "a\"b\\c").as_deref(),
        Ok("theme = \"a\\\"b\\\\c\"\n")
    );
}

fn named(model: &mut Model, key: Named) {
    model.update(Msg::Key(Stroke::named(key)));
}

fn picker(config: Config) -> (tempfile::TempDir, Model) {
    let dir = workdir(SAMPLE);
    let model = started(deps(dir.path(), config));
    (dir, model)
}

fn open(model: &mut Model) {
    press(model, ":theme");
    named(model, Named::Cr);
}

#[test]
fn enter_with_no_theme_left_keeps_the_popup_open() {
    let (dir, mut model) = picker(alone());
    open(&mut model);
    press(&mut model, "/nothing");
    named(&mut model, Named::Cr);
    named(&mut model, Named::Cr);
    assert!(model.choosing.is_some());
    assert!(!dir.path().join("config.toml").exists());
}

#[test]
fn without_a_config_the_theme_lasts_for_the_session() {
    let (dir, mut model) = picker(Config {
        no_config: true,
        ..alone()
    });
    open(&mut model);
    press(&mut model, "j");
    named(&mut model, Named::Cr);
    assert!(model.choosing.is_none());
    let name = model.deps.config.theme.clone();
    assert_eq!(
        model.message.as_deref(),
        Some(format!("theme {name} for this session").as_str())
    );
    assert!(!dir.path().join("config.toml").exists());
}

#[test]
fn a_config_that_is_not_toml_is_left_and_the_failure_shown() {
    let dir = workdir(SAMPLE);
    let file = dir.path().join("config.toml");
    fs::write(&file, "theme = [\n").unwrap_or_else(|e| panic!("{e}"));
    let mut model = started(Deps {
        config_file: Some(file.clone()),
        ..deps(dir.path(), alone())
    });
    open(&mut model);
    press(&mut model, "j");
    named(&mut model, Named::Cr);
    let want =
        format!("can't save the theme: {}: not valid TOML", file.display());
    assert_eq!(model.message.as_deref(), Some(want.as_str()));
    assert_eq!(
        fs::read_to_string(&file).ok().as_deref(),
        Some("theme = [\n")
    );
}

#[test]
fn the_config_folder_is_made_when_missing() {
    let dir = workdir(SAMPLE);
    let file = dir.path().join("conf").join("lsty").join("config.toml");
    let mut model = started(Deps {
        config_file: Some(file.clone()),
        ..deps(dir.path(), alone())
    });
    open(&mut model);
    press(&mut model, "j");
    named(&mut model, Named::Cr);
    let name = model.deps.config.theme.clone();
    let want = format!("theme = \"{name}\"\n");
    assert_eq!(fs::read_to_string(&file).ok(), Some(want));
}

#[test]
fn a_cancelled_filter_goes_back_to_the_one_before() {
    let (_dir, mut model) = picker(alone());
    open(&mut model);
    press(&mut model, "/nord");
    named(&mut model, Named::Cr);
    press(&mut model, "/x");
    named(&mut model, Named::Esc);
    named(&mut model, Named::Cr);
    assert_eq!(model.deps.config.theme, "nord");
}

fn loaded(model: &Model, name: &str) -> lsty_core::theme::Theme {
    let deps = &model.deps;
    let no_config = deps.config.no_config;
    lsty_core::theme::load(name, &deps.themes, &deps.cwd, no_config).0
}

/// Opens the `:` line on `:theme ` and steps to the first name.
fn stepped(model: &mut Model) -> lsty_core::theme::Theme {
    let before = model.deps.theme.clone();
    press(model, ":theme ");
    named(model, Named::Tab);
    before
}

#[test]
fn a_theme_name_stepped_to_on_the_line_shows_at_once() {
    let (_dir, mut model) = picker(alone());
    let before = stepped(&mut model);
    assert_eq!(model.deps.theme, loaded(&model, "ansi"));
    assert_ne!(model.deps.theme, before);
    named(&mut model, Named::Esc);
    assert_eq!(model.deps.theme, before);
}

#[test]
fn the_arrows_move_through_the_names_and_enter_keeps_one() {
    let (_dir, mut model) = picker(alone());
    stepped(&mut model);
    named(&mut model, Named::Right);
    named(&mut model, Named::Down);
    named(&mut model, Named::Left);
    let second = "catppuccin-frappe";
    assert_eq!(model.deps.theme, loaded(&model, second));
    named(&mut model, Named::Cr);
    assert!(model.line.is_none());
    assert_eq!(model.deps.config.theme, second);
    assert_eq!(model.deps.theme, loaded(&model, second));
}

#[test]
fn typing_after_a_step_brings_back_the_theme_before() {
    let (_dir, mut model) = picker(alone());
    let before = stepped(&mut model);
    press(&mut model, "x");
    assert_eq!(model.deps.theme, before);
}

#[test]
fn the_popup_and_the_line_go_back_to_the_same_theme() {
    let (_dir, mut model) = picker(alone());
    let before = model.deps.theme.clone();
    open(&mut model);
    press(&mut model, "jj");
    assert_ne!(model.deps.theme, before);
    named(&mut model, Named::Esc);
    assert_eq!(model.deps.theme, before);
    assert!(model.previewed.is_none());
}
