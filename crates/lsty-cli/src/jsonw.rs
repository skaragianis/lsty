//! Writing a JSON value straight into one string, for values that come
//! in thousands: nothing is encoded on its own and joined afterwards.

use std::fmt::Write as _;

use crate::output::{push_json_bytes, push_json_string};

/// An object or array being written; its fields or items keep the order
/// they are written in.
pub(crate) struct Seq<'a> {
    out: &'a mut String,
    first: bool,
}

/// Writes an object to `out` with the fields that `fill` writes.
pub(crate) fn object(out: &mut String, fill: impl FnOnce(&mut Seq<'_>)) {
    out.push('{');
    let mut obj = Seq { out, first: true };
    fill(&mut obj);
    obj.out.push('}');
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

    /// Starts a field and gives the string its value is written to.
    fn key(&mut self, key: &str) -> &mut String {
        push_json_string(self.next(), key);
        self.out.push(':');
        self.out
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

    /// A field made from bytes, or `null` when there are none or the
    /// value is empty.
    pub(crate) fn non_empty(&mut self, key: &str, value: Option<&[u8]>) {
        match value.filter(|v| !v.is_empty()) {
            Some(value) => self.bytes(key, value),
            None => self.null(key),
        }
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
        fill: impl FnOnce(&mut Seq<'_>),
    ) {
        array(self.key(key), fill);
    }
}

/// Writes an array to `out` with the items that `fill` writes.
pub(crate) fn array(out: &mut String, fill: impl FnOnce(&mut Seq<'_>)) {
    out.push('[');
    let mut arr = Seq { out, first: true };
    fill(&mut arr);
    arr.out.push(']');
}

impl Seq<'_> {
    /// An item made from bytes, one U+FFFD for each invalid byte.
    pub(crate) fn item_bytes(&mut self, value: &[u8]) {
        push_json_bytes(self.next(), value);
    }

    /// An item that is an array of what `fill` writes.
    pub(crate) fn item_array(&mut self, fill: impl FnOnce(&mut Seq<'_>)) {
        array(self.next(), fill);
    }

    /// An item that is already JSON text.
    pub(crate) fn item_raw(&mut self, json: &str) {
        self.next().push_str(json);
    }

    /// An item that is an object of what `fill` writes.
    pub(crate) fn item_object(&mut self, fill: impl FnOnce(&mut Seq<'_>)) {
        object(self.next(), fill);
    }
}
