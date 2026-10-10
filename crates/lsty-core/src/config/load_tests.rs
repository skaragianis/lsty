use std::{
    collections::BTreeMap, ffi::OsString, fs, path::Path, time::Duration,
};

use super::{
    Config, Filter, Hidden, LineNumbers, Paths, Value, Warning, find_paths,
    is_option, load, set,
};
use crate::todotxt::Priority;

fn paths_for(file: &Path) -> Paths {
    Paths {
        config_file: file.to_path_buf(),
        keys_file: "keys.toml".into(),
        themes_dir: "themes".into(),
        state_dir: "state".into(),
        explicit: false,
    }
}

fn load_text(text: impl AsRef<[u8]>) -> (Config, Vec<Warning>) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.toml");
    fs::write(&file, text).unwrap();
    load(&paths_for(&file), Path::new("/"), false)
}

fn letter(c: char) -> Priority {
    Priority::new(c as u8).expect("letter")
}

fn messages(warnings: &[Warning]) -> Vec<String> {
    warnings
        .iter()
        .map(|w| format!("{}|{}", w.source, w.message))
        .collect()
}

#[test]
fn no_file_gives_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("none.toml");
    let (cfg, warnings) = load(&paths_for(&file), Path::new("/"), false);
    assert_eq!((cfg, warnings), (Config::default(), Vec::new())); // 07.1
}

#[test]
fn defaults() {
    let want = Config {
        // 07.2
        theme: "catppuccin-mocha".into(),
        leader: "<Space>".into(),
        include_creation: true,
        auto_archive: false,
        keep_priority: true,
        keep_adding: true,
        mouse: true,
        default_priority: None,
        priority_range: letter('D'),
        upcoming_days: 7,
        hidden_items: vec![Hidden::Later, Hidden::Archive],
        show_sidebar: true,
        sidebar_min_width: 80,
        show_detail: true,
        inline_projects: false,
        inline_contexts: false,
        detail_min_width: 120,
        line_numbers: LineNumbers::Hybrid,
        lock_timeout: Duration::from_secs(2),
        claim_stale_after: Duration::from_hours(4),
        start_filter: None,
        paste_command: None,
        default_file: None,
        hints: BTreeMap::new(),
        filters: Vec::new(),
        no_config: false,
    };
    assert_eq!(Config::default(), want);
}

const EVERY_OPTION: &str = r#"
theme = "latte"
leader = ","
include_creation = false
auto_archive = true
keep_priority = false
keep_adding = false
mouse = false
default_priority = "c"
priority_range = "F"
upcoming_days = 3
hidden_items = ["blocked", "done"]
show_sidebar = false
sidebar_min_width = 0
show_detail = false
inline_projects = true
inline_contexts = true
detail_min_width = 10
line_numbers = "relative"
lock_timeout = "5s"
claim_stale_after = "1h30m"
start_filter = "Urgent"
paste_command = "wl-paste"
default_file = "~/todo.txt"

[hints]
normal = ["help"]

[filters]
Urgent = "pri:A"
"#;

#[test]
fn every_option_is_read() {
    let (cfg, warnings) = load_text(EVERY_OPTION); // 07.3
    let mut want = Config {
        theme: "latte".into(),
        leader: ",".into(),
        include_creation: false,
        auto_archive: true,
        keep_priority: false,
        keep_adding: false,
        mouse: false,
        default_priority: Some(letter('C')),
        priority_range: letter('F'),
        upcoming_days: 3,
        hidden_items: vec![Hidden::Blocked, Hidden::Done],
        show_sidebar: false,
        sidebar_min_width: 0,
        show_detail: false,
        inline_projects: true,
        inline_contexts: true,
        detail_min_width: 10,
        line_numbers: LineNumbers::Relative,
        lock_timeout: Duration::from_secs(5),
        claim_stale_after: Duration::from_mins(90),
        start_filter: Some("Urgent".into()),
        paste_command: Some("wl-paste".into()),
        default_file: Some("~/todo.txt".into()),
        ..Config::default()
    };
    want.hints.insert("normal".into(), vec!["help".into()]);
    want.filters.push(Filter {
        name: "Urgent".into(),
        query: "pri:A".into(),
    });
    assert_eq!(cfg, want);
    assert_eq!(warnings, Vec::new());
}

#[test]
fn no_config_reads_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.toml");
    fs::write(&file, "auto_archive = true").unwrap();
    let (cfg, warnings) = load(&paths_for(&file), Path::new("/"), true); // 07.8
    assert!(!cfg.auto_archive && cfg.no_config);
    assert_eq!(warnings, Vec::new());
}

