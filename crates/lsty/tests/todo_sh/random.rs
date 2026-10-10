//! Shared by `todo_sh` and `bench`: the random lines their files start
//! from, shell quoting, percentiles and the host's settings.

use std::{str::FromStr, time::Duration};

/// The `PATH` of every program under test: the system's tools.
pub(crate) const SYSTEM_PATH: &str =
    "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin";

#[expect(
    clippy::disallowed_methods,
    reason = "the test, not lsty, reads its settings from the host"
)]
pub(crate) fn host_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// A number setting, if set; a value that does not parse is an error.
pub(crate) fn number<T: FromStr>(name: &str) -> Result<Option<T>, String> {
    host_var(name)
        .map(|v| v.parse().map_err(|_| format!("{name}: not a number: {v}")))
        .transpose()
}

/// `arg` as one word of a shell command line.
pub(crate) fn quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', "'\\''"))
}

/// The nearest-rank percentile `p` of `sorted`: the smallest time that at
/// least `p`% of the calls took no longer than.
pub(crate) fn percentile(sorted: &[Duration], p: usize) -> Duration {
    let rank = (sorted.len() * p).div_ceil(100).max(1);
    sorted.get(rank - 1).copied().unwrap_or_default()
}

/// splitmix64: small, and the same sequence on every machine.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`, or 0 when `n` is 0.
    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() >> 32) as usize % n.max(1)
    }

    /// True once in `n`.
    pub(crate) fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    pub(crate) fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
        from.get(self.below(from.len())).copied().unwrap_or("")
    }
}

pub(crate) const WORDS: &[&str] = &[
    "call", "buy", "milk", "write", "report", "fix", "review", "plan", "send",
    "mail", "tax", "gym", "book", "pay", "rent", "clean", "the", "a", "to",
    "for", "bike", "garden", "email", "draft", "ab", "b2", "2026", "10",
    "report", "Call", "MILK",
];
/// Values of `due:` and `t:` that are not dates.
pub(crate) const BAD_DATES: &[&str] = &["x", "1", "abc", "2026-13-45"];
pub(crate) const PROJECTS: &[&str] =
    &["+home", "+work", "+tax", "+a", "+p1", "+Home"];
pub(crate) const CONTEXTS: &[&str] =
    &["@phone", "@desk", "@home", "@a", "@Phone"];
pub(crate) const KEYS: &[&str] = &["est", "k", "ref"];
pub(crate) const VALUES: &[&str] =
    &["1", "2026-10-01", "2026-09-29", "x", "abc", "v2"];
pub(crate) const DATES: &[&str] =
    &["2026-01-15", "2026-09-28", "2026-09-29", "2025-12-31"];

pub(crate) fn token(rng: &mut Rng) -> String {
    match rng.below(12) {
        0 => rng.pick(PROJECTS).to_owned(),
        1 => rng.pick(CONTEXTS).to_owned(),
        2 => format!("{}:{}", rng.pick(KEYS), rng.pick(VALUES)),
        4 => format!("{}:{}", rng.pick(&["due", "t"]), rng.pick(DATES)),
        3 => rng.pick(DATES).to_owned(),
        _ => rng.pick(WORDS).to_owned(),
    }
}

/// Up to `most` tokens. The first is never a date: a date in front of the
/// text is a creation date, which `lsty` does not let a text edit change.
pub(crate) fn words(rng: &mut Rng, most: usize) -> String {
    let n = 1 + rng.below(most);
    let mut list = vec![rng.pick(WORDS).to_owned()];
    list.extend((1..n).map(|_| token(rng)));
    list.join(" ")
}

/// Now and then, a `due:` or `t:` tag whose value is not a date.
pub(crate) fn bad_tag(rng: &mut Rng) -> Option<String> {
    rng.one_in(12).then(|| {
        format!("{}:{}", rng.pick(&["due", "t"]), rng.pick(BAD_DATES))
    })
}

/// A priority letter; todo.sh and lsty both take it in lower case too.
pub(crate) fn letter(rng: &mut Rng) -> char {
    let letters = ['A', 'B', 'C', 'D', 'Z', 'b', 'z'];
    letters
        .get(rng.below(letters.len()))
        .copied()
        .unwrap_or('A')
}

pub(crate) fn file_line(rng: &mut Rng) -> String {
    let line = match rng.below(12) {
        0 => String::new(),
        1 | 2 => format!("x {} {}", rng.pick(DATES), words(rng, 5)),
        3 => format!(
            "x {} {} {}",
            rng.pick(DATES),
            rng.pick(DATES),
            words(rng, 4)
        ),
        4 | 5 => format!("({}) {}", letter(rng), words(rng, 5)),
        6 => format!("{} {}", rng.pick(DATES), words(rng, 5)),
        7 => {
            format!("({}) {} {}", letter(rng), rng.pick(DATES), words(rng, 4))
        }
        8 => format!("x {}", words(rng, 4)),
        9 => {
            format!("{} ({}) {}", rng.pick(DATES), letter(rng), words(rng, 3))
        }
        _ => words(rng, 6),
    };
    match rng.below(14) {
        0 => format!(" {line}"),
        1 => format!("{line} "),
        _ => line,
    }
}

/// `n` lines of a starting file, now and then without the last newline.
pub(crate) fn file_text(rng: &mut Rng, n: usize) -> Vec<u8> {
    let mut text = String::new();
    for _ in 0..n {
        text.push_str(&file_line(rng));
        text.push('\n');
    }
    if rng.one_in(6) && text.ends_with('\n') {
        text.pop();
    }
    text.into_bytes()
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
