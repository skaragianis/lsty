use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

use rustix::fs::{Mode, fstat};

use crate::{errs, errs::io_at};

/// The whole file named `path`, which is opened from `cwd` when it is
/// relative, or `None` when it does not exist. A failure is worded with the
/// path as given, `open x.toml: permission denied`. For files read for
/// settings.
pub(crate) fn read_in(
    cwd: &Path,
    path: &Path,
) -> errs::Result<Option<Vec<u8>>> {
    let mut file = match File::open(cwd.join(path)) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(errs::io("open", path, e)),
    };
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| errs::io("read", path, e))?;
    Ok(Some(data))
}

/// The largest file [`read`] takes and [`write`](super::write()) writes:
/// parsed lines keep their offsets in 32 bits.
pub(super) const MAX_LEN: u64 = u32::MAX as u64;

/// The whole file and its permission bits as opened, which a write of it
/// keeps; no bytes and no mode when it does not exist. A file over 4 GiB is
/// an error.
pub fn read(path: &Path) -> errs::Result<Fetched> {
    read_within(path, MAX_LEN)
}

/// A file's bytes and the mode it was read with; none when it was absent.
pub type Fetched = (Vec<u8>, Option<Mode>);

/// `read`, refusing a file over `max` bytes.
pub(super) fn read_within(path: &Path, max: u64) -> errs::Result<Fetched> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Fetched::default());
        }
        Err(e) => return Err(io_at("read", path, "open", path, e)),
    };
    let fail = |e| io_at("read", path, "read", path, e);
    let too_large = || fail(io::ErrorKind::FileTooLarge.into());
    // The size is a quick refusal; the bound on the bytes read covers a
    // file that grows and one with no size.
    if file.metadata().map_err(fail)?.len() > max {
        return Err(too_large());
    }
    let mode = fstat(&file).map_err(|e| fail(e.into()))?.st_mode & 0o777;
    let mut data = Vec::new();
    file.take(max + 1).read_to_end(&mut data).map_err(fail)?;
    if data.len() as u64 > max {
        return Err(too_large());
    }
    Ok((data, Some(Mode::from_bits_truncate(mode))))
}
