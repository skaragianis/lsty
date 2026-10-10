use std::time::{Duration, Instant};

use lsty_core::{
    config::{Config, Filter, Hidden},
    keys::{Action, Mods, Named, Stroke},
};

use crate::{
    field::Field,
    model::Model,
    msg::Msg,
    sidebar::{FILTERS, PROJECTS},
    test_util::{
        alone, deps, keys, press, reload, screen, started, workdir,
        workdir_model,
    },
};

const LIST: &str = "(A) Call client +work @phone due:2026-09-29\n\
(B) Send Invoice +work.acme @email due:2026-09-27\n\
Buy milk +home t:2026-10-05\n\
x 2026-09-28 Fix bug +work\n";

fn nrs(model: &Model) -> Vec<u32> {
    model.rows.iter().map(|r| r.nr.get()).collect()
}

fn active(model: &Model, panel: usize) -> Option<String> {
    let name = model.panels[panel].active.first()?;
    Some(String::from_utf8_lossy(name).into_owned())
}

#[test]
fn the_view_combines_the_switches_the_name_and_the_search() {
    let (_dir, mut model) = workdir_model(LIST, Config::default());
    assert_eq!(nrs(&model), [1, 2, 4]);
    model.shows.flip(Hidden::Later);
    model.rebuild();
    assert_eq!(nrs(&model), [1, 2, 3, 4]);
    keys(&mut model, "<C-w>3<CR>");
    assert_eq!(active(&model, PROJECTS).as_deref(), Some("home"));
}

#[test]
fn a_project_shows_its_subtree_and_leaving_it_restores_the_rows() {
    let (_dir, mut model) = workdir_model(LIST, Config::default());
    keys(&mut model, "<C-w>3j<Space>");
    assert_eq!(active(&model, PROJECTS).as_deref(), Some("work"));
    assert_eq!(nrs(&model), [1, 2, 4]);
    keys(&mut model, "<Space>");
    assert_eq!(active(&model, PROJECTS), None);
    assert_eq!(model.focus, crate::focus::Focus::Projects);
}

#[test]
fn enter_on_a_toggle_keeps_the_focus() {
    let (_dir, mut model) = workdir_model(LIST, Config::default());
    keys(&mut model, "<C-w>5<CR>");
    assert_eq!(model.focus, crate::focus::Focus::Filters);
    assert!(!model.shows.on(Hidden::Done));
    assert_eq!(nrs(&model), [1, 2]);
}

#[test]
fn a_filter_that_asks_for_a_category_shows_it_without_the_switch() {
    let todo = "a t:2026-10-05\nb\n";
    let config = Config {
        start_filter: Some("Snoozed".to_owned()),
        filters: vec![Filter {
            name: "Snoozed".to_owned(),
            query: "is:later".to_owned(),
        }],
        ..Config::default()
    };
    let (_dir, model) = workdir_model(todo, config);
    assert_eq!(nrs(&model), [1]);
    assert!(!model.shows.on(Hidden::Later));
}

#[test]
fn a_saved_filter_with_a_built_in_name_takes_its_place() {
    let config = Config {
        start_filter: Some("Today".to_owned()),
        filters: vec![Filter {
            name: "Today".to_owned(),
            query: "pri:B".to_owned(),
        }],
        ..Config::default()
    };
    let (_dir, model) = workdir_model(LIST, config);
    assert_eq!(nrs(&model), [2]);
    let names: Vec<_> =
        model.catalog.iter().filter(|e| e.name == "Today").collect();
    assert_eq!(names.len(), 1);
}

#[test]
fn an_unknown_start_filter_is_a_warning_and_no_filter() {
    let config = Config {
        start_filter: Some("Nope".to_owned()),
        ..Config::default()
    };
    let (_dir, model) = workdir_model(LIST, config);
    assert_eq!(active(&model, FILTERS), None);
    let status = screen(&model, 80, 24).pop().unwrap_or_default();
    assert!(
        status.contains("config.toml: start_filter: no filter \"Nope\""),
        "{status}"
    );
}

#[test]
fn the_search_narrows_while_typing_and_enter_keeps_it() {
    let (_dir, mut model) = workdir_model(LIST, alone());
    keys(&mut model, "/inv");
    assert_eq!(nrs(&model), [2]);
    assert_eq!(model.mode().as_str(), "command");
    keys(&mut model, "<CR>");
    assert_eq!(model.mode().as_str(), "normal");
    assert_eq!(nrs(&model), [2]);
    let top = screen(&model, 80, 24)[0].clone();
    assert!(top.starts_with("┌ [1]─/inv "), "{top}");
}

