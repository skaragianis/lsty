use std::{fs, io, path::Path};

use serde_json::Value;

use crate::run_tests::{Fake, Ran};

const TODO: &str = "Call client +work id:c1\nPay rent id:r1\n";

/// Runs `wait` on a list whose watch, as it starts, leaves the todo and
/// done files as `todo` and `done`.
fn wait_after(todo: &'static str, done: &'static str, args: &[&str]) -> Ran {
    let change = move |dir: &Path| -> io::Result<()> {
        fs::write(dir.join("todo.txt"), todo)?;
        fs::write(dir.join("done.txt"), done)
    };
    Fake::new()
        .file("todo.txt", TODO)
        .file("done.txt", "")
        .var("TODO_FILE", "todo.txt")
        .watching(change)
        .run(
            &[&["wait"], args, &["--no-config", "--timeout", "50ms"]].concat(),
        )
}

fn assert_returned(ran: &Ran) {
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out, "hint: run \"lsty show id:c1\"\n");
}

#[test]
fn an_edit_ends_the_wait() {
    let ran = wait_after(
        "Call client +home id:c1\nPay rent id:r1\n",
        "",
        &["id:c1"],
    );
    assert_returned(&ran);
}

#[test]
fn a_start_ends_the_wait() {
    let todo = "Call client +work id:c1 started:2026-09-29T12:00Z\n\
                Pay rent id:r1\n";
    assert_returned(&wait_after(todo, "", &["id:c1"]));
}

#[test]
fn an_archive_ends_the_wait() {
    let ran = wait_after(
        "Pay rent id:r1\n",
        "Call client +work id:c1\n",
        &["id:c1"],
    );
    assert_returned(&ran);
}

#[test]
fn a_delete_ends_the_wait() {
    assert_returned(&wait_after("Pay rent id:r1\n", "", &["id:c1"]));
}

#[test]
fn a_new_line_number_does_not_end_the_wait() {
    let todo = "Pay rent id:r1\nNew\nCall client +work id:c1\n";
    let ran = wait_after(todo, "", &["id:c1"]);
    assert_eq!(ran.code, 5, "{}", ran.out);
    assert_eq!(ran.err, "lsty: no task changed within 50ms\n");
}

#[test]
fn a_change_to_another_task_does_not_end_the_wait() {
    let ran = wait_after(
        "Call client +work id:c1\nPay rent x id:r1\n",
        "",
        &["id:c1"],
    );
    assert_eq!(ran.code, 5, "{}", ran.out);
}

#[test]
fn a_copy_of_the_id_counts_as_a_change() {
    let todo = "Call client +work id:c1\nPay rent id:r1\nCopy id:c1\n";
    let ran = wait_after(todo, "", &["id:c1"]);
    assert_returned(&ran);
}

#[test]
fn the_hint_names_every_id_watched() {
    let ran = wait_after("Call client +work id:c1\n", "", &["id:c1,id:r1"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out, "hint: run \"lsty show id:c1 id:r1\"\n");
}

#[test]
fn the_json_names_the_action_and_the_next_call() {
    let ran =
        wait_after("Pay rent id:r1\n", "", &["id:c1", "id:r1", "--json"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let got: Value = serde_json::from_str(&ran.out).unwrap();
    assert_eq!(got["action"], "wait");
    assert_eq!(got["next"], "lsty show id:c1 id:r1");
}

#[test]
fn zero_in_any_unit_sets_no_limit() {
    for zero in ["0", "0s", "0ms", "0h"] {
        let ran = Fake::new()
            .file("todo.txt", TODO)
            .var("TODO_FILE", "todo.txt")
            .watching(|dir| {
                fs::write(dir.join("todo.txt"), "Pay rent id:r1\n")
            })
            .run(&["wait", "id:c1", "--timeout", zero, "--no-config"]);
        assert_returned(&ran);
    }
}

#[test]
fn without_a_watch_the_wait_still_times_out() {
    let ran = Fake::new()
        .file("todo.txt", TODO)
        .var("TODO_FILE", "todo.txt")
        .run(&["wait", "id:c1", "--timeout", "1ms", "--json", "--no-config"]);
    assert_eq!(ran.code, 5);
    let got: Value = serde_json::from_str(&ran.out).unwrap();
    assert_eq!(got["error"]["code"], "timeout", "{got}");
}

#[test]
fn a_line_in_a_comma_list_is_refused() {
    let ran = Fake::new()
        .file("todo.txt", TODO)
        .var("TODO_FILE", "todo.txt")
        .run(&["wait", "id:c1,2", "--no-config"]);
    assert_eq!(ran.code, 2);
    assert!(
        ran.err
            .starts_with("lsty: wait takes id: references, not 2\n")
    );
}

#[test]
fn the_next_call_runs_as_written() {
    let fake = Fake::new()
        .file(
            "todo.txt",
            "x 2026-09-28 Call client id:c1\nPay rent id:r1\n",
        )
        .var("TODO_FILE", "todo.txt");
    let ran = fake.run(&["wait", "id:c1,id:r1", "--json", "--no-config"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let got: Value = serde_json::from_str(&ran.out).unwrap();
    let next = got["next"].as_str().unwrap();
    let words: Vec<&str> = next.split(' ').skip(1).collect();
    let shown = fake.run(&[&words[..], &["--no-config"]].concat());
    assert_eq!(shown.code, 0, "{next}: {}", shown.err);
    assert!(
        shown.out.contains("Call client") && shown.out.contains("Pay rent")
    );
}
