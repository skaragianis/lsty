//! Transcripts that must fail, and how the harness reports them.

use std::path::Path;

use lsty_scripttest::{Outcome, Suite};

fn suite() -> Suite {
    Suite::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        env!("CARGO_BIN_EXE_lsty-stub"),
    )
}

/// The failure log of `source`, or `None` if it did not fail.
fn failure(source: &str) -> Option<String> {
    match suite().run_source("case", source) {
        Outcome::Fail(log) => Some(log),
        Outcome::Pass | Outcome::Skip(_) => None,
    }
}

fn passes(source: &str) {
    assert_eq!(suite().run_source("case", source), Outcome::Pass);
}

#[test]
fn exit_names_both_codes() {
    let log = failure("exit 3 lsty 2\n").expect("want a failure");
    assert!(log.contains("exit code is 2, want 3"), "{log}");
    assert!(log.contains("FAIL: case:1:"), "{log}");
}

#[test]
fn bound_id_must_match_later() {
    let log = failure(
        "exec lsty 0 added k3f9\nmatch stdout want\n\
         exec lsty 0 added a1b2\nmatch stdout want\n\
         -- want --\nadded <id1>\n",
    )
    .expect("want a failure");
    assert!(
        log.contains("FAIL: case:4: stdout does not match want"),
        "{log}"
    );
}

#[test]
fn id_placeholder_needs_an_id() {
    for text in ["K3F9", "k3i9", "k3f"] {
        let log = failure(&format!(
            "exec lsty 0 {text}\nmatch stdout want\n-- want --\n<id1>\n"
        ))
        .expect("want a failure");
        assert!(log.contains("does not match"), "{text}: {log}");
    }
}

#[test]
fn cmpjson_shows_a_diff() {
    let log = failure(
        "cmpjson got.json want.json\n\
         -- got.json --\n{\"a\": 2}\n-- want.json --\n{\"a\": 1}\n",
    )
    .expect("want a failure");
    assert!(log.contains("JSON values differ"), "{log}");
    assert!(log.contains("+  \"a\": 2"), "{log}");
}

#[test]
fn negation_and_conditions() {
    passes("! exec lsty 1\n! stdout x\n[!unix] exec nope\n");
    let log = failure("! exec lsty 0\n").expect("want a failure");
    assert!(log.contains("unexpected command success"), "{log}");
    let log = failure("exec lsty 1\n").expect("want a failure");
    assert!(log.contains("unexpected command failure"), "{log}");
}

#[test]
fn unknown_command_fails() {
    let log = failure("lsty 0\n").expect("want a failure");
    assert!(log.contains("unknown command \"lsty\""), "{log}");
}

#[test]
fn archive_names_are_unique() {
    let log = failure("-- a --\nx\n-- a --\ny\n").expect("want a failure");
    assert!(log.contains("would overwrite"), "{log}");
}

#[test]
fn background_and_wait() {
    passes(
        "[!exec:sh] skip\n\
         exec sh -c 'echo late' &\nwait\nstdout '^late$'\n",
    );
}

#[test]
fn programs_are_on_path() {
    passes(
        "[!exec:sh] skip\n\
         exec sh -c 'lsty 0 from sh'\nstdout '^from sh$'\n",
    );
}

#[test]
fn skip_reports_its_reason() {
    assert_eq!(
        suite().run_source("case", "skip 'not here'\nexec lsty 1\n"),
        Outcome::Skip("not here".to_owned())
    );
}