#[test]
fn escape_goes_back_to_the_search_that_was_kept() {
    let (_dir, mut model) = workdir_model(LIST, alone());
    keys(&mut model, "/inv<CR>/zzz");
    assert_eq!(nrs(&model), [] as [u32; 0]);
    keys(&mut model, "<Esc>");
    assert_eq!(model.search, "inv");
    assert_eq!(nrs(&model), [2]);
    keys(&mut model, "/<C-u><CR>");
    assert_eq!(model.search, "");
    assert_eq!(nrs(&model), [1, 2, 4]);
}

#[test]
fn the_search_is_smartcase_text_and_not_query_syntax() {
    let (_dir, mut model) = workdir_model(LIST, alone());
    keys(&mut model, "/Inv");
    assert_eq!(nrs(&model), [2]);
    press(&mut model, "OICE");
    assert_eq!(nrs(&model), [] as [u32; 0]);
    keys(&mut model, "<C-u>+work");
    assert_eq!(nrs(&model), [1, 2, 4]);
}

#[test]
fn a_search_from_a_panel_returns_to_that_panel() {
    let (_dir, mut model) = workdir_model(LIST, Config::default());
    keys(&mut model, "<C-w>3/call<CR>");
    assert_eq!(model.focus, crate::focus::Focus::Projects);
    assert_eq!(nrs(&model), [1]);
}

#[test]
fn escape_in_normal_mode_clears_no_filter() {
    let (_dir, mut model) = workdir_model(LIST, alone());
    keys(&mut model, "/call<CR><Esc>");
    assert_eq!(nrs(&model), [1]);
}

#[test]
fn the_cursor_stays_on_its_item_or_moves_to_the_next_row() {
    let todo = "a +x\nb +y\nc +x\nd +y\n";
    let (_dir, mut model) = workdir_model(todo, alone());
    keys(&mut model, "j");
    model.panels[PROJECTS].active = vec![b"x".to_vec()];
    model.rebuild();
    assert_eq!(nrs(&model), [1, 3]);
    assert_eq!(
        model.rows[model.cursor].nr.get(),
        3,
        "next row after line 2"
    );
    model.panels[PROJECTS].active = vec![b"y".to_vec()];
    model.rebuild();
    assert_eq!(model.rows[model.cursor].nr.get(), 4);
    keys(&mut model, "G");
    model.panels[PROJECTS].active = vec![b"x".to_vec()];
    model.rebuild();
    assert_eq!(
        model.rows[model.cursor].nr.get(),
        3,
        "last row when none after"
    );
}

#[test]
fn an_active_name_that_leaves_the_file_stays_listed_with_no_items() {
    let (dir, mut model) = workdir_model("a +w\nb +x.y\n", Config::default());
    model.panels[PROJECTS].active = vec![b"x.y".to_vec()];
    std::fs::write(dir.path().join("todo.txt"), "a +w\n")
        .unwrap_or_else(|e| panic!("{e}"));
    reload(&mut model);
    let lines = screen(&model, 80, 24);
    assert!(lines.iter().any(|l| l.contains('>')), "{lines:?}");
    let text = lines.join("\n");
    assert!(text.contains("+x"), "{text}");
    assert!(text.contains("0/0"), "{text}");
    model.panels[PROJECTS].active.clear();
    model.rebuild();
    assert!(!screen(&model, 80, 24).join("\n").contains("0/0"));
}

#[test]
fn the_counts_do_not_follow_the_filters() {
    let (_dir, mut model) = workdir_model(LIST, Config::default());
    let side = |m: &Model| {
        screen(m, 80, 24)[1..4]
            .iter()
            .map(|l| l.chars().take(14).collect::<String>())
            .collect::<Vec<_>>()
    };
    let before = side(&model);
    keys(&mut model, "/call<CR>");
    assert_eq!(side(&model), before);
}

#[test]
fn the_prompt_has_a_text_cursor() {
    let (_dir, mut model) = workdir_model(LIST, alone());
    assert_eq!(model.cursor_at(), None);
    keys(&mut model, "/ab");
    // " COMMAND " (9), a space, then "/ab".
    assert_eq!(model.cursor_at(), Some((9 + 1 + 3, 23)));
}

