use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::errs;
use crate::errs::io_at;

/// The SHA-256 of `data`.
pub fn hash(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// The whole file, or `None` when it does not exist.
pub fn read(path: &Path) -> errs::Result<Option<Vec<u8>>> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_at("read", path, "open", path, e)),
    };
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| io_at("read", path, "read", path, e))?;
    Ok(Some(data))
}
