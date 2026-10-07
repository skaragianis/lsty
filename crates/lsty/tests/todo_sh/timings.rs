//! How long each program takes for each command, as percentiles.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::Duration;

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
    /// command; times in milliseconds. `parallel` is how many sequences
    /// ran at once, since they compete for the machine.
    pub(crate) fn table(&self, parallel: usize) -> String {
        let Ok(calls) = self.0.lock() else {
            return String::new();
        };
        let mut out = format!(
            "call times in ms ({} build of lsty, {parallel} sequences at once)\n\
             {:<12} {:<8} {:>6}",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            "command",
            "program",
            "calls"
        );
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

/// The nearest-rank percentile `p` of `sorted`: the smallest time that at
/// least `p`% of the calls took no longer than.
fn percentile(sorted: &[Duration], p: usize) -> Duration {
    let rank = (sorted.len() * p).div_ceil(100).max(1);
    sorted.get(rank - 1).copied().unwrap_or_default()
}

/// Checks `percentile` on a known list; runs whether or not todo.sh is
/// there.
pub(crate) fn check_percentiles() -> Result<(), String> {
    let ms = Duration::from_millis;
    let sorted: Vec<Duration> = (1..=10).map(ms).collect();
    let cases = [
        (percentile(&sorted, 50), ms(5)),
        (percentile(&sorted, 90), ms(9)),
        (percentile(&sorted, 95), ms(10)),
        (percentile(&sorted, 99), ms(10)),
        (percentile(&[ms(7)], 50), ms(7)),
        (percentile(&[], 50), Duration::ZERO),
    ];
    for (i, (got, want)) in cases.iter().enumerate() {
        if got != want {
            return Err(format!("case {i}: got {got:?}, want {want:?}"));
        }
    }
    Ok(())
}
