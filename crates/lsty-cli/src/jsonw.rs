//! Writing a JSON value straight into one string, for values that come
//! in thousands: nothing is encoded on its own and joined afterwards.

use std::fmt::Write as _;

use crate::output::{push_json_bytes, push_json_string};

/// The commas of an object or array being written; its fields or items
/// keep the order they are written in.
struct Seq<'a> {
    out: &'a mut String,
    first: bool,
}

impl Seq<'_> {
    /// Starts an item and gives the string it is written to.
    fn next(&mut self) -> &mut String {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        self.out
    }
}

/// An object being written.
pub(crate) struct Obj<'a>(Seq<'a>);

/// An array being written.
pub(crate) struct Arr<'a>(Seq<'a>);

/// Writes an object to `out` with the fields that `fill` writes.
pub(crate) fn object(out: &mut String, fill: impl FnOnce(&mut Obj<'_>)) {
    out.push('{');
    let mut obj = Obj(Seq { out, first: true });
    fill(&mut obj);
    obj.0.out.push('}');
}

/// Writes an array to `out` with the items that `fill` writes.
pub(crate) fn array(out: &mut String, fill: impl FnOnce(&mut Arr<'_>)) {
    out.push('[');
    let mut arr = Arr(Seq { out, first: true });
    fill(&mut arr);
    arr.0.out.push(']');
}

impl Obj<'_> {
    /// Starts a field and gives the string its value is written to.
    fn key(&mut self, key: &str) -> &mut String {
        push_json_string(self.0.next(), key);
        self.0.out.push(':');
        self.0.out
    }

    /// An integer field.
    pub(crate) fn int(&mut self, key: &str, value: i64) {
        // Writing to a string cannot fail.
        write!(self.key(key), "{value}").ok();
    }

    /// A boolean field.
    pub(crate) fn bool(&mut self, key: &str, value: bool) {
        self.key(key).push_str(if value { "true" } else { "false" });
    }

    /// A string field.
    pub(crate) fn string(&mut self, key: &str, value: &str) {
        push_json_string(self.key(key), value);
    }

    /// A string field, or `null`.
    pub(crate) fn opt_string(&mut self, key: &str, value: Option<&str>) {
        match value {
            Some(value) => self.string(key, value),
            None => self.null(key),
        }
    }

    /// A field made from bytes, one U+FFFD for each invalid byte.
    pub(crate) fn bytes(&mut self, key: &str, value: &[u8]) {
        push_json_bytes(self.key(key), value);
    }

    /// A field made from bytes, or `null`.
    pub(crate) fn opt_bytes(&mut self, key: &str, value: Option<&[u8]>) {
        match value {
            Some(value) => self.bytes(key, value),
            None => self.null(key),
        }
    }

    /// A field that is already JSON text.
    pub(crate) fn raw(&mut self, key: &str, json: &str) {
        self.key(key).push_str(json);
    }

    /// A `null` field.
    pub(crate) fn null(&mut self, key: &str) {
        self.key(key).push_str("null");
    }

    /// A field that is an array of what `fill` writes.
    pub(crate) fn array(
        &mut self,
        key: &str,
        fill: impl FnOnce(&mut Arr<'_>),
    ) {
        array(self.key(key), fill);
    }
}

impl Arr<'_> {
    /// An item made from bytes, one U+FFFD for each invalid byte.
    pub(crate) fn bytes(&mut self, value: &[u8]) {
        push_json_bytes(self.0.next(), value);
    }

    /// An item that is an array of what `fill` writes.
    pub(crate) fn array(&mut self, fill: impl FnOnce(&mut Arr<'_>)) {
        array(self.0.next(), fill);
    }

    /// An item that is already JSON text.
    pub(crate) fn raw(&mut self, json: &str) {
        self.0.next().push_str(json);
    }

    /// An item that is an object of what `fill` writes.
    pub(crate) fn object(&mut self, fill: impl FnOnce(&mut Obj<'_>)) {
        object(self.0.next(), fill);
    }
}
