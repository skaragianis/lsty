//! A list on the screen keeps up with `lsty` processes that change its
//! files: bursts of adds, edits, completions, reopenings, archivings and
//! deletions, some back to back and some at once, and after each burst the
//! screen shows exactly the tasks the files hold, within `LIMIT`, far below
//! the minute after which the list would read the files again anyway.
//!
//! It runs `BURSTS` bursts; `LSTY_SOAK_SECONDS` runs it for that long
//! instead, and `LSTY_SOAK_SEED` is the seed of the random choices, printed
//! on a failure.
use std::{
    collections::BTreeMap,
    env, fs,
    process::{Child, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    Check, Duration, Failed, Instant, Path, ROWS, Screen, Work, start_with,
};

/// The tasks the list holds at least and at most, about a screen.
const SHORT: usize = 14;
const LONG: usize = 26;
/// The columns of the panels.
const PANEL: usize = 16;
/// The bursts a run has unless it is given its seconds.
const BURSTS: usize = 10;
/// The projects, `+p1` to `+p5`, and the contexts, `@c1` to `@c3`, the tasks
/// may have.
const PROJECTS: usize = 5;
const CONTEXTS: usize = 3;

/// A source of choices that a seed repeats.
struct Rng(u64);

impl Rng {
    /// A zero state would stay zero; the product tells near seeds apart.
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 >> 11).unwrap_or(0) % n
    }
}

/// What the person sees of a task line: the words, with no `x`, date or
/// tag with a colon.
fn shown(line: &str) -> String {
    let words: Vec<&str> = line
        .split_whitespace()
        .filter(|w| {
            *w != "x"
                && !w.contains(':')
                && !(w.len() == 10 && w.as_bytes()[4] == b'-')
        })
        .collect();
    words.join(" ")
}

/// What the files hold: the text of `todo.txt` and of `done.txt`.
fn files(root: &Path) -> Result<[String; 2], Failed> {
    let read = |name| fs::read_to_string(root.join(name)).unwrap_or_default();
    Ok([fs::read_to_string(root.join("todo.txt"))?, read("done.txt")])
}

/// The open tasks `todo` holds, in the order the list shows them: the done
/// ones and those that wait for a date are hidden.
fn expected(todo: &str) -> Vec<String> {
    let open = |l: &&str| {
        !l.trim().is_empty() && !l.starts_with("x ") && !l.contains(" t:2099")
    };
    todo.lines().filter(open).map(shown).collect()
}

/// The numbers a panel row shows for each project and context of the todo
/// file: its open tasks that the list would show, not those that wait for a
/// date, over all its tasks, those in the done file too. A name only the
/// done file has is not shown.
fn counts(files: &[String; 2]) -> BTreeMap<String, (usize, usize)> {
    let mut counts: BTreeMap<String, (usize, usize, bool)> = BTreeMap::new();
    for (n, text) in files.iter().enumerate() {
        for line in text.lines() {
            let open =
                n == 0 && !line.starts_with("x ") && !line.contains(" t:2099");
            let names = line.split_whitespace().filter(|w| w.len() > 1);
            for name in names.filter(|w| w.starts_with(['+', '@'])) {
                let count = counts.entry(name.to_owned()).or_default();
                count.0 += usize::from(open);
                count.1 += 1;
                count.2 |= n == 0;
            }
        }
    }
    let shown = counts.into_iter().filter(|(_, c)| c.2);
    shown.map(|(name, c)| (name, (c.0, c.1))).collect()
}

/// The counts of the rows of the panels, at the left of `rows`: `+p1 8/9`.
fn panel(rows: &[String]) -> BTreeMap<String, (usize, usize)> {
    let mut panel = BTreeMap::new();
    for row in rows {
        let row: String =
            row.chars().take(PANEL).filter(|c| *c != '│').collect();
        let mut words = row.split_whitespace();
        let (name, count) = (words.next(), words.next());
        let numbers = count.and_then(|c| c.split_once('/'));
        if let (Some(name), Some((open, all))) = (name, numbers)
            && let (Ok(open), Ok(all)) = (open.parse(), all.parse())
        {
            panel.insert(name.to_owned(), (open, all));
        }
    }
    panel
}

/// The tasks the list shows, in order, as `shown` words them: each row
/// without its line number, its id at the right and its marks.
fn listed(rows: &[String], sidebar: bool) -> Vec<String> {
    let mut tasks = Vec::new();
    for row in rows {
        let row: String = row
            .chars()
            .skip(if sidebar { PANEL } else { 0 })
            .filter(|c| *c != '│')
            .collect();
        let mut words = row.split_whitespace();
        if !words
            .next()
            .is_some_and(|w| w.bytes().all(|b| b.is_ascii_digit()))
        {
            continue;
        }
        let mut words: Vec<&str> =
            words.filter(|w| !matches!(*w, "✓" | "▷" | "⧗")).collect();
        words.pop();
        words.retain(|w| !w.contains(':'));
        tasks.push(words.join(" "));
    }
    tasks
}

