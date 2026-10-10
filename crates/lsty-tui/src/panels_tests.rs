use lsty_core::config::{Config, Filter};
use ratatui::{buffer::Buffer, layout::Rect};
use tempfile::TempDir;

use crate::{
    focus::Focus,
    layout::Size,
    model::Model,
    msg::Msg,
    test_util::{keys, press, screen, workdir_model},
};

const LIST: &str = "(A) Call client +work.acme @phone due:2026-09-30 id:c1\n\
    Plan +work/q3\nBuy milk +home\nFix tap +home\n";

fn model(config: Config, size: (u16, u16), todo: &str) -> (TempDir, Model) {
    let (dir, mut model) = workdir_model(todo, config);
    model.update(Msg::Resize(size.0, size.1));
    (dir, model)
}

fn no_detail() -> Config {
    Config {
        show_detail: false,
        ..Config::default()
    }
}

fn names(model: &Model, panel: usize) -> Vec<String> {
    let entries = &model.panels[panel].entries;
    let at = |i: usize| entries.get(i).map(|e| e.spans(40));
    (0..entries.len())
        .filter_map(at)
        .map(|spans| spans.iter().map(|s| s.text.as_ref()).collect())
        .collect()
}

#[test]
fn a_panel_cursor_takes_counts_and_stops_at_the_ends() {
    let (_dir, mut m) = model(Config::default(), (130, 30), LIST);
    keys(&mut m, "<C-w>3");
    assert_eq!(m.focus, Focus::Projects);
    press(&mut m, "2j");
    assert_eq!(m.panels[0].cursor, 2);
    press(&mut m, "G");
    assert_eq!(m.panels[0].cursor, m.panels[0].entries.len() - 1);
    press(&mut m, "gg");
    assert_eq!(m.panels[0].cursor, 0);
    press(&mut m, "3G");
    assert_eq!(m.panels[0].cursor, 2);
    press(&mut m, "99k");
    assert_eq!(m.panels[0].cursor, 0);
    assert_eq!(m.cursor, 0, "the list's cursor did not move");
}

#[test]
fn the_filters_cursor_skips_the_rule() {
    let (_dir, mut m) = model(Config::default(), (130, 30), LIST);
    keys(&mut m, "<C-w>5");
    assert_eq!(m.focus, Focus::Filters);
    press(&mut m, "3j");
    assert_eq!(m.panels[2].cursor, 3);
    press(&mut m, "j");
    assert_eq!(m.panels[2].cursor, 5, "the rule is not an entry");
    press(&mut m, "k");
    assert_eq!(m.panels[2].cursor, 3);
}

#[test]
fn folding_hides_children_and_keeps_the_cursor_on_the_parent() {
    let (_dir, mut m) = model(Config::default(), (130, 30), LIST);
    keys(&mut m, "<C-w>3");
    assert_eq!(names(&m, 0).len(), 4);
    press(&mut m, "2j");
    assert!(names(&m, 0)[2].contains(".acme"));
    press(&mut m, "h");
    let shown = names(&m, 0);
    assert_eq!(shown.len(), 2, "{shown:?}");
    assert!(shown[m.panels[0].cursor].contains("▸+work"));
    press(&mut m, "h");
    assert_eq!(names(&m, 0).len(), 2, "a folded node stays folded");
    press(&mut m, "l");
    assert_eq!(names(&m, 0).len(), 4);
    press(&mut m, "l");
    assert_eq!(names(&m, 0).len(), 4);
}

#[test]
fn a_top_level_leaf_does_not_fold() {
    let (_dir, mut m) = model(Config::default(), (130, 30), LIST);
    keys(&mut m, "<C-w>3h");
    assert_eq!(names(&m, 0).len(), 4, "+home has no parent");
    keys(&mut m, "<C-w>5h");
    assert_eq!(m.panels[2].cursor, 0);
}

#[test]
fn focus_moves_in_both_directions_and_the_sidebar_follows() {
    let (_dir, mut m) = model(Config::default(), (130, 30), LIST);
    keys(&mut m, "<C-w>j");
    assert_eq!(m.focus, Focus::List, "nothing below the list");
    keys(&mut m, "<S-Tab>");
    assert_eq!(m.focus, Focus::Filters);
    keys(&mut m, "<C-w>j");
    assert_eq!(m.focus, Focus::Filters, "the last panel stays");
    keys(&mut m, "<C-w>k<C-w>k<C-w>k");
    assert_eq!(m.focus, Focus::Projects);
    keys(&mut m, "<C-w>l");
    assert_eq!(m.focus, Focus::List);
}

#[test]
fn an_overlay_opened_by_focus_closes_with_the_focus() {
    let (_dir, mut m) = model(Config::default(), (70, 24), LIST);
    assert!(!m.sidebar_shown());
    keys(&mut m, "<C-w>3");
    assert!(m.overlay && m.sidebar_shown());
    keys(&mut m, "<C-w>5<Tab>");
    assert_eq!(m.focus, Focus::List);
    assert!(!m.sidebar_shown());
    keys(&mut m, "gs");
    assert!(m.overlay);
    m.update(Msg::Resize(100, 24));
    assert!(!m.overlay && m.docked(), "docked once there is room");
}

#[test]
fn the_filters_panel_lists_saved_filters_after_the_built_in_ones() {
    let filter = |name: &str, query: &str| Filter {
        name: name.to_owned(),
        query: query.to_owned(),
    };
    let config = Config {
        filters: vec![
            filter("Urgent", "pri:A"),
            filter("Doing", "is:doing"),
            filter("Broken", "is:bogus"),
            filter("Agents", "owner:agent1"),
        ],
        ..Config::default()
    };
    let (entries, warnings) = crate::filters::catalog(&config);
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Today",
            "Overdue",
            "Upcoming",
            "Doing",
            "Waiting",
            "No project",
            "Urgent",
            "Agents"
        ]
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].message.starts_with("filters: \"Broken\": "));
}

