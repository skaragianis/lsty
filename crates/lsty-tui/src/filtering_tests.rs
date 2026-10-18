use lsty_core::{
    config::{Config, Filter, Hidden},
    keys::{Action, Mods, Named, Stroke},
};

use crate::{
    connective::{Conn, Term},
    field::Field,
    model::Model,
    msg::Msg,
    sidebar::{CONTEXTS, FILTERS, PROJECTS},
    test_util::{
        alone, deps, keys, plain, press, refiltering, reload, screen, settled,
        started, workdir, workdir_model,
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
    Some(String::from_utf8_lossy(&name.name).into_owned())
}

#[test]
fn the_view_combines_the_switches_the_name_and_the_search() {
    let (_dir, mut model) = workdir_model(LIST, plain());
    assert_eq!(nrs(&model), [1, 2, 4]);
    model.shows.flip(Hidden::Later);
    model.rebuild();
    assert_eq!(nrs(&model), [1, 2, 3, 4]);
    keys(&mut model, "<C-w>3x");
    assert_eq!(active(&model, PROJECTS).as_deref(), Some("home"));
}

#[test]
fn done_tasks_are_hidden_at_launch_until_the_done_switch_is_on() {
    let (_dir, mut model) = workdir_model(LIST, Config::default());
    assert_eq!(nrs(&model), [1, 2]);
    keys(&mut model, "<C-w>5");
    settled(&mut model, "x");
    assert_eq!(nrs(&model), [1, 2, 4]);
}

#[test]
fn a_project_shows_its_subtree_and_leaving_it_restores_the_rows() {
    let (_dir, mut model) = workdir_model(LIST, plain());
    keys(&mut model, "<C-w>3jx");
    assert_eq!(active(&model, PROJECTS).as_deref(), Some("work"));
    assert_eq!(nrs(&model), [1, 2, 4]);
    keys(&mut model, "xx");
    assert_eq!(active(&model, PROJECTS), None);
    assert_eq!(model.focus, crate::focus::Focus::Projects);
}

/// What a panel key must leave alone: the rows, the switches and the
/// names chosen.
fn state(m: &Model) -> (Vec<u32>, bool, usize) {
    let chosen = [PROJECTS, CONTEXTS, FILTERS]
        .iter()
        .map(|&i| m.panels[i].active.len())
        .sum();
    (nrs(m), m.shows.on(Hidden::Done), chosen)
}

#[test]
fn enter_in_a_panel_goes_to_the_list_and_changes_nothing() {
    for typed in ["<C-w>3", "<C-w>4", "<C-w>5", "<C-w>5G"] {
        let (_dir, mut m) = workdir_model(LIST, plain());
        keys(&mut m, typed);
        let before = state(&m);
        keys(&mut m, "<CR>");
        assert_eq!(m.focus, crate::focus::Focus::List, "{typed}");
        assert_eq!(state(&m), before, "{typed}");
    }
}

#[test]
fn space_does_what_x_does_in_the_panels() {
    for typed in ["<C-w>3", "<C-w>4", "<C-w>5"] {
        let (_dir, mut m) = workdir_model(LIST, plain());
        keys(&mut m, typed);
        let before = state(&m);
        keys(&mut m, "x");
        let by_x = state(&m);
        assert_ne!(by_x, before, "{typed}");
        let (_dir, mut m) = workdir_model(LIST, plain());
        keys(&mut m, typed);
        keys(&mut m, "<Space>");
        assert_eq!(state(&m), by_x, "{typed}");
    }
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
        ..plain()
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
        ..plain()
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
        ..plain()
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
    let (_dir, mut model) = workdir_model(LIST, plain());
    keys(&mut model, "<C-w>3/call<CR>");
    assert_eq!(model.focus, crate::focus::Focus::Projects);
    assert_eq!(nrs(&model), [1]);
}

#[test]
fn escape_drops_a_count_or_prefix_before_it_clears_the_search() {
    let (_dir, mut model) = workdir_model(LIST, alone());
    keys(&mut model, "/call<CR>");
    assert_eq!(nrs(&model), [1]);
    for pending in ["2", "g", "d", "2g"] {
        keys(&mut model, pending);
        keys(&mut model, "<Esc>");
        assert_eq!(nrs(&model), [1], "{pending} <Esc> drops the pending");
    }
    keys(&mut model, "<Esc>");
    assert_eq!(nrs(&model), [1, 2, 4]);
    keys(&mut model, "/call<CR>");
    keys(&mut model, "<C-c>");
    assert_eq!(nrs(&model), [1, 2, 4]);
}

#[test]
fn the_cursor_stays_on_its_task_or_moves_to_the_next_row() {
    let todo = "a +x\nb +y\nc +x\nd +y\n";
    let (_dir, mut model) = workdir_model(todo, alone());
    keys(&mut model, "j");
    model.panels[PROJECTS].active = vec![Term::and(*b"x")];
    model.rebuild();
    assert_eq!(nrs(&model), [1, 3]);
    assert_eq!(
        model.rows[model.cursor].nr.get(),
        3,
        "next row after line 2"
    );
    model.panels[PROJECTS].active = vec![Term::and(*b"y")];
    model.rebuild();
    assert_eq!(model.rows[model.cursor].nr.get(), 4);
    keys(&mut model, "G");
    model.panels[PROJECTS].active = vec![Term::and(*b"x")];
    model.rebuild();
    assert_eq!(
        model.rows[model.cursor].nr.get(),
        3,
        "last row when none after"
    );
}

#[test]
fn an_active_name_that_leaves_the_file_stays_listed_with_no_tasks() {
    let (dir, mut model) = workdir_model("a +w\nb +x.y\n", plain());
    model.panels[PROJECTS].active = vec![Term::and(*b"x.y")];
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
    let (_dir, mut model) = workdir_model(LIST, plain());
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

fn two_names(tasks: usize) -> String {
    let lines: Vec<String> = (1..=tasks)
        .map(|i| {
            let name = if i % 2 == 0 { "b" } else { "a" };
            format!("Task {i:03} +{name}\n")
        })
        .collect();
    lines.concat()
}

fn first_task(model: &Model) -> String {
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
    model.panels[PROJECTS].active = vec![Term::and(*b"a")];
    model.refresh();
    assert_eq!(model.rows[model.cursor].nr.get(), 41);
    assert_eq!(model.cursor, 20);
    assert!(model.top < top);
    assert_eq!(model.top, 20);
    assert!(
        first_task(&model).contains("Task 041"),
        "{}",
        first_task(&model)
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
        first_task(&model).contains("Task 041"),
        "{}",
        first_task(&model)
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
fn a_filter_that_asks_for_archived_tasks_lists_them() {
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
    assert!(text.contains("nothing matches"), "{text}");
}

const SPREAD: &str = "A +p @a\nB +p @b\nC +q @a @b\nD +q @c\nE +r\n";

fn chip_texts(model: &Model) -> Vec<String> {
    model.chips().into_iter().map(|c| c.text).collect()
}

// Each case: the keys in the Contexts panel (the cursor starts on `@a`),
// the chip line, and the rows the list holds.
#[test]
fn x_steps_contexts_through_the_connectives_and_the_list_follows() {
    let cases = [
        ("x", "@a", vec![1, 3]),
        ("xx", "-@a", vec![2, 4, 5]),
        ("xxx", "", vec![1, 2, 3, 4, 5]),
        ("xjx", "@a|@b", vec![1, 2, 3]),
        ("xjxx", "@a @b", vec![3]),
        ("xjxxx", "@a -@b", vec![1]),
        ("xxjx", "-@a|@b", vec![4, 5]),
        ("xjxxxx", "@a", vec![1, 3]),
        ("jjx", "@c", vec![4]),
        ("xjjx", "@a|@c", vec![1, 3, 4]),
    ];
    for (steps, chips, rows) in cases {
        let (_dir, mut model) = workdir_model(SPREAD, plain());
        settled(&mut model, &format!("<C-w>4{steps}"));
        assert_eq!(nrs(&model), rows, "{steps}");
        assert_eq!(chip_texts(&model).join(" "), chips, "{steps}");
    }
}

#[test]
fn the_two_panels_join_with_a_space() {
    let (_dir, mut model) = workdir_model(SPREAD, plain());
    settled(&mut model, "<C-w>3jx<C-w>4jjx");
    assert_eq!(chip_texts(&model), ["+q", "@c"]);
    assert_eq!(nrs(&model), [4]);
    keys(&mut model, "<C-w>3xx");
    assert_eq!(chip_texts(&model), ["@c"]);
}

// A chosen name filters by its bytes, whatever the filter language would
// make of them.
#[test]
fn a_name_the_filter_language_cannot_spell_filters_as_it_is() {
    let todo: &[u8] = b"X +a|b\nY +a\nZ +b\nW +a\"b\nV +caf\xe9\n";
    let names = [(b"a|b".as_slice(), [1]), (b"a\"b", [4]), (b"caf\xe9", [5])];
    for (name, rows) in names {
        let dir = workdir(todo);
        let mut model = started(deps(dir.path(), plain()));
        model.panels[PROJECTS].active = vec![Term::and(name)];
        model.reshow();
        assert_eq!(nrs(&model), rows, "{}", String::from_utf8_lossy(name));
    }
}

#[test]
fn the_list_holds_what_the_connectives_say() {
    let (_dir, mut model) = workdir_model(SPREAD, plain());
    let with = |conns: &[(&str, Conn)]| -> Vec<Term> {
        conns
            .iter()
            .map(|&(name, conn)| Term {
                name: name.into(),
                conn,
            })
            .collect()
    };
    let cases = [
        (with(&[("a", Conn::Not), ("b", Conn::Or)]), vec![4, 5]),
        (with(&[("b", Conn::Not)]), vec![1, 4, 5]),
        (with(&[("a", Conn::And), ("b", Conn::Or)]), vec![1, 2, 3]),
    ];
    for (active, rows) in cases {
        model.panels[CONTEXTS].active = active;
        model.reshow();
        assert_eq!(nrs(&model), rows);
    }
}

#[test]
fn the_order_of_the_chips_is_the_order_the_names_were_chosen() {
    let (_dir, mut model) = workdir_model(SPREAD, plain());
    keys(&mut model, "<C-w>4jjxkxkx");
    assert_eq!(chip_texts(&model), ["@c|@b|@a"]);
    keys(&mut model, "x");
    assert_eq!(chip_texts(&model), ["@c|@b", "@a"]);
}

#[test]
fn a_chip_click_clears_its_whole_term() {
    let (_dir, mut model) = workdir_model(SPREAD, plain());
    keys(&mut model, "<C-w>4xjxjxx");
    assert_eq!(chip_texts(&model), ["@a|@b", "@c"]);
    model.clear_chip(0);
    assert_eq!(chip_texts(&model), ["@c"]);
    assert_eq!(model.panels[CONTEXTS].active[0].conn, Conn::And);
}

// A panel step changes the marks and the chip line at once and the rows
// 50 ms after the last step, whatever came between.
#[test]
fn the_list_follows_the_last_of_several_panel_steps() {
    let (_dir, mut model) = workdir_model(SPREAD, plain());
    let mut timers = Vec::new();
    for key in ["<C-w>4", "x", "x"] {
        timers.extend(refiltering(&keys(&mut model, key)));
    }
    assert_eq!(timers.len(), 2);
    assert_eq!(chip_texts(&model), ["-@a"]);
    assert_eq!(nrs(&model).len(), 5);
    model.update(timers.remove(0));
    assert_eq!(nrs(&model).len(), 5);
    model.update(timers.remove(0));
    assert_eq!(nrs(&model).len(), 3);
    assert!(!model.refilter.owed);
}

#[test]
fn any_other_key_refilters_first() {
    let (_dir, mut model) = workdir_model(SPREAD, plain());
    keys(&mut model, "<C-w>4x");
    assert_eq!(nrs(&model), [1, 2, 3, 4, 5]);
    keys(&mut model, "<CR>");
    assert_eq!(nrs(&model), [1, 3]);
    keys(&mut model, "<C-w>4x");
    keys(&mut model, "/");
    assert_eq!(nrs(&model), [2, 4, 5]);
}

#[test]
fn the_refilter_timer_leaves_the_message_alone() {
    let (_dir, mut model) = workdir_model(SPREAD, plain());
    let step = keys(&mut model, "<C-w>4x");
    model.update(Msg::Edited(Err("exit status 1".to_owned())));
    let said = model.message.clone();
    assert!(said.is_some());
    model.update(refiltering(&step).unwrap_or_else(|| panic!("no timer")));
    assert_eq!(nrs(&model), [1, 3]);
    assert_eq!(model.message, said);
}
