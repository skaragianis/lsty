//! The todo.txt line grammar and a file model that keeps every byte.
//!
//! [`Line::parse`] splits one line into its done mark, priority, dates and
//! the tokens of its description. [`File::parse`] splits a file into lines
//! and [`File::to_bytes`] writes it back, byte for byte, whatever the input.
//! Bytes that are not UTF-8 are kept as they are, so text is `&[u8]`.

mod build;
mod file;
mod filelink;
mod id;
mod line;
mod name;
mod owner;
mod parse;
mod tags;
mod tokens;
mod trim;

pub use build::{Parts, build};
pub use file::File;
pub use filelink::FileLink;
pub use id::{ID_ALPHABET, Id, is_generated_id};
pub use line::{EditError, Eol, Kind, Line, Priority, TokenView};
pub use name::{name_has_prefix, name_parent, name_segments};
pub use owner::Owner;
pub(crate) use parse::{BOM, is_date};
pub(crate) use tags::parse_stamp;
pub use tags::{Rec, RecUnit, Tags, is_single_valued};
pub use trim::{trim, trim_range};

#[cfg(test)]
mod build_tests;
#[cfg(test)]
mod file_tests;
#[cfg(test)]
mod filelink_tests;
#[cfg(test)]
mod id_tests;
#[cfg(test)]
mod interop_tests;
#[cfg(test)]
mod invariant_tests;
#[cfg(test)]
mod line_tests;
#[cfg(test)]
mod name_tests;
#[cfg(test)]
mod owner_tests;
#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod tags_tests;
#[cfg(test)]
mod tokens_tests;
#[cfg(test)]
mod trim_tests;
