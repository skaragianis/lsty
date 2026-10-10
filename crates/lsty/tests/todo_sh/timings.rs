//! How long each program takes for each command, as percentiles.

use std::{
    collections::BTreeMap, fmt::Write as _, sync::Mutex, time::Duration,
};

use super::random::percentile;

/// The percentiles the table shows.
const PERCENTILES: [usize; 4] = [50, 90, 95, 99];

/// Call times, by command and then by program.
type Calls = BTreeMap<(&'static str, &'static str), Vec<Duration>>;

/// Call times gathered from every thread.
#[derive(Default)]
pub(crate) struct Timings(Mutex<Calls>);

impl Timings {
    /// Records one call of `command` by `program`.
    pub(crate) fn add(
        &self,
        program: &'static str,
        command: &'static str,
        took: Duration,
    ) {
        if let Ok(mut calls) = self.0.lock() {
            calls.entry((command, program)).or_default().push(took);
        }
    }

    /// One row per command and program, then one per program over every
    /// command; times in milliseconds.
    pub(crate) fn table(&self) -> String {
        let Ok(calls) = self.0.lock() else {
            return String::new();
        };
        let mut out =
            format!("{:<12} {:<8} {:>6}", "command", "program", "calls");
        for p in PERCENTILES {
            put!(out, " {:>8}", format!("p{p}"));
        }
        out.push('\n');
        let mut all: BTreeMap<&str, Vec<Duration>> = BTreeMap::new();
        for (&(command, program), times) in calls.iter() {
            row(&mut out, command, program, times);
            all.entry(program).or_default().extend(times);
        }
        for (program, times) in &all {
            row(&mut out, "all", program, times);
        }
        out
    }
}

fn row(out: &mut String, command: &str, program: &str, times: &[Duration]) {
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    put!(out, "{command:<12} {program:<8} {:>6}", sorted.len());
    for p in PERCENTILES {
        let ms = percentile(&sorted, p).as_secs_f64() * 1000.0;
        put!(out, " {ms:>8.1}");
    }
    out.push('\n');
}