#[test]
fn syntax_error_gives_defaults_and_one_warning() {
    let (cfg, warnings) =
        load_text("theme = \"x\"\nauto_archive = true\n= oops\n"); // 07.9
    assert_eq!(cfg, Config::default());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.starts_with("line 3:"), "{warnings:?}");
}

/// Text and the line its syntax error is reported on.
const SYNTAX_LINES: &[(&str, &str)] = &[
    ("a = \"\"\"\nx\n", "line 2:"),
    ("a = \"\"\"\nx\n\n", "line 3:"),
    ("a = 1\nb = \"\"\"\nx", "line 3:"),
    ("a = 1\n  y\n", "line 2:"),
    ("a = 1\r\nb = = 3\r\n", "line 2:"),
    ("a = = 1\nb = 2\nc = 3\n", "line 1:"),
    (
        "b = 99999999999999999999\na = 99999999999999999998\n",
        "line 1: 99999999999999999999 is out",
    ),
    (
        "t = {b = 99999999999999999999, a = 99999999999999999998}\n",
        "line 1: 99999999999999999999 is out",
    ),
    (
        "x = [99999999999999999999]\n[t]\na = 99999999999999999998\n",
        "line 1: 99999999999999999999 is out",
    ),
    (
        "a = 99999999999999999999\n",
        "line 1: 99999999999999999999 is out",
    ),
    (
        "a = 1\n[t]\nb = 0xFFFFFFFFFFFFFFFFF\n",
        "line 3: 0xFFFFFFFFFFFFFFFFF is out",
    ),
    ("a = 1e999\n", "line 1: 1e999 is out of range for float64"),
    (
        "a = 1_0e999\n",
        "line 1: 1_0e999 is out of range for float64",
    ),
    (
        "x = 1\n[hints]\nnormal = [1e999]\n",
        "line 3: 1e999 is out of range for float64",
    ),
    (
        "a = 1e999\nb = 99999999999999999999\n",
        "line 1: 1e999 is out of range for float64",
    ),
];

#[test]
fn syntax_error_lines() {
    for (text, want) in SYNTAX_LINES {
        let (cfg, warnings) = load_text(text);
        assert_eq!(cfg, Config::default(), "{text:?}");
        assert_eq!(warnings.len(), 1, "{text:?}");
        assert!(
            warnings[0].message.starts_with(want),
            "{text:?}: {warnings:?}"
        );
    }
}

#[test]
fn invalid_utf8_is_a_syntax_warning() {
    let (cfg, warnings) = load_text(b"a = 1\nb = \xff\n");
    assert_eq!(cfg, Config::default());
    assert_eq!(
        messages(&warnings),
        ["config.toml|line 2: invalid UTF-8 byte: 0xff"]
    );
}

/// Name, text and the warnings it gives.
type WarningRow = (&'static str, &'static str, &'static [&'static str]);

