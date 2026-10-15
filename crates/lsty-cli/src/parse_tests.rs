use lsty_core::errs::{Code, Error, HELP_HINT};

use crate::parse::{Opt, Parsed, parse};

fn split(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_owned).collect()
}

fn parsed(line: &str) -> Parsed {
    let (parsed, result) = parse(&split(line));
    if let Err(e) = result {
        panic!("{line:?}: {e}");
    }
    parsed
}

fn failed(line: &str) -> Error {
    match parse(&split(line)).1 {
        Ok(()) => panic!("{line:?} parsed"),
        Err(e) => e,
    }
}

fn command(p: &Parsed) -> Option<&'static str> {
    p.command.map(|c| c.name)
}

fn args(p: &Parsed) -> Vec<&str> {
    p.args.iter().map(String::as_str).collect()
}

/// 10.1, 10.3, 10.4, 10.14: `--` ends the options.
type Case = (&'static str, &'static str, &'static [&'static str]);

const DOUBLE_DASH: [Case; 5] = [
    ("ls invoice -- -@waiting", "ls", &["invoice", "-@waiting"]),
    ("due id:k3f9 -- -3d", "due", &["id:k3f9", "-3d"]),
    (
        "add -- fix the --top flag",
        "add",
        &["fix", "the", "--top", "flag"],
    ),
    ("ls -", "ls", &["-"]),
    ("ls -- --", "ls", &["--"]),
];

#[test]
fn double_dash_ends_the_options() {
    for (line, name, want) in DOUBLE_DASH {
        let p = parsed(line);
        assert_eq!(command(&p), Some(name), "{line}");
        assert_eq!(args(&p), want, "{line}");
        assert!(!p.opts.has(Opt::Top), "{line}");
    }
}

#[test]
fn double_dash_is_dropped_before_the_command() {
    let p = parsed("-- ls -x");
    assert_eq!(command(&p), Some("ls"));
    assert_eq!(args(&p), ["-x"]);
}

#[test]
fn json_before_after_and_in_any_case() {
    for line in ["--json ls", "ls --json", "LS --json"] {
        let p = parsed(line);
        assert_eq!(command(&p), Some("ls"), "{line}");
        assert!(p.opts.has(Opt::Json), "{line}");
        assert!(p.args.is_empty(), "{line}");
    }
}

#[test]
fn a_value_may_start_with_a_dash() {
    let p = parsed("ls --filter -@waiting");
    assert_eq!(p.opts.first(Opt::Filter), Some("-@waiting"));
}

#[test]
fn a_value_attaches_with_equals_or_follows() {
    let a = parsed("ls --filter=+work");
    let b = parsed("ls --filter +work");
    assert_eq!(a.opts, b.opts);
    assert_eq!(a.opts.first(Opt::Filter), Some("+work"));
    assert_eq!(a.args, b.args);
}

#[test]
fn an_equals_in_the_value_stays() {
    let p = parsed("ls --filter=a=b");
    assert_eq!(p.opts.first(Opt::Filter), Some("a=b"));
}

#[test]
fn id_alone_takes_no_value() {
    let p = parsed("add --id Buy milk");
    assert_eq!(p.opts.first(Opt::Id), Some(""));
    assert_eq!(args(&p), ["Buy", "milk"]);
}

#[test]
fn id_attaches_only_with_equals() {
    let p = parsed("add --id=parser Buy");
    assert_eq!(p.opts.first(Opt::Id), Some("parser"));
    assert_eq!(args(&p), ["Buy"]);
    assert_eq!(parsed("add --id= Buy").opts.first(Opt::Id), Some(""));
}

#[test]
fn a_missing_value_is_a_usage_error() {
    let e = failed("ls --filter");
    assert_eq!(e.code(), Code::Usage);
    assert_eq!(e.message(), "option --filter needs a value");
}

const USAGE_ERRORS: [(&str, &str); 5] = [
    ("ls --json=yes", "option --json takes no value"),
    ("ls --frob", "unknown option --frob"),
    ("ls --sort due --sort created", "option --sort given twice"),
    ("ls --json --json", "option --json given twice"),
    ("ls --=x", "unknown option --"),
];

#[test]
fn malformed_options_are_usage_errors() {
    for (line, message) in USAGE_ERRORS {
        let e = failed(line);
        assert_eq!(e.code(), Code::Usage, "{line}");
        assert_eq!(e.message(), message, "{line}");
        assert_eq!(e.hint(), Some("usage: lsty ls [TERM…]"), "{line}");
    }
}

const NOT_TAKEN: [(&str, &str); 6] = [
    ("ls --top", "ls does not take --top"),
    ("archive --expect x", "archive does not take --expect"),
    ("help --json", "help does not take --json"),
    ("ls --create", "ls does not take --create"),
    ("next --create", "next does not take --create"),
    ("tui --human", "tui does not take --human"),
];

