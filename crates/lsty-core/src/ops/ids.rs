//! Generated ids: random, and never one that a line of either file or a
//! line staged in the same transaction carries.

use std::collections::HashSet;

use crate::errs::{Code, Error, Result};
use crate::list::Snapshot;
use crate::todotxt::{ID_ALPHABET, Id};

/// The length of the first id drawn.
const FIRST_LENGTH: usize = 4;
/// How many clashes one length takes before the length grows.
const CLASHES_PER_LENGTH: usize = 16;

/// Draws the characters of a generated id.
pub trait IdSource {
    /// An id of `length` characters of the id alphabet.
    ///
    /// # Errors
    ///
    /// `io` when the source of randomness fails.
    fn next(&self, length: usize) -> Result<String>;
}

/// The operating system's random numbers.
#[derive(Clone, Copy, Debug)]
pub struct OsIds;

impl IdSource for OsIds {
    fn next(&self, length: usize) -> Result<String> {
        let mut bytes = vec![0_u8; length];
        getrandom::fill(&mut bytes).map_err(|e| {
            Error::new(Code::Io, format!("read random bytes: {e}"))
        })?;
        // The alphabet has 32 characters, so the remainder is uniform.
        let alphabet = ID_ALPHABET.len();
        Ok(bytes
            .iter()
            .map(|&b| char::from(ID_ALPHABET[usize::from(b) % alphabet]))
            .collect())
    }
}

/// The ids that may not be drawn: those of the lines of the snapshot (found
/// in its id index), those of the lines the transaction changed or added
/// (looked up when the first id is drawn), the ids held up front and the
/// ids already drawn.
#[derive(Debug)]
pub(super) struct Taken {
    held: HashSet<Vec<u8>>,
    looked_up: bool,
}

impl Taken {
    /// Nothing is looked up until an id is drawn. `held` ids are never
    /// drawn.
    pub(super) fn new<'a>(held: impl IntoIterator<Item = &'a [u8]>) -> Self {
        Self {
            held: held.into_iter().map(<[u8]>::to_vec).collect(),
            looked_up: false,
        }
    }

    /// Draws an id nobody carries and marks it taken. After 16 clashes
    /// at one length the length grows by one. `edited` gives the ids of
    /// the lines the transaction changed or added; it is called at most
    /// once, on the first draw.
    ///
    /// # Errors
    ///
    /// `io` when the source fails or gives an id lsty may not write.
    pub(super) fn draw(
        &mut self,
        snap: &Snapshot,
        edited: impl FnOnce() -> Vec<Vec<u8>>,
        src: &dyn IdSource,
    ) -> Result<Id> {
        if !self.looked_up {
            self.held.extend(edited());
            self.looked_up = true;
        }
        for length in FIRST_LENGTH.. {
            if let Some(id) = self.draw_at(snap, src, length)? {
                return Id::new(&id).ok_or_else(unusable);
            }
        }
        Err(Error::new(Code::Io, "no id is free"))
    }

    fn draw_at(
        &mut self,
        snap: &Snapshot,
        src: &dyn IdSource,
        length: usize,
    ) -> Result<Option<String>> {
        for _ in 0..CLASHES_PER_LENGTH {
            let id = src.next(length)?;
            let bytes = id.as_bytes();
            if !snap.id_in_use(bytes) && self.held.insert(bytes.to_vec()) {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }
}

fn unusable() -> Error {
    Error::new(Code::Io, "the source of ids gave an id that can't be used")
}

/// An id that no line of `snap` and none of the `held` ids carries.
///
/// # Errors
///
/// `io` when the source fails or gives an id lsty may not write.
pub fn fresh_id(
    snap: &Snapshot,
    held: &[Id],
    src: &dyn IdSource,
) -> Result<Id> {
    let taken = held.iter().map(|id| id.as_str().as_bytes());
    Taken::new(taken).draw(snap, Vec::new, src)
}
