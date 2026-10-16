use std::fs;

use lsty_core::config::Config;

use super::{Unset, with_theme};
use crate::{
    deps::Deps,
    model::Model,
    msg::Msg,
    test_util::{
        SAMPLE, alone, deps, keys, press, started, workdir, workdir_model,
    },
};

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
        "theme = \"nord\"\nshow_sidebar = false\nshow_detail = false",
    ),
    (
        "# mine\n\n[hints]\nnormal = []\n",
        "theme = \"nord\"\n# mine\n\n[hints]\nnormal = []\n",
    ),
    (
        "[hints]\ntheme = \"x\"\n",
        "theme = \"nord\"\n[hints]\ntheme = \"x\"\n",
    ),
    (
        "filters.work = '''\n[work]\n'''\n",
        "theme = \"nord\"\nfilters.work = '''\n[work]\n'''\n",
    ),
    (
        "keys = \"\"\"\n[a]\n\"\"\"\n[hints]\n",
        "theme = \"nord\"\nkeys = \"\"\"\n[a]\n\"\"\"\n[hints]\n",
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
    keys(&mut model, "j<CR>");
    let name = model.deps.config.theme.clone();
    assert!(fs::symlink_metadata(&link).is_ok_and(|m| m.is_symlink()));
    assert_eq!(
        fs::read_to_string(&real).ok(),
        Some(format!("theme = \"{name}\"\nshow_sidebar = false\n"))
    );
}

#[test]
fn a_name_is_quoted_as_toml() {
    assert_eq!(
        with_theme("", "a\"b\\c").as_deref(),
        Ok("theme = \"a\\\"b\\\\c\"\n")
    );
}

fn open(model: &mut Model) {
    press(model, ":theme");
    keys(model, "<CR>");
}

#[test]
fn enter_with_no_theme_left_keeps_the_popup_open() {
    let (dir, mut model) = workdir_model(SAMPLE, alone());
    open(&mut model);
    keys(&mut model, "/nothing<CR><CR>");
    assert!(model.choosing.is_some());
    assert!(!dir.path().join("config.toml").exists());
}

#[test]
fn without_a_config_the_theme_lasts_for_the_session() {
    let (dir, mut model) = workdir_model(
        SAMPLE,
        Config {
            no_config: true,
            ..alone()
        },
    );
    open(&mut model);
    keys(&mut model, "j<CR>");
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
    keys(&mut model, "j<CR>");
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
    keys(&mut model, "j<CR>");
    let name = model.deps.config.theme.clone();
    let want = format!("theme = \"{name}\"\n");
    assert_eq!(fs::read_to_string(&file).ok(), Some(want));
}

#[test]
fn a_cancelled_filter_goes_back_to_the_one_before() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    open(&mut model);
    keys(&mut model, "/nord<CR>/x<Esc><CR>");
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
    keys(model, "<Tab>");
    before
}

#[test]
fn a_theme_name_stepped_to_on_the_line_shows_at_once() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    let before = stepped(&mut model);
    assert_eq!(model.deps.theme, loaded(&model, "ansi"));
    assert_ne!(model.deps.theme, before);
    keys(&mut model, "<Esc>");
    assert_eq!(model.deps.theme, before);
}

#[test]
fn a_prefix_of_theme_shows_the_stepped_name_at_once() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    press(&mut model, ":th ");
    keys(&mut model, "<Tab>");
    assert_eq!(model.deps.theme, loaded(&model, "ansi"));
}

#[test]
fn the_arrows_move_through_the_names_and_enter_keeps_one() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    stepped(&mut model);
    keys(&mut model, "<Right><Down><Left>");
    let second = "catppuccin-frappe";
    assert_eq!(model.deps.theme, loaded(&model, second));
    keys(&mut model, "<CR>");
    assert!(model.line.is_none());
    assert_eq!(model.deps.config.theme, second);
    assert_eq!(model.deps.theme, loaded(&model, second));
}

#[test]
fn typing_after_a_step_brings_back_the_theme_before() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    let before = stepped(&mut model);
    press(&mut model, "x");
    assert_eq!(model.deps.theme, before);
}

#[test]
fn the_popup_and_the_line_go_back_to_the_same_theme() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    let before = model.deps.theme.clone();
    open(&mut model);
    press(&mut model, "jj");
    assert_ne!(model.deps.theme, before);
    keys(&mut model, "<Esc>");
    assert_eq!(model.deps.theme, before);
    assert!(model.previewed.is_none());
}

#[test]
fn an_unknown_configured_theme_shows_the_first_row_the_popup_selects() {
    let (_dir, mut model) = workdir_model(
        SAMPLE,
        Config {
            theme: "nonesuch".to_owned(),
            ..alone()
        },
    );
    let before = model.deps.theme.clone();
    open(&mut model);
    assert_eq!(model.deps.theme, loaded(&model, "ansi"));
    assert_ne!(model.deps.theme, before);
    keys(&mut model, "<Esc>");
    assert_eq!(model.deps.theme, before);
}

#[test]
fn view_keys_start_from_the_view_that_is_drawn() {
    let config = Config {
        theme: "tokyo-night".into(),
        ..alone()
    };
    let (_dir, mut model) = workdir_model(SAMPLE, config);
    model.update(Msg::Resize(80, 6));
    open(&mut model);
    let at = |model: &Model| model.choosing.as_ref().map(|p| (p.at, p.top));
    let rows = model.theme_rows();
    let (start, _) = at(&model).unwrap_or_default();
    assert!(start >= rows, "opens far down");
    // The view drawn ends on the cursor row.
    let drawn = start + 1 - rows;
    keys(&mut model, "<C-e>");
    assert_eq!(at(&model), Some((start, drawn + 1)), "scrolls down a row");
    keys(&mut model, "H");
    assert_eq!(at(&model), Some((drawn + 1, drawn + 1)), "the top row");
}

#[test]
fn the_cursor_of_the_filter_is_in_the_prompt_of_the_popup() {
    let (_dir, mut model) = workdir_model(SAMPLE, alone());
    model.update(Msg::Resize(60, 24));
    open(&mut model);
    press(&mut model, "/ab");
    assert_eq!(model.cursor_at(), Some((22, 14)));
    press(&mut model, &"x".repeat(70));
    assert_eq!(model.cursor_at(), Some((41, 14)));
}
