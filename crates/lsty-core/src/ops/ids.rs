//! Generated ids: random, and never one that a line of either file or a
//! line staged in the same transaction carries.

use std::collections::HashSet;

use crate::errs::{Code, Error, Result};
use crate::list::Snapshot;
use crate::todotxt::ID_ALPHABET;

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
#[derive(Clone, Copy, Debug, Default)]
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
            .filter_map(|&b| ID_ALPHABET.get(usize::from(b) % alphabet))
            .map(|&c| char::from(c))
            .collect())
    }
}

/// The ids that may not be drawn.
#[derive(Debug, Default)]
pub struct Taken(HashSet<Vec<u8>>);

impl Taken {
    /// Every id a line of `snap` carries, including a later `id:` token
    /// of a line, and the `staged` ids.
    pub fn new<'a>(
        snap: &Snapshot,
        staged: impl IntoIterator<Item = &'a [u8]>,
    ) -> Self {
        let mut taken = HashSet::new();
        for item in snap.items() {
            let tags = snap.tags(item);
            taken.extend(tags.id.map(<[u8]>::to_vec));
            taken.extend(
                tags.extra
                    .iter()
                    .filter(|t| t.key == b"id")
                    .map(|t| t.value.to_vec()),
            );
        }
        taken.extend(staged.into_iter().map(<[u8]>::to_vec));
        Self(taken)
    }

    /// Draws an id nobody carries and marks it taken. After 16 clashes
    /// at one length the length grows by one.
    ///
    /// # Errors
    ///
    /// Whatever the source fails with.
    pub fn draw(&mut self, src: &dyn IdSource) -> Result<String> {
        for length in FIRST_LENGTH.. {
            if let Some(id) = self.draw_at(src, length)? {
                return Ok(id);
            }
        }
        Err(Error::new(Code::Io, "no id is free"))
    }

    fn draw_at(
        &mut self,
        src: &dyn IdSource,
        length: usize,
    ) -> Result<Option<String>> {
        for _ in 0..CLASHES_PER_LENGTH {
            let id = src.next(length)?;
            if self.0.insert(id.as_bytes().to_vec()) {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }
}

/// Draws an id that no line of `snap` and no `staged` id carries.
///
/// # Errors
///
/// Whatever the source fails with.
pub fn new_id<'a>(
    snap: &Snapshot,
    staged: impl IntoIterator<Item = &'a [u8]>,
    src: &dyn IdSource,
) -> Result<String> {
    Taken::new(snap, staged).draw(src)
}