fn field(text: &str) -> Field {
    Field::new(text)
}

#[test]
fn the_field_edits_like_a_command_line() {
    let mut f = field("foo bar");
    f.act(Action::DeleteWordBack);
    assert_eq!(f.text(), "foo ");
    f.typed(Stroke::char('x'));
    f.act(Action::LineStart);
    f.typed(Stroke::char('y'));
    f.act(Action::CursorRight);
    f.typed(Stroke::named(Named::Del));
    assert_eq!(f.text(), "yfo x");
    f.act(Action::LineEnd);
    f.act(Action::CursorLeft);
    f.typed(Stroke::named(Named::Bs));
    assert_eq!(f.text(), "yfox");
    f.act(Action::DeleteToStart);
    assert_eq!(f.text(), "x");
}

#[test]
fn control_keys_are_not_text() {
    let mut f = field("");
    f.typed(Stroke::char('\u{1b}'));
    f.typed(Stroke::char('a').with_mods(Mods::CTRL));
    assert_eq!(f.text(), "");
}

fn long_list(items: usize) -> String {
    let lines: Vec<String> = (1..=items)
        .map(|i| format!("Item {i} +p{} @c{}\n", i % 50, i % 7))
        .collect();
    lines.concat()
}

/// Each key of a search narrows ten thousand items without a pause.
#[test]
fn searching_ten_thousand_items_is_fast() {
    let (_dir, mut model) = workdir_model(&long_list(10_000), alone());
    keys(&mut model, "/");
    let mut worst = Duration::ZERO;
    for c in "item 99".chars() {
        let at = Instant::now();
        model.update(Msg::Key(Stroke::char(c)));
        worst = worst.max(at.elapsed());
    }
    eprintln!("worst key of a search over 10 000 items: {worst:?}");
    assert_eq!(model.rows.len(), 111);
    assert!(worst < Duration::from_secs(1));
}

fn two_names(items: usize) -> String {
    let lines: Vec<String> = (1..=items)
        .map(|i| {
            let name = if i % 2 == 0 { "b" } else { "a" };
            format!("Item {i:03} +{name}\n")
        })
        .collect();
    lines.concat()
}

fn first_item(model: &Model) -> String {
    let lines = screen(model, 130, 30);
    lines[1].clone()
}

/// A filter keeps the top of the view and moves it only as far as the
/// cursor needs.
#[test]
fn a_filter_moves_the_view_by_the_least_step() {
    let (_dir, mut model) = workdir_model(&two_names(100), alone());
    model.update(Msg::Resize(130, 30));
    keys(&mut model, "60j20k");
    assert_eq!(model.rows[model.cursor].nr.get(), 41);
    let top = model.top;
    model.panels[PROJECTS].active = vec![b"a".to_vec()];
    model.refresh();
    assert_eq!(model.rows[model.cursor].nr.get(), 41);
    assert_eq!(model.cursor, 20);
    assert!(model.top < top);
    assert_eq!(model.top, 20);
    assert!(
        first_item(&model).contains("Item 041"),
        "{}",
        first_item(&model)
    );
}

/// A search narrows the same way.
#[test]
fn a_search_moves_the_view_by_the_least_step() {
    let (_dir, mut model) = workdir_model(&two_names(100), alone());
    model.update(Msg::Resize(130, 30));
    keys(&mut model, "60j20k/+a");
    assert_eq!(model.rows[model.cursor].nr.get(), 41);
    assert!(
        first_item(&model).contains("Item 041"),
        "{}",
        first_item(&model)
    );
}

#[test]
fn backspace_and_delete_ignore_the_modifiers() {
    let mut f = field("abc");
    let bs = Stroke::named(Named::Bs);
    f.typed(bs.with_mods(Mods::ALT));
    f.typed(bs.with_mods(Mods::CTRL));
    assert_eq!(f.text(), "a");
}

#[test]
fn a_filter_that_asks_for_archived_items_lists_them() {
    let dir = workdir("");
    std::fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old task\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let config = Config {
        start_filter: Some("Arch".to_owned()),
        filters: vec![Filter {
            name: "Arch".to_owned(),
            query: "is:archived +nothing".to_owned(),
        }],
        ..alone()
    };
    let model = started(deps(dir.path(), config));
    let text = screen(&model, 80, 24).join("\n");
    assert!(text.contains("nothing matches the filters"), "{text}");
}
