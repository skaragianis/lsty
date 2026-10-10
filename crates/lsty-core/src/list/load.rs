use std::time::Duration;

use super::snapshot::{Params, Snapshot};
use crate::{
    discover::Found,
    errs, fsio,
    lock::{self, Cancel, Mode, Want},
    todotxt::File,
};

/// Reads both files under shared locks and builds the snapshot. A missing
/// file reads as an empty one; a lock that is not free within `timeout` is
/// `lock_timeout`.
pub fn load(
    found: &Found,
    timeout: Duration,
    cancel: &Cancel,
    params: &Params,
) -> errs::Result<Snapshot> {
    let wants = [&found.todo, &found.done].map(|file| Want {
        file: file.clone(),
        mode: Mode::Shared,
    });
    let mut held = lock::acquire(&wants, timeout, cancel)?;
    let todo = read_file(&found.todo);
    let done = read_file(&found.done);
    held.release()?;
    Ok(Snapshot::build(found.clone(), todo?, done?, params))
}

fn read_file(path: &std::path::Path) -> errs::Result<File> {
    let data = fsio::read(path)?;
    Ok(data.map_or_else(File::default, |data| File::parse(&data)))
}
