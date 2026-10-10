//! A stand-in for `lsty` that the harness's own transcripts run.
//!
//! `lsty-stub CODE ARGS…` prints its arguments on one line and exits with
//! CODE. Then, for each argument `env:NAME` it prints `NAME=value` or
//! `NAME unset`, and for `tty` whether stdin is a terminal.

use std::{
    io::{self, IsTerminal, Write},
    process::ExitCode,
};

fn main() -> ExitCode {
    #[expect(
        clippy::disallowed_methods,
        reason = "the stub is a process entry point"
    )]
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (code, out) = stub(&args, io::stdin().is_terminal());
    match io::stdout().write_all(out.as_bytes()) {
        Ok(()) => ExitCode::from(code),
        Err(_) => ExitCode::from(1),
    }
}

fn stub(args: &[String], stdin_tty: bool) -> (u8, String) {
    let Some((code, rest)) = args.split_first() else {
        return (2, String::new());
    };
    let Ok(code) = code.parse() else {
        return (2, String::new());
    };
    let mut out = rest.join(" ");
    out.push('\n');
    for arg in rest {
        out.push_str(&describe(arg, stdin_tty));
    }
    (code, out)
}

fn describe(arg: &str, stdin_tty: bool) -> String {
    if let Some(name) = arg.strip_prefix("env:") {
        #[expect(
            clippy::disallowed_methods,
            reason = "the stub reports its own environment"
        )]
        let value = std::env::var(name);
        return match value {
            Ok(value) => format!("{name}={value}\n"),
            Err(_) => format!("{name} unset\n"),
        };
    }
    match (arg, stdin_tty) {
        ("tty", true) => "stdin is a terminal\n".to_owned(),
        ("tty", false) => "stdin is not a terminal\n".to_owned(),
        _ => String::new(),
    }
}
