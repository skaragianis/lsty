use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use crate::errs;
use crate::errs::io_at;

/// The whole file named `path`, which is opened from `cwd` when it is
/// relative. A failure is worded with the path as given, `open x.toml:
/// permission denied`; [`is_missing`](super::is_missing) tells a file that
/// is not there from the others. For files read for settings.
pub(crate) fn read_in(cwd: &Path, path: &Path) -> errs::Result<Vec<u8>> {
    let mut file =
        File::open(cwd.join(path)).map_err(|e| errs::io("open", path, e))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| errs::io("read", path, e))?;
    Ok(data)
}

/// The largest file [`read`] takes: parsed lines keep their offsets in
/// 32 bits.
const MAX_LEN: u64 = u32::MAX as u64;

/// The whole file, or `None` when it does not exist. A file over 4 GiB is
/// an error.
pub fn read(path: &Path) -> errs::Result<Option<Vec<u8>>> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_at("read", path, "open", path, e)),
    };
    let fail = |e| io_at("read", path, "read", path, e);
    let len = file.metadata().map_err(fail)?.len();
    if len > MAX_LEN {
        return Err(fail(io::ErrorKind::FileTooLarge.into()));
    }
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(fail)?;
    Ok(Some(data))
}