struct Soak<'a> {
    work: &'a Work,
    rng: Rng,
    /// The next task number.
    next: usize,
    /// How many tasks the list may grow to, less than the screen holds or
    /// more.
    most: usize,
    /// Whether the sidebar is shown.
    sidebar: bool,
    /// How many projects no task had before were added: `+new1`, `+new2`.
    fresh: usize,
}

/// The ids of the tasks of the file, and whether each is done.
type Ids = Vec<(String, bool)>;

impl Soak<'_> {
    fn lsty(&self, args: &[String]) -> std::io::Result<Child> {
        let mut cmd = super::command(&self.work.root);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }

    /// The id of a task of `ids`, done or not as `done` says, taken out.
    fn take(&mut self, ids: &mut Ids, done: Option<bool>) -> Option<String> {
        let fits: Vec<usize> = (0..ids.len())
            .filter(|i| done.is_none_or(|d| ids[*i].1 == d))
            .collect();
        let at = *fits.get(self.rng.below(fits.len().max(1)))?;
        Some(ids.swap_remove(at).0)
    }

    /// The line of a new task: now and then long (tags are not drawn, so
    /// the row is not cut), waiting on an earlier task, naming a file, or
    /// in a project no task had: the first add of a run is one, and a task
    /// of a new project never waits for a date, so that its row must show.
    fn added(&mut self, count: usize, ids: &mut Ids, word: &str) -> String {
        self.next += 1;
        let n = self.next;
        let project = self.rng.below(PROJECTS) + 1;
        let context = self.rng.below(CONTEXTS) + 1;
        let fresh = self.fresh == 0
            || self.fresh < 2 && count < SHORT && self.rng.below(3) == 0;
        // One in four waits for a date far off: the list hides it.
        let later = if !fresh && self.rng.below(4) == 0 {
            " t:2099-01-01"
        } else {
            ""
        };
        let long = [0, 0, 0, 0, 0, 150, 470, 1800][self.rng.below(8)];
        let long = if long == 0 {
            String::new()
        } else {
            format!(" note:{}", "n".repeat(long))
        };
        let dep = match self.rng.below(4) {
            0 => self
                .take(ids, Some(false))
                .map(|id| format!(" dep:{}", id.trim_start_matches("id:"))),
            _ => None,
        };
        let file = [" file:notes/n.md", ""][self.rng.below(2)];
        let project = if fresh {
            self.fresh += 1;
            format!("new{}", self.fresh)
        } else {
            format!("p{project}")
        };
        format!(
            "T{n:02} {word} +{project} @c{context}{later}{long}{}{file} id:t{n:02}",
            dep.unwrap_or_default()
        )
    }

    /// The arguments of a change of one of the tasks `ids` of the file, or
    /// of the list. A change takes its task out of `ids`, so that no two
    /// changes of a burst meet, and `archive` is the only one of its burst:
    /// each must work.
    fn change(
        &mut self,
        count: usize,
        ids: &mut Ids,
        alone: bool,
    ) -> Vec<String> {
        let word = format!("w{}", self.rng.below(1000));
        let pick = if self.fresh == 0 || count < self.most - 4 {
            0
        } else if count >= self.most {
            self.rng.below(6) + 1
        } else {
            self.rng.below(7)
        };
        let (op, id) = match pick {
            1 => ("replace", self.take(ids, Some(false))),
            2 | 3 => ("do", self.take(ids, Some(false))),
            4 => ("reopen", self.take(ids, Some(true))),
            5 if alone => ("archive", Some(String::new())),
            6 => ("del", self.take(ids, None)),
            _ => ("add", None),
        };
        let args = |a: &[&str]| -> Vec<String> {
            a.iter()
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect()
        };
        let Some(id) = id else {
            let text = self.added(count, ids, &word);
            return args(&["add", "--human", &text]);
        };
        let n = id.trim_start_matches("id:t");
        match op {
            "replace" => {
                let text = format!("T{n} {word} {word} +p1 id:t{n}");
                args(&[op, "--human", &id, &text])
            }
            _ => args(&[op, "--human", &id]),
        }
    }

    /// One burst: a few changes, one after the other or all at once. The
    /// list's terminal is read meanwhile.
    fn burst(&mut self, screen: &mut Screen) -> Check {
        let text = fs::read_to_string(self.work.root.join("todo.txt"))?;
        let count = expected(&text).len();
        let mut ids: Ids = text
            .lines()
            .filter(|line| !line.contains(" dep:"))
            .filter_map(|line| {
                let id =
                    line.split_whitespace().find(|w| w.starts_with("id:"))?;
                Some((id.to_owned(), line.starts_with("x ")))
            })
            .collect();
        let (size, together) = (1 + self.rng.below(6), self.rng.below(2) == 0);
        // One after the other, the screen read between them or not.
        let reading = self.rng.below(2) == 0;
        let mut running: Vec<(Child, String)> = Vec::new();
        for _ in 0..size {
            let args = self.change(count, &mut ids, size == 1);
            running.push((self.lsty(&args)?, args.join(" ")));
            if !together && reading {
                Self::finish(screen, &mut running)?;
            } else if !together {
                running.pop().map(Self::reap).transpose()?;
            }
        }
        Self::finish(screen, &mut running)
    }

    /// Takes the end of a change that has ended: it must be a success.
    fn reap((mut child, args): (Child, String)) -> Check {
        let status = child.wait()?;
        if !status.success() {
            return Err(format!("lsty {args} ended with {status}").into());
        }
        Ok(())
    }

    /// Waits for the changes to end, all with success.
    fn finish(
        screen: &mut Screen,
        running: &mut Vec<(Child, String)>,
    ) -> Check {
        let end = Instant::now() + super::LIMIT;
        while !running.is_empty() {
            let done = running.iter_mut().position(|(child, _)| {
                matches!(child.try_wait(), Ok(Some(_)))
            });
            if let Some(at) = done {
                Self::reap(running.swap_remove(at))?;
            }
            if Instant::now() > end {
                return Err("a command did not end".into());
            }
            screen.pull();
        }
        Ok(())
    }

    /// Whether the rows are the tasks `want`, all of them or, when the
    /// list is longer than the screen (a long task takes several rows), the
    /// footer's count and a run of them; and the sidebar shows the counts
    /// of the files.
    fn shows(
        &self,
        rows: &[String],
        want: &[String],
        files: &[String; 2],
    ) -> bool {
        let tasks = listed(rows, self.sidebar);
        let total = rows[ROWS - 1]
            .split_whitespace()
            .find_map(|w| w.split_once('|')?.1.parse::<usize>().ok());
        let rows_ok = tasks == want
            || total == Some(want.len())
                && tasks.len() >= 5
                && want.windows(tasks.len()).any(|run| run == tasks);
        rows_ok && (!self.sidebar || panel(rows) == counts(files))
    }

    /// Waits until the screen shows what the files hold.
    fn shows_files(&self, screen: &mut Screen) -> Check {
        let end = Instant::now() + super::LIMIT;
        let mut closed = false;
        loop {
            let files = files(&self.work.root)?;
            let want = expected(&files[0]);
            let rows = screen.grid.rows();
            if self.shows(&rows, &want, &files) {
                return Ok(());
            }
            if closed || Instant::now() > end {
                let stop =
                    ["too late", "the terminal closed"][usize::from(closed)];
                return Err(format!(
                    "{stop}: the files hold {want:?}\n{}the screen shows\n{}",
                    files.concat(),
                    rows.join("\n")
                )
                .into());
            }
            closed = !screen.pull();
        }
    }

    /// Keys that write nothing; the next burst checks their effect.
    fn press(&mut self, screen: &mut Screen) -> Check {
        let keys: [&[u8]; 6] = [b"jj", b"k", b"G", b"gg", b"/T1\r\x1b", b"gs"];
        let keys = keys[self.rng.below(keys.len())];
        self.sidebar ^= keys == b"gs";
        screen.type_keys(keys)?;
        screen.pull();
        Ok(())
    }
}

