use lsty_core::{
    config::{Config, DetailPosition, Filter},
    errs::Source,
};
use ratatui::{buffer::Buffer, layout::Rect};
use tempfile::TempDir;

use crate::{
    focus::Focus,
    layout::Size,
    model::Model,
    msg::Msg,
    test_util::{
        deps, keys, press, screen, started, todo_of, workdir, workdir_model,
    },
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

/// Types `typed` on the second of three items with the list focused, or
/// with the detail panel focused after the cursor is there; gives the file
/// and the focus.
fn typed_on_second(typed: &str, pane: bool) -> (String, Focus) {
    let todo = "A id:a\nB due:2026-10-01 id:b\nC\nD\n";
    let (dir, mut m) = model(Config::default(), (130, 30), todo);
    keys(&mut m, "j");
    if pane {
        keys(&mut m, "<C-w>2");
    }
    keys(&mut m, typed);
    (todo_of(dir.path()), m.focus)
}

#[test]
fn the_detail_panel_acts_on_the_item_shown_as_the_list_does() {
    const ROWS: [&str; 24] = [
        "x",
        "t",
        "tt",
        "tT",
        "dd",
        "J",
        "K",
        "gp",
        "gP",
        "gd2026-10-05<CR>",
        "gtk:v<CR>",
        "gTdue:2026-10-01<CR>",
        "ccNew<CR>",
        "iX<CR>",
        "aX<CR>",
        "IX<CR>",
        "oNew<CR>",
        "ONew<CR>",
        "yyp",
        "yyP",
        "xu",
        "xu<C-r>",
        "x.",
        "2x",
    ];
    for typed in ROWS {
        let list = typed_on_second(typed, false);
        let pane = typed_on_second(typed, true);
        assert_eq!(pane.0, list.0, "{typed}");
        assert_eq!(pane.1, Focus::Detail, "{typed}");
    }
}

#[test]
fn the_detail_panel_keeps_its_own_keys() {
    let todo = "A\nB file:a.md\nC\n";
    let (dir, mut m) = model(Config::default(), (130, 30), todo);
    keys(&mut m, "j<C-w>2");
    for typed in ["j", "k", "gg", "G", "<Tab>", "<C-w>1"] {
        keys(&mut m, typed);
        assert_eq!(todo_of(dir.path()), todo, "{typed}");
        assert_eq!(m.cursor, 1, "{typed}");
    }
    assert_eq!(m.focus, Focus::List, "<C-w>1 returns to the list");
    keys(&mut m, "<C-w>2<CR>");
    assert_eq!(
        todo_of(dir.path()),
        todo,
        "<CR> opens a link, edits nothing"
    );
}

#[test]
fn deleting_the_last_item_from_the_detail_panel_returns_to_the_list() {
    let (dir, mut m) = model(Config::default(), (130, 30), "A\n");
    keys(&mut m, "<C-w>2dd");
    assert_eq!(todo_of(dir.path()).trim(), "");
    assert_eq!(m.focus, Focus::List);
}

#[test]
fn a_finished_item_has_no_note_beside_its_due_date() {
    const ROWS: [(&str, bool); 6] = [
        ("Fix due:2026-09-20", true),
        ("Fix due:2026-09-29", true),
        ("Fix due:2026-10-09", true),
        ("x 2026-09-28 Fix due:2026-09-20", false),
        ("x 2026-09-28 Fix due:2026-09-29", false),
        ("x 2026-09-28 Fix due:2026-10-09", false),
    ];
    for (line, note) in ROWS {
        let (_dir, m) = model(Config::default(), (130, 30), line);
        let text = screen(&m, 130, 30).join("\n");
        let due = &line[line.len() - 10..];
        assert!(text.contains(&format!("  {due}")), "{line}:\n{text}");
        let noted = text.contains("(today)")
            || text.contains(" overdue)")
            || text.contains("(in ");
        assert_eq!(noted, note, "{line}:\n{text}");
    }
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

/// The metadata lines, their white space squeezed.
fn fields(m: &Model) -> Vec<String> {
    let doc = m.detail_doc(40, 20);
    doc.lines[doc.desc + 1..]
        .iter()
        .map(|l| {
            let line: String = l.iter().map(|s| s.text.as_ref()).collect();
            line.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .collect()
}

#[test]
fn the_detail_panel_numbers_a_todo_item_not_an_archived_one() {
    let dir = workdir("A\n");
    std::fs::write(dir.path().join("done.txt"), "x 2026-09-01 Old\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let config = Config {
        hidden_items: Vec::new(),
        ..Config::default()
    };
    let mut m = started(deps(dir.path(), config));
    m.update(Msg::Resize(130, 30));
    assert_eq!(fields(&m)[..2], ["state todo", "nr 1"]);
    press(&mut m, "j");
    assert_eq!(m.rows[m.cursor].source, Source::Done);
    assert!(fields(&m).iter().all(|f| !f.starts_with("nr")));
}

/// `<C-w>j` then `<C-w>k` from the list, and where each leaves the focus.
fn down_and_up(config: Config, size: (u16, u16)) -> (Focus, Focus) {
    let (_dir, mut m) = model(config, size, LIST);
    keys(&mut m, "<C-w>j");
    let down = m.focus;
    m.set_focus(Focus::Detail);
    keys(&mut m, "<C-w>k");
    (down, m.focus)
}

#[test]
fn ctrl_w_j_and_k_move_between_the_list_and_a_detail_panel_below_it() {
    let bottom = Config {
        detail_position: DetailPosition::Bottom,
        ..Config::default()
    };
    let hidden = Config {
        detail_position: DetailPosition::Bottom,
        ..no_detail()
    };
    for (name, config, size, want) in [
        ("bottom", bottom, (130, 30), (Focus::Detail, Focus::List)),
        (
            "narrow",
            Config::default(),
            (100, 30),
            (Focus::Detail, Focus::List),
        ),
        (
            "right",
            Config::default(),
            (130, 30),
            (Focus::List, Focus::Detail),
        ),
        ("hidden", hidden, (130, 30), (Focus::List, Focus::List)),
    ] {
        assert_eq!(down_and_up(config, size), want, "{name}");
    }
}

/// Where `<C-w>l` and `<C-w>h` leave the focus from the list, `<C-w>h`
/// from the detail panel and `<C-w>l` from a sidebar panel.
fn sideways(config: Config, size: (u16, u16)) -> [Focus; 4] {
    let (_dir, mut m) = model(config.clone(), size, LIST);
    keys(&mut m, "<C-w>l");
    let right = m.focus;
    let (_dir, mut m) = model(config.clone(), size, LIST);
    keys(&mut m, "<C-w>h");
    let left = m.focus;
    let (_dir, mut m) = model(config.clone(), size, LIST);
    m.set_focus(Focus::Detail);
    keys(&mut m, "<C-w>h");
    let from_detail = m.focus;
    let (_dir, mut m) = model(config, size, LIST);
    keys(&mut m, "<C-w>3<C-w>l");
    [right, left, from_detail, m.focus]
}

#[test]
fn ctrl_w_h_and_l_reach_the_detail_panel_only_beside_the_list() {
    let under = |sidebar| Config {
        detail_position: DetailPosition::Bottom,
        show_sidebar: sidebar,
        ..Config::default()
    };
    let no_sidebar = Config {
        show_sidebar: false,
        ..Config::default()
    };
    let hidden = Config {
        show_detail: false,
        ..Config::default()
    };
    let (l, p, d) = (Focus::List, Focus::Projects, Focus::Detail);
    for (name, config, size, want) in [
        ("beside", Config::default(), (130, 30), [d, p, l, l]),
        ("beside, no sidebar", no_sidebar, (130, 30), [d, p, l, l]),
        ("under", under(true), (130, 30), [l, p, p, l]),
        ("under, no sidebar", under(false), (130, 30), [l, p, p, l]),
        ("narrow", Config::default(), (100, 30), [l, p, p, l]),
        ("hidden", hidden, (130, 30), [l, p, p, l]),
    ] {
        assert_eq!(sideways(config, size), want, "{name}");
    }
}

fn shown(model: &Model, panel: usize) -> Vec<String> {
    let rows = names(model, panel);
    let words =
        |r: &String| r.split_whitespace().collect::<Vec<_>>().join(" ");
    rows.iter().map(words).collect()
}

/// The config, the todo file, the keys, the panel rows after them. `{s}`
/// is the sigil of the panel.
type Gone = (Config, &'static str, &'static str, &'static [&'static str]);

fn gone_cases() -> Vec<(&'static str, Gone)> {
    let shown_done = Config {
        hidden_items: Vec::new(),
        ..Config::default()
    };
    let hidden_done = Config {
        hidden_items: vec![lsty_core::config::Hidden::Done],
        ..Config::default()
    };
    let auto = Config {
        auto_archive: true,
        ..shown_done.clone()
    };
    let one = "A {s}solo\nB keep\n";
    let sole = |todo: &'static str,
                keys: &'static str,
                rows: &'static [&'static str]| {
        (shown_done.clone(), todo, keys, rows)
    };
    vec![
        ("delete", sole(one, "dd", &[])),
        ("edit", sole(one, "A<C-w><CR>", &[])),
        ("untag key", sole(one, "gT{s}solo<CR>", &[])),
        ("untag command", sole(one, ":untag 1 {s}solo<CR>", &[])),
        ("done shown", sole(one, "x", &["{s}solo 0/1"])),
        ("done hidden", (hidden_done, one, "x", &["{s}solo 0/1"])),
        ("archive", (auto, one, "x", &[])),
        ("add", sole("B keep\n", "oC {s}solo<CR>", &["{s}solo 1/1"])),
        ("undo add", sole("B keep\n", "oC {s}solo<CR>u", &[])),
        ("undo delete", sole(one, "ddu", &["{s}solo 1/1"])),
        ("redo delete", sole(one, "ddu<C-r>", &[])),
        ("last child", sole("A {s}w.acme\nB keep\n", "dd", &[])),
        (
            "parent used",
            sole("A {s}w.acme\nB {s}w\n", "dd", &["{s}w 1/1"]),
        ),
        (
            "sibling left",
            sole("A {s}w.a\nB {s}w.b\n", "dd", &["▾{s}w 1/1", ".b 1/1"]),
        ),
    ]
}

// A name with no item left leaves its panel in the redraw of the write;
// a done item keeps it, shown or hidden.
#[test]
fn a_name_with_no_use_left_leaves_its_panel() {
    for (name, (config, todo, ks, rows)) in gone_cases() {
        for (panel, s) in [(0, "+"), (1, "@")] {
            let todo = todo.replace("{s}", s);
            let (_dir, mut m) = model(config.clone(), (130, 30), &todo);
            keys(&mut m, &ks.replace("{s}", s));
            let want: Vec<String> =
                rows.iter().map(|r| r.replace("{s}", s)).collect();
            assert_eq!(shown(&m, panel), want, "{name} {s}");
        }
    }
}

// Another program rewriting the file.
#[test]
fn a_rewrite_that_drops_the_last_use_drops_the_name() {
    let (dir, mut m) = model(Config::default(), (130, 30), "A +solo @solo\n");
    std::fs::write(dir.path().join("todo.txt"), "B keep\n")
        .unwrap_or_else(|e| panic!("{e}"));
    crate::test_util::reload(&mut m);
    assert!(shown(&m, 0).is_empty() && shown(&m, 1).is_empty());
}

// A chosen name stays with its count until it is unchosen; the filter is
// never dropped behind the user's back.
#[test]
fn a_chosen_name_stays_until_it_is_unchosen() {
    let todo = "A +solo\nB keep +x\n";
    let (_dir, mut m) = model(Config::default(), (130, 30), todo);
    keys(&mut m, "<C-w>3<CR><C-w>1dd");
    assert_eq!(shown(&m, 0), [">+solo 0/0", "+x 1/1"]);
    assert_eq!(m.panels[0].active.len(), 1);
    keys(&mut m, "<C-w>3<CR>");
    assert_eq!(shown(&m, 0), ["+x 1/1"]);
    assert_eq!(m.panels[0].active.len(), 0);
}

// The panel cursor lands on the row that takes the place of its row,
// else on the last.
#[test]
fn the_panel_cursor_lands_on_the_row_that_takes_its_place() {
    let todo = "A +a\nB +b\nC +c\n";
    let (_dir, mut m) = model(Config::default(), (130, 30), todo);
    keys(&mut m, "<C-w>3j<C-w>1jdd");
    assert_eq!(shown(&m, 0), ["+a 1/1", "+c 1/1"]);
    assert_eq!(m.panels[0].cursor, 1, "+c took its place");
    keys(&mut m, "Gdd");
    assert_eq!(shown(&m, 0), ["+a 1/1"]);
    assert_eq!(m.panels[0].cursor, 0, "the last row");
}