#[test]
fn an_option_the_command_does_not_take_is_a_usage_error() {
    for (line, message) in NOT_TAKEN {
        let e = failed(line);
        assert_eq!(e.code(), Code::Usage, "{line}");
        assert_eq!(e.message(), message, "{line}");
    }
}

#[test]
fn common_and_command_options_are_taken() {
    let lines = [
        "ls --file x --no-config --color never --help --version --owner a",
        "next --start --create",
        "add --top --pri A --date --if-absent --id=x --expect a",
        "reorder 1 --after 2",
        "do 1 --archive --ignore-blocked --takeover",
        "show 1 --raw --human",
    ];
    for line in lines {
        parsed(line);
    }
}

#[test]
fn expect_may_repeat_in_order() {
    let p = parsed("do 3 5 --expect a --expect b");
    assert_eq!(p.opts.all(Opt::Expect), ["a", "b"]);
    assert_eq!(args(&p), ["3", "5"]);
}

const CONFLICTS: [(&str, &str); 6] = [
    (
        "do 1 --archive --no-archive",
        "--archive and --no-archive cannot be used together",
    ),
    (
        "add x --top --after 3",
        "--top and --after cannot be used together",
    ),
    (
        "add x --before 3 --after 3",
        "--after and --before cannot be used together",
    ),
    (
        "ls --json --raw",
        "--json and --raw cannot be used together",
    ),
    (
        "add x --date --no-date",
        "--date and --no-date cannot be used together",
    ),
    (
        "ls --owner a --human",
        "--owner and --human cannot be used together",
    ),
];

#[test]
fn conflicting_options_are_usage_errors() {
    for (line, message) in CONFLICTS {
        let e = failed(line);
        assert_eq!(e.code(), Code::Usage, "{line}");
        assert_eq!(e.message(), message, "{line}");
    }
}

#[test]
fn a_dash_alone_is_an_argument() {
    let p = parsed("ls -");
    assert_eq!(args(&p), ["-"]);
}

const ALIASES: [(&str, &str); 17] = [
    ("a", "add"),
    ("app", "append"),
    ("prep", "prepend"),
    ("rm", "del"),
    ("done", "do"),
    ("DONE", "do"),
    ("p", "pri"),
    ("dp", "depri"),
    ("list", "ls"),
    ("lsa", "lsa"),
    ("listall", "lsa"),
    ("lsp", "lsp"),
    ("listpri", "lsp"),
    ("lsc", "lsc"),
    ("lsprj", "lsprj"),
    ("listproj", "lsprj"),
    ("Tui", "tui"),
];

#[test]
fn aliases_name_the_canonical_command() {
    for (alias, name) in ALIASES {
        let p = parsed(alias);
        assert_eq!(command(&p), Some(name), "{alias}");
    }
}

#[test]
fn only_the_first_argument_is_the_command() {
    let p = parsed("add ls");
    assert_eq!(command(&p), Some("add"));
    assert_eq!(args(&p), ["ls"]);
}

#[test]
fn an_unknown_first_argument_is_no_command() {
    let p = parsed("frobnicate x");
    assert_eq!(command(&p), None);
    assert_eq!(args(&p), ["frobnicate", "x"]);
}

#[test]
fn errors_without_a_command_keep_the_generic_hint() {
    for line in ["--frob", "--json --raw", "--sort", "-z"] {
        let e = failed(line);
        assert_eq!(e.code(), Code::Usage, "{line}");
        assert_eq!(e.hint(), Some(HELP_HINT), "{line}");
    }
}

#[test]
fn a_command_gives_its_synopsis_as_the_hint() {
    assert_eq!(failed("add --frob").hint(), Some("usage: lsty add TEXT…"));
    assert_eq!(
        failed("do --json --raw").hint(),
        Some("usage: lsty do REFS…")
    );
}

#[test]
fn the_first_error_wins() {
    let e = failed("ls --frob --json=1");
    assert_eq!(e.message(), "unknown option --frob");
}

#[test]
fn what_was_read_comes_back_with_the_error() {
    let (p, result) = parse(&split("ls a --frob b --json"));
    assert!(result.is_err());
    assert_eq!(args(&p), ["a", "b"]);
    assert!(p.opts.has(Opt::Json));
}

#[test]
fn help_and_version_survive_a_parse_error() {
    let (p, result) = parse(&split("ls --frob --version"));
    assert!(result.is_err());
    assert!(p.opts.has(Opt::Version));
    let (p, _) = parse(&split("ls --frob -h"));
    assert!(p.opts.has(Opt::Help));
}