/// The seconds the soak runs, if it is given them.
fn seconds() -> Option<u64> {
    env::var("LSTY_SOAK_SECONDS").ok()?.parse().ok()
}

pub(crate) fn keeps_up() -> Check {
    let seed = env::var("LSTY_SOAK_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            let now = SystemTime::now().duration_since(UNIX_EPOCH);
            now.map_or(1, |d| u64::try_from(d.as_nanos()).unwrap_or(1))
        });
    run(seed).map_err(|e| {
        format!("seed {seed}: {}", e.message().unwrap_or("")).into()
    })
}

fn run(seed: u64) -> Check {
    let work =
        Work::new()?.config("show_detail = false\nlock_timeout = \"5s\"\n")?;
    let list: Vec<String> = (1..=12)
        .map(|n| {
            let (p, c) = (n % 2 + 1, n % CONTEXTS + 1);
            format!("T{n:02} start +p{p} @c{c} id:t{n:02}")
        })
        .collect();
    fs::write(work.root.join("todo.txt"), list.join("\n") + "\n")?;
    let (mut child, mut screen) = start_with(&work.root, &[], &[])?;
    screen.wait_for("T01")?;
    let mut rng = Rng::new(seed);
    let most = [SHORT, LONG][rng.below(2)];
    let mut soak = Soak {
        work: &work,
        rng,
        next: 12,
        most,
        sidebar: true,
        fresh: 0,
    };
    let end = seconds().map(|s| Instant::now() + Duration::from_secs(s));
    for burst in 1.. {
        if end.map_or(burst > BURSTS, |end| Instant::now() > end) {
            break;
        }
        let at = |e: Failed| -> Failed {
            format!("burst {burst}: {}", e.message().unwrap_or("")).into()
        };
        soak.burst(&mut screen).map_err(at)?;
        soak.shows_files(&mut screen).map_err(at)?;
        if soak.rng.below(2) == 0 {
            soak.press(&mut screen).map_err(at)?;
        }
    }
    screen.type_keys(b"q")?;
    screen.ends_with(&mut child, 0)
}
