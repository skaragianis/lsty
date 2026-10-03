use super::run_tests::Fake;

const WARNING_OPEN: &str = "\x1b[38;2;249;226;175mlsty: warning:\x1b[m";

fn themed(config: &str) -> Fake {
    Fake::new()
        .file("todo.txt", "(A) Call +work\n")
        .file(".config/lsty/config.toml", config)
}

#[test]
fn theme_warnings_follow_the_config_warnings_and_are_printed_once() {
    let fake = themed("theme = \"nope\"\nauto_archive = \"yes\"\n");
    let ran = fake.run(&["ls", "--color=always"]);
    let lines: Vec<&str> = ran.err.lines().collect();
    assert_eq!(lines.len(), 2, "{}", ran.err);
    assert!(
        lines[0].contains("config.toml: auto_archive"),
        "{}",
        ran.err
    );
    assert!(lines[0].starts_with(WARNING_OPEN), "{}", ran.err);
    assert!(lines[1].contains("config.toml: theme: no theme \"nope\""));
}

#[test]
fn no_colour_means_no_theme_and_no_theme_warning() {
    let fake = themed("theme = \"nope\"\n");
    for args in [&["ls", "--color=never"][..], &["ls"]] {
        let ran = fake.run(args);
        assert_eq!(ran.err, "", "{args:?}");
        assert!(!ran.out.contains('\x1b'), "{args:?}");
    }
}

#[test]
fn theme_warnings_are_printed_when_only_stderr_is_coloured() {
    let fake = themed("theme = \"nope\"\n").ttys([false, false, true]);
    let ran = fake.run(&["ls"]);
    assert!(ran.err.starts_with(WARNING_OPEN), "{}", ran.err);
    assert!(ran.err.contains("theme: no theme \"nope\""), "{}", ran.err);
    assert!(!ran.out.contains('\x1b'), "{}", ran.out);
}

#[test]
fn the_tui_loads_no_theme_and_prints_no_theme_warning() {
    let fake =
        themed("theme = \"nope\"\nauto_archive = \"yes\"\n").ttys([true; 3]);
    let ran = fake.run(&["tui"]);
    assert_eq!(ran.tui.len(), 1);
    assert!(ran.err.contains("auto_archive"), "{}", ran.err);
    assert!(!ran.err.contains("no theme"), "{}", ran.err);
}

#[test]
fn json_is_never_coloured() {
    let ran = themed("").run(&["ls", "--json", "--color=always"]);
    assert!(!ran.out.contains('\x1b'), "{}", ran.out);
    let ran = themed("").run(&["show", "99", "--json", "--color=always"]);
    assert!(!ran.out.contains('\x1b'), "{}", ran.out);
    assert_eq!(ran.err, "");
}

#[test]
fn a_colour_stderr_colours_the_error_and_paints_the_hint_as_text() {
    let ran = themed("").run(&["frob", "--color=always"]);
    assert!(
        ran.err.starts_with("\x1b[1;38;2;243;139;168mlsty:\x1b[m "),
        "{}",
        ran.err
    );
    assert!(
        ran.err.contains("\x1b[38;2;205;214;244mhint: "),
        "{}",
        ran.err
    );
}