/// A part's cells: (x, y, w, h).
type Cells = (u16, u16, u16, u16);

/// The sidebar's width, the list and the detail panel; a part not shown is
/// `None`.
type Parts = (Option<usize>, Cells, Option<Cells>);

/// A name, a screen size, the sidebar flag, the detail panel's least width
/// (`None`: panel off), and the parts.
type Case = (&'static str, (u16, u16), (bool, Option<u64>), Parts);

const GEOMETRY: [Case; 8] = [
    (
        "wide",
        (130, 30),
        (true, Some(120)),
        (Some(15), (15, 0, 72, 29), Some((87, 0, 43, 29))),
    ),
    (
        "detail below",
        (100, 30),
        (true, Some(120)),
        (Some(15), (15, 0, 85, 19), Some((15, 19, 85, 10))),
    ),
    (
        "narrow",
        (70, 24),
        (true, Some(120)),
        (None, (0, 0, 70, 15), Some((0, 15, 70, 8))),
    ),
    (
        "no sidebar",
        (130, 30),
        (false, Some(120)),
        (None, (0, 0, 87, 29), Some((87, 0, 43, 29))),
    ),
    (
        "detail beside at the least widths",
        (40, 24),
        (false, Some(0)),
        (None, (0, 0, 20, 23), Some((20, 0, 20, 23))),
    ),
    (
        "detail below one cell narrower",
        (39, 24),
        (false, Some(0)),
        (None, (0, 0, 39, 15), Some((0, 15, 39, 8))),
    ),
    (
        "alone",
        (130, 30),
        (false, None),
        (None, (0, 0, 130, 29), None),
    ),
    (
        "too short for a panel below",
        (60, 6),
        (false, Some(120)),
        (None, (0, 0, 60, 5), None),
    ),
];

fn rect(r: Rect) -> Cells {
    (r.x, r.y, r.width, r.height)
}

#[test]
fn the_parts_of_the_screen_have_the_sizes_of_the_layout() {
    for (what, size, (side, detail), want) in GEOMETRY {
        let config = Config {
            show_sidebar: side,
            show_detail: detail.is_some(),
            detail_min_width: detail.unwrap_or(0),
            ..Config::default()
        };
        let (_dir, m) = model(config, size, LIST);
        let dims = Size {
            w: usize::from(size.0),
            h: usize::from(size.1),
        };
        let panes = m.panes(dims);
        let got = (
            panes.sidebar.map(|s| usize::from(s.rect.width)),
            rect(panes.list),
            panes.detail.map(rect),
        );
        assert_eq!(got, want, "{what}");
    }
}

#[test]
fn names_and_details_draw_control_characters_as_pictures() {
    let todo = "Fix \u{1b}[31mred +pro\u{1b}j @c\u{7}x key\u{1}:v\u{2}\n";
    let (_dir, m) = model(Config::default(), (130, 30), todo);
    let lines = screen(&m, 130, 30);
    let text = lines.join("\n");
    assert!(text.contains("+pro\u{241b}j"), "sidebar:\n{text}");
    assert!(text.contains("key\u{2401}   v\u{2402}"), "detail:\n{text}");
    let mut buf = Buffer::empty(Rect::new(0, 0, 130, 30));
    m.draw(&mut buf);
    let raw = buf.content().iter();
    assert!(
        raw.into_iter()
            .all(|c| !c.symbol().chars().any(char::is_control))
    );
}

#[test]
fn a_name_that_does_not_fit_is_cut_and_its_count_stays() {
    let todo = "Item +a-project-with-a-very-long-name-indeed\n";
    let (_dir, m) = model(no_detail(), (130, 30), todo);
    let lines = screen(&m, 130, 30);
    assert!(
        lines[1].starts_with("│ +a-project-with-a-very"),
        "{lines:?}"
    );
    assert!(lines[1].contains("… 1/1│"), "{lines:?}");
}

#[test]
fn a_search_that_empties_the_list_takes_the_focus_from_the_detail_panel() {
    let (_dir, mut m) = model(Config::default(), (130, 30), LIST);
    keys(&mut m, "<C-w>2");
    assert_eq!(m.focus, Focus::Detail);
    keys(&mut m, "/zzz<CR>");
    assert!(m.rows.is_empty());
    assert_eq!(m.focus, Focus::List);
}

#[test]
fn a_set_that_empties_the_list_takes_the_focus_from_the_detail_panel() {
    let config = Config {
        hidden_items: Vec::new(),
        ..Config::default()
    };
    let (_dir, mut m) = model(config, (130, 30), "x 2026-01-01 done\n");
    keys(&mut m, "<C-w>2");
    assert_eq!(m.focus, Focus::Detail);
    keys(&mut m, ":set hidden_items=done<CR>");
    assert!(m.rows.is_empty());
    assert_eq!(m.focus, Focus::List);
}

#[test]
fn the_overlay_closes_when_the_detail_panel_takes_the_focus() {
    let (_dir, mut m) = model(Config::default(), (62, 14), LIST);
    keys(&mut m, "<C-w>3");
    assert!(m.overlay);
    keys(&mut m, "<C-w>2");
    assert_eq!(m.focus, Focus::Detail);
    assert!(!m.overlay);
}
