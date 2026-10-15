//! `report`: archive, then log how many tasks the two files hold.

use super::{env::Env, result::Outcome, run::run, tx::Tx};
use crate::{
    discover::report_file,
    errs::{Result, Source},
    fsio,
    todotxt::Line,
};

/// Archives, then appends `YYYY-MM-DDTHH:MM:SS <todo> <done>` to the report
/// file when the counts differ from the last line's, and says the last
/// line. The counts are those of the tasks: a blank line is not one. The
/// report file is written under the lock of the todo file's folder, after
/// the lists; it is a log, and no undo reverses it.
///
/// # Errors
///
/// Whatever archiving, or reading or writing the report file, fails with.
pub fn report(env: &Env<'_>) -> Result<Outcome> {
    run(env, "report", true, |tx| {
        super::archive::plan(tx)?;
        let counts =
            format!("{} {}", tasks(tx, Source::Todo), tasks(tx, Source::Done));
        let path = report_file(&env.found.todo);
        let mut log = fsio::read(&path)?.unwrap_or_default();
        let last = last_line(&log);
        if last.split_once(' ').is_some_and(|(_, seen)| seen == counts) {
            let line = last.to_owned();
            tx.say(line);
            return Ok(());
        }
        let line = format!("{} {counts}", env.clock.local_stamp());
        if !log.is_empty() && !log.ends_with(b"\n") {
            log.push(b'\n');
        }
        log.extend_from_slice(line.as_bytes());
        log.push(b'\n');
        tx.say(line);
        tx.log = Some((path, log));
        Ok(())
    })
}

/// The last line of `log`, without its newline.
fn last_line(log: &[u8]) -> &str {
    let text = log.strip_suffix(b"\n").unwrap_or(log);
    let start = text.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    std::str::from_utf8(&text[start..]).unwrap_or_default()
}

/// How many lines of `source` the plan leaves that are not blank.
fn tasks(tx: &Tx, source: Source) -> usize {
    let file = tx.file(source);
    file.order()
        .iter()
        .filter(|&&i| !Line::is_blank_text(&file.rows()[i].raw))
        .count()
}