const WARNINGS: &[WarningRow] = &[
    (
        "07.10 unknown option",
        "colour = \"red\"\nauto_archive = true",
        &[r#"unknown option "colour""#],
    ),
    (
        "07.10 unknown table",
        "[extra]\nx = 1",
        &[r#"unknown option "extra""#],
    ),
    (
        "dotted unknown",
        "colour.x = 1",
        &[r#"unknown option "colour""#],
    ),
    (
        "nested unknown",
        "[colour.x]\ny = 1",
        &[r#"unknown option "colour""#],
    ),
    (
        "dotted unknown warns once",
        "colour.x = 1\ncolour.y = 2",
        &[r#"unknown option "colour""#],
    ),
    (
        "dotted under string",
        "theme.x = 1",
        &["theme: expected string"],
    ),
    (
        "nested filter",
        "[filters]\nM.n = \"3\"",
        &["filters.M: expected string"],
    ),
    (
        "07.11 wrong type",
        r#"auto_archive = "yes""#,
        &["auto_archive: expected bool"],
    ),
    (
        "07.12 line numbers",
        r#"line_numbers = "both""#,
        &[
            r#"line_numbers: "both" is not one of hybrid, absolute, relative, off"#,
        ],
    ),
    (
        "07.13 hidden items",
        r#"hidden_items = ["later", "foo", "done", "later"]"#,
        &[r#"hidden_items: unknown item "foo""#],
    ),
    (
        "hidden items wrong type",
        r#"hidden_items = "later""#,
        &["hidden_items: expected list of strings"],
    ),
    (
        "hidden items with a number",
        r#"hidden_items = ["later", 1]"#,
        &["hidden_items: expected list of strings"],
    ),
    (
        "07.20 filter wrong type",
        "[filters]\nUrgent = 5",
        &["filters.Urgent: expected string"],
    ),
    (
        "hints wrong type",
        "[hints]\nnormal = 5",
        &["hints.normal: expected list of strings"],
    ),
    (
        "hint key with a control character",
        "[hints]\n\"a\\u001bb\" = 5",
        &["hints.a\\x1bb: expected list of strings"],
    ),
    (
        "filter key with a control character",
        "[filters]\n\"a\\u001bb\" = 5",
        &["filters.a\\x1bb: expected string"],
    ),
    (
        "filters not a table",
        "filters = 5",
        &["filters: expected table"],
    ),
    (
        "hints not a table",
        "hints = []",
        &["hints: expected table"],
    ),
    (
        "07.24 file order",
        "upcoming_days = 0\nauto_archive = 1\ncolour = 2",
        &[
            "upcoming_days: 0 must be 1 or more",
            "auto_archive: expected bool",
            r#"unknown option "colour""#,
        ],
    ),
    (
        "dotted hints around another option",
        "hints.a = [\"x\"]\ncolour = \"r\"\nhints.b = 5",
        &[
            r#"unknown option "colour""#,
            "hints.b: expected list of strings",
        ],
    ),
    (
        "table split across the file",
        "[filters]\nb=\"1\"\n[colour]\n[filters.c]\nd = 1",
        &[r#"unknown option "colour""#, "filters.c: expected string"],
    ),
    (
        "names are quoted as text",
        "\"a\\\"b\\n\" = 1",
        &[r#"unknown option "a\"b\n""#],
    ),
    (
        "bad duration",
        r#"lock_timeout = "fast""#,
        &[
            r#"lock_timeout: "fast" is not a duration such as 500ms, 2s, 4h or 2d"#,
        ],
    ),
    (
        "float is not an integer",
        "detail_min_width = 1.5",
        &["detail_min_width: expected integer"],
    ),
    (
        "non-ascii letter that uppercases to S",
        r#"priority_range = "ſ""#,
        &[r#"priority_range: "ſ" must be one letter A-Z"#],
    ),
    (
        "non-ascii letter whose low byte is A",
        r#"priority_range = "Ł""#,
        &[r#"priority_range: "Ł" must be one letter A-Z"#],
    ),
];

#[test]
fn warnings_rows() {
    for (name, text, want) in WARNINGS {
        let (_, warnings) = load_text(text);
        let want: Vec<String> =
            want.iter().map(|m| format!("config.toml|{m}")).collect();
        assert_eq!(messages(&warnings), want, "{name}");
    }
}

/// Text, a reader of the field, and what it reads, as text.
type ValueRow = (
    &'static str,
    &'static str,
    fn(&Config) -> String,
    &'static str,
    usize,
);

fn auto_archive(c: &Config) -> String {
    c.auto_archive.to_string()
}
fn line_numbers(c: &Config) -> String {
    c.line_numbers.as_str().into()
}
fn hidden(c: &Config) -> String {
    c.hidden_items
        .iter()
        .map(|h| h.as_str())
        .collect::<Vec<_>>()
        .join(",")
}
fn default_priority(c: &Config) -> String {
    c.default_priority
        .map(|p| p.to_string())
        .unwrap_or_default()
}
fn priority_range(c: &Config) -> String {
    c.priority_range.to_string()
}
fn upcoming(c: &Config) -> String {
    c.upcoming_days.to_string()
}
fn sidebar(c: &Config) -> String {
    c.sidebar_min_width.to_string()
}
fn detail(c: &Config) -> String {
    c.detail_min_width.to_string()
}
fn lock_timeout(c: &Config) -> String {
    format!("{:?}", c.lock_timeout)
}
fn claim(c: &Config) -> String {
    (c.claim_stale_after.as_secs() / 3600).to_string()
}
fn filters(c: &Config) -> String {
    c.filters
        .iter()
        .map(|f| format!("{}={}", f.name, f.query))
        .collect::<Vec<_>>()
        .join(",")
}
fn hints(c: &Config) -> String {
    format!("{:?}", c.hints)
}
fn default_file(c: &Config) -> String {
    format!("{:?}", c.default_file)
}
fn start_filter(c: &Config) -> String {
    format!("{:?}", c.start_filter)
}
fn paste(c: &Config) -> String {
    format!("{:?}", c.paste_command)
}

const VALUES: &[ValueRow] = &[
    (
        "07.10 the rest is read",
        "colour = \"red\"\nauto_archive = true",
        auto_archive,
        "true",
        1,
    ),
    (
        "07.11 default kept",
        r#"auto_archive = "yes""#,
        auto_archive,
        "false",
        1,
    ),
    (
        "07.12 default kept",
        r#"line_numbers = "both""#,
        line_numbers,
        "hybrid",
        1,
    ),
    (
        "line numbers hybrid",
        r#"line_numbers = "hybrid""#,
        line_numbers,
        "hybrid",
        0,
    ),
    (
        "line numbers absolute",
        r#"line_numbers = "absolute""#,
        line_numbers,
        "absolute",
        0,
    ),
    (
        "hidden items archive",
        r#"hidden_items = ["archive"]"#,
        hidden,
        "archive",
        0,
    ),
    (
        "line numbers off",
        r#"line_numbers = "off""#,
        line_numbers,
        "off",
        0,
    ),
    (
        "07.13 hidden items",
        r#"hidden_items = ["later", "foo", "done", "later"]"#,
        hidden,
        "later,done",
        1,
    ),
    ("hidden items empty", "hidden_items = []", hidden, "", 0),
    (
        "07.14 lowercase",
        r#"default_priority = "b""#,
        default_priority,
        "B",
        0,
    ),
    (
        "07.14 two letters",
        r#"default_priority = "BC""#,
        default_priority,
        "",
        1,
    ),
    (
        "07.14 digit",
        r#"default_priority = "1""#,
        default_priority,
        "",
        1,
    ),
    (
        "dotless i",
        r#"default_priority = "ı""#,
        default_priority,
        "",
        1,
    ),
    (
        "empty default priority",
        r#"default_priority = """#,
        default_priority,
        "",
        0,
    ),
    (
        "07.15 lowercase range",
        r#"priority_range = "f""#,
        priority_range,
        "F",
        0,
    ),
    (
        "07.15 empty range",
        r#"priority_range = """#,
        priority_range,
        "D",
        1,
    ),
    ("07.16 zero days", "upcoming_days = 0", upcoming, "7", 1),
    (
        "07.16 negative days",
        "upcoming_days = -1",
        upcoming,
        "7",
        1,
    ),
    ("07.16 valid days", "upcoming_days = 14", upcoming, "14", 0),
    ("negative width", "sidebar_min_width = -1", sidebar, "80", 1),
    ("float width", "detail_min_width = 1.5", detail, "120", 1),
    (
        "07.17 milliseconds",
        r#"lock_timeout = "500ms""#,
        lock_timeout,
        "500ms",
        0,
    ),
    (
        "07.17 zero",
        r#"lock_timeout = "0s""#,
        lock_timeout,
        "2s",
        1,
    ),
    (
        "07.17 not a duration",
        r#"lock_timeout = "fast""#,
        lock_timeout,
        "2s",
        1,
    ),
    ("07.17 integer", "lock_timeout = 2", lock_timeout, "2s", 1),
    ("07.18 days", r#"claim_stale_after = "2d""#, claim, "48", 0),
    (
        "07.19 filters keep file order",
        "[filters]\nUrgent = \"a\"\nAgents = \"b\"",
        filters,
        "Urgent=a,Agents=b",
        0,
    ),
    (
        "07.19 order is not alphabetical",
        "[filters]\nZ = \"a\"\nA = \"b\"\nM = \"c\"",
        filters,
        "Z=a,A=b,M=c",
        0,
    ),
    (
        "07.20 bad filter dropped",
        "[filters]\nUrgent = 5\nOk = \"x\"",
        filters,
        "Ok=x",
        1,
    ),
    (
        "07.21 hints",
        "[hints]\nnormal = [\"toggle_complete\", \"help\"]",
        hints,
        r#"{"normal": ["toggle_complete", "help"]}"#,
        0,
    ),
    (
        "07.23 empty default file",
        r#"default_file = """#,
        default_file,
        "None",
        0,
    ),
    (
        "empty start filter",
        r#"start_filter = """#,
        start_filter,
        "None",
        0,
    ),
    (
        "empty paste command",
        r#"paste_command = """#,
        paste,
        "None",
        0,
    ),
    (
        "default file kept",
        r#"default_file = "~/t.txt""#,
        default_file,
        r#"Some("~/t.txt")"#,
        0,
    ),
];

#[test]
fn values_rows() {
    for (name, text, field, want, count) in VALUES {
        let (cfg, warnings) = load_text(text);
        assert_eq!(field(&cfg), *want, "{name}");
        assert_eq!(warnings.len(), *count, "{name}: {warnings:?}");
    }
}

#[test]
fn explicit_missing_file_warns() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("none.toml");
    let text = missing.to_str().unwrap().to_owned();
    let vars = |k: &str| (k == "LSTY_CONFIG").then(|| OsString::from(&text));
    let paths = find_paths(&vars, Some(Path::new("/h")));
    let (cfg, warnings) = load(&paths, Path::new("/"), false); // 07.22
    assert_eq!(cfg, Config::default());
    assert_eq!(
        messages(&warnings),
        [format!("config.toml|{text} not found")]
    );
}

#[test]
fn unreadable_file_warns_with_the_system_message() {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, warnings) = load(&paths_for(dir.path()), Path::new("/"), false);
    assert_eq!(cfg, Config::default());
    assert_eq!(
        messages(&warnings),
        [format!(
            "config.toml|read {}: is a directory",
            dir.path().display()
        )]
    );
}

#[test]
fn warning_formats_for_stderr() {
    let (_, warnings) = load_text(r#"auto_archive = "yes""#);
    assert_eq!(
        warnings[0].to_string(), // 07.25
        "lsty: warning: config.toml: auto_archive: expected bool"
    );
}

fn value(text: &str) -> Value {
    match text {
        "true" => Value::Bool(true),
        "0" => Value::Int(0),
        _ if text.starts_with('[') => Value::List(
            text.trim_matches(|c| c == '[' || c == ']')
                .split(", ")
                .map(|i| i.trim_matches('"').to_owned())
                .collect(),
        ),
        _ if text.starts_with('{') => Value::Other,
        _ => Value::Str(text.trim_matches('"').to_owned()),
    }
}

/// Name, option, value, and the reasons (empty when accepted).
type SetRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const SETS: &[SetRow] = &[
    ("bool", "auto_archive", "true", &[]),
    (
        "wrong type",
        "auto_archive",
        "\"yes\"",
        &["auto_archive: expected bool"],
    ),
    ("letter uppercased", "priority_range", "\"f\"", &[]),
    (
        "too small",
        "upcoming_days",
        "0",
        &["upcoming_days: 0 must be 1 or more"],
    ),
    (
        "choice",
        "line_numbers",
        "\"both\"",
        &[
            r#"line_numbers: "both" is not one of hybrid, absolute, relative, off"#,
        ],
    ),
    ("list", "hidden_items", "[\"blocked\", \"blocked\"]", &[]),
    (
        "bad list item",
        "hidden_items",
        "[\"later\", \"foo\"]",
        &[r#"hidden_items: unknown item "foo""#],
    ),
    (
        "unknown",
        "colour",
        "\"red\"",
        &[r#"unknown option "colour""#],
    ),
    (
        "tables are not set",
        "hints",
        "{}",
        &[r#"unknown option "hints""#],
    ),
];

#[test]
fn set_rows() {
    let start = Config::default();
    for (name, option, raw, want) in SETS {
        let got = set(&start, option, &value(raw));
        match (got, want.is_empty()) {
            (Ok(changed), true) => assert_ne!(changed, start, "{name}"),
            (Err(reasons), false) => assert_eq!(reasons, *want, "{name}"),
            (got, _) => panic!("{name}: {got:?}"),
        }
    }
}

#[test]
fn set_changes_only_the_named_option() {
    let start = Config::default();
    let got = set(&start, "priority_range", &Value::Str("f".into())).unwrap();
    assert_eq!(
        got,
        Config {
            priority_range: letter('F'),
            ..start.clone()
        }
    );
    let got =
        set(&start, "hidden_items", &value("[\"blocked\", \"blocked\"]"))
            .unwrap();
    assert_eq!(got.hidden_items, [Hidden::Blocked]);
}

#[test]
fn is_option_rows() {
    assert!(is_option("auto_archive"));
    assert!(is_option("hints"));
    assert!(is_option("filters"));
    assert!(!is_option("frob"));
}

#[test]
fn relative_file_is_read_from_the_given_folder() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("rel.toml"), "claim_stale_after = \"2h\"\n")
        .unwrap();
    let paths = paths_for(Path::new("rel.toml"));
    let (cfg, warnings) = load(&paths, dir.path(), false);
    assert_eq!(
        (cfg.claim_stale_after, warnings),
        (Duration::from_hours(2), Vec::new())
    );
}