type Bad = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const BAD_VALUES: [Bad; 2] = [
    (
        "ls --color=sometimes",
        "--color",
        "sometimes",
        &["auto", "always", "never"],
    ),
    (
        "ls --sort size",
        "--sort",
        "size",
        &["manual", "file", "priority", "due", "created", "alpha"],
    ),
];

#[test]
fn bad_color_and_sort_are_invalid_values() {
    for (line, field, value, accepted) in BAD_VALUES {
        let e = failed(line);
        assert_eq!(e.code(), Code::InvalidValue, "{line}");
        assert_eq!(
            e.message(),
            format!(
                "{field}: \"{value}\" is not one of {}",
                accepted.join(", ")
            ),
            "{line}"
        );
        assert_eq!(e.hint(), None, "{line}");
        assert_eq!(e.fields().count(), 3, "{line}");
    }
}

#[test]
fn good_color_and_sort_values_pass() {
    for line in ["ls --color auto", "ls --color=always", "ls --sort alpha"] {
        parsed(line);
    }
    for sort in ["manual", "file", "priority", "due", "created", "alpha"] {
        parsed(&format!("ls --sort {sort}"));
    }
}

#[test]
fn short_options_combine_before_the_command() {
    let p = parsed("-tA add one");
    assert_eq!(command(&p), Some("add"));
    assert!(p.opts.has(Opt::Date) && !p.opts.has(Opt::Archive));
    let p = parsed("-Aa do 1");
    assert!(p.opts.has(Opt::NoArchive) && !p.opts.has(Opt::Archive));
    assert_eq!(parsed("-cp ls").opts.first(Opt::Color), Some("never"));
    let p = parsed("-fNvx -T -pc ls");
    assert!(p.opts.has(Opt::Color));
    assert_eq!(p.opts.first(Opt::Color), Some("always"));
    let p = parsed("-p --color=always ls");
    assert_eq!(p.opts.first(Opt::Color), Some("always"));
}

#[test]
fn short_options_flip_the_hidden_parts() {
    let p = parsed("-@+P ls");
    assert!(p.hide.contexts && p.hide.projects && p.hide.priority);
    let p = parsed("-@@ -+ ls");
    assert!(!p.hide.contexts && p.hide.projects && !p.hide.priority);
}

#[test]
fn short_options_reach_only_the_commands_that_take_them() {
    let p = parsed("-AtT ls");
    assert!(!p.opts.has(Opt::Archive) && !p.opts.has(Opt::NoDate));
    let p = parsed("-A do 1");
    assert!(p.opts.has(Opt::Archive));
}

#[test]
fn a_word_with_one_dash_after_the_command_is_a_term_or_text() {
    for (line, want) in [
        ("ls -p", &["-p"][..]),
        ("ls -@waiting", &["-@waiting"]),
        ("add -5 degrees tonight", &["-5", "degrees", "tonight"]),
        ("due id:k3f9 -3d", &["id:k3f9", "-3d"]),
        ("ls -json", &["-json"]),
        ("ls -V", &["-V"]),
        ("ls -hV", &["-hV"]),
        ("ls -d cfg", &["-d", "cfg"]),
        ("do -1", &["-1"]),
    ] {
        let p = parsed(line);
        assert_eq!(args(&p), want, "{line}");
        assert!(
            !p.opts.has(Opt::Help) && !p.opts.has(Opt::Version),
            "{line}"
        );
    }
    let p = parsed("ls -h");
    assert!(p.opts.has(Opt::Help));
    assert_eq!(args(&p).len(), 0);
    let p = parsed("ls -@a -h");
    assert!(p.opts.has(Opt::Help));
    assert_eq!(args(&p), ["-@a"]);
    let p = parsed("ls -- -@");
    assert_eq!(args(&p), ["-@"]);
}

#[test]
fn short_d_takes_the_rest_of_the_cluster_or_the_next_word() {
    for line in ["-d cfg ls", "-dcfg ls", "-fd cfg ls", "-d x -d cfg ls"] {
        let p = parsed(line);
        assert_eq!(p.config.as_deref(), Some("cfg"), "{line}");
        assert_eq!(command(&p), Some("ls"), "{line}");
    }
    assert_eq!(parsed("-pdcfg ls").opts.first(Opt::Color), Some("never"));
    assert_eq!(parsed("ls").config, None);
    for line in ["-d", "-fd"] {
        let e = failed(line);
        assert_eq!(e.message(), "option -d needs a file", "{line}");
        assert_eq!(e.code(), Code::Usage);
    }
}

#[test]
fn short_options_refuse_n_and_unknown_letters() {
    assert_eq!(
        failed("-n ls").message(),
        "option -n is not supported: lsty always keeps line numbers"
    );
    assert_eq!(failed("-fn ls").code(), Code::Usage);
    for line in ["-z ls", "-pz ls"] {
        assert!(
            failed(line).message().starts_with("unknown option -"),
            "{line}"
        );
    }
}
