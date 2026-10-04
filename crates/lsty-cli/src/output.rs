//! The JSON envelope and the lines printed for humans.

use std::io::{self, Write};

use lsty_core::errs::{self, Error, Field as ErrField, Ref};

use crate::paint::Painter;

/// The version of the JSON envelope.
pub(crate) const ENVELOPE_VERSION: u8 = 1;

/// A JSON value, already encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Json(String);

impl Json {
    /// A string.
    pub(crate) fn string(value: &str) -> Self {
        Self(json_string(value))
    }

    /// A string made from bytes that may not be UTF-8: each invalid
    /// byte becomes U+FFFD.
    pub(crate) fn bytes(value: &[u8]) -> Self {
        Self(json_bytes(value))
    }

    /// An integer.
    pub(crate) fn int(value: i64) -> Self {
        Self(value.to_string())
    }

    /// `true` or `false`.
    pub(crate) fn bool(value: bool) -> Self {
        Self(value.to_string())
    }

    /// `null`.
    pub(crate) fn null() -> Self {
        Self("null".to_owned())
    }

    /// A string, or `null`.
    pub(crate) fn opt_string(value: Option<&str>) -> Self {
        value.map_or_else(Self::null, Self::string)
    }

    /// A list of strings.
    pub(crate) fn strs<S: AsRef<str>>(values: &[S]) -> Self {
        Self::array(values.iter().map(|v| Self::string(v.as_ref())))
    }

    /// A list of values.
    pub(crate) fn array(values: impl IntoIterator<Item = Self>) -> Self {
        let parts: Vec<String> = values.into_iter().map(|v| v.0).collect();
        Self(format!("[{}]", parts.join(",")))
    }

    /// An object whose keys keep the order of `fields`.
    pub(crate) fn object(fields: &[Field]) -> Self {
        Self(json_object(fields))
    }

    /// The value as JSON text.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// One key of a JSON envelope; keys keep the order they are given in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Field {
    /// The key.
    pub key: &'static str,
    value: Json,
}

impl Field {
    /// A field with `value`.
    pub(crate) fn new(key: &'static str, value: Json) -> Self {
        Self { key, value }
    }

    /// A string field.
    pub(crate) fn string(key: &'static str, value: &str) -> Self {
        Self::new(key, Json::string(value))
    }

    /// An integer field.
    pub(crate) fn int(key: &'static str, value: i64) -> Self {
        Self::new(key, Json::int(value))
    }

    /// A boolean field.
    pub(crate) fn bool(key: &'static str, value: bool) -> Self {
        Self::new(key, Json::bool(value))
    }

    /// A string field, or `null`.
    pub(crate) fn opt_string(key: &'static str, value: Option<&str>) -> Self {
        Self::new(key, Json::opt_string(value))
    }

    /// A field that is a list of strings.
    pub(crate) fn strs<S: AsRef<str>>(
        key: &'static str,
        values: &[S],
    ) -> Self {
        Self::new(key, Json::strs(values))
    }

    /// A field that is a list of values.
    pub(crate) fn array(
        key: &'static str,
        values: impl IntoIterator<Item = Json>,
    ) -> Self {
        Self::new(key, Json::array(values))
    }

    /// The value as JSON text.
    pub(crate) fn json(&self) -> &str {
        self.value.as_str()
    }
}

/// What a handler returns: the fields that follow `today` in the JSON
/// envelope, and the lines printed without `--json`. Lines are bytes, so
/// that text which is not UTF-8 prints as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Output {
    /// The fields after `today`.
    pub fields: Vec<Field>,
    /// The lines printed without `--json`, without their newlines.
    pub lines: Vec<Vec<u8>>,
}

/// `bytes` as a JSON string, with one U+FFFD for each invalid byte.
pub(crate) fn json_bytes(bytes: &[u8]) -> String {
    json_string(&errs::lossy(bytes))
}

/// `text` as a JSON string. `<`, `>` and `&` stay as they are, and
/// U+2028 and U+2029 are escaped.
pub(crate) fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\u{2028}' | '\u{2029}' | '\0'..='\u{1f}' => {
                push_unicode_escape(&mut out, c);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn push_unicode_escape(out: &mut String, c: char) {
    out.push_str("\\u");
    let n = u32::from(c);
    for shift in [12, 8, 4, 0] {
        out.push(char::from_digit((n >> shift) & 0xf, 16).unwrap_or('0'));
    }
}

/// The fields written as one JSON object.
pub(crate) fn json_object(fields: &[Field]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .map(|f| format!("{}:{}", json_string(f.key), f.json()))
        .collect();
    format!("{{{}}}", parts.join(","))
}

/// The fields of the `error` object of the envelope.
pub(crate) fn error_fields(err: &Error) -> Vec<Field> {
    let mut fields = vec![
        Field::string("code", err.code().as_str()),
        Field::int("exit", i64::from(err.exit())),
        Field::string("message", err.message()),
        Field::opt_string("hint", err.hint()),
    ];
    if let Some(item) = err.item() {
        fields.push(Field::int("nr", i64::from(item.nr.get())));
        fields.push(Field::string("source", item.source.as_str()));
        fields.push(Field::opt_string("id", item.id.as_deref()));
    }
    if let (None, Some((nr, source))) = (err.item(), err.place()) {
        fields.push(Field::int("nr", nr));
        fields.push(Field::string("source", source.as_str()));
        fields.push(Field::new("id", Json::null()));
    }
    for (key, value) in err.fields() {
        fields.push(Field::new(key, extra_json(value)));
    }
    fields
}

fn extra_json(value: &ErrField) -> Json {
    match value {
        ErrField::Null => Json::null(),
        ErrField::Int(n) => Json::int(*n),
        ErrField::Str(s) => Json::string(s),
        ErrField::Strs(list) => Json::strs(list),
        ErrField::Ref(r) => ref_json(r),
        ErrField::Refs(list) => Json::array(list.iter().map(ref_json)),
    }
}

/// A reference to an item as a JSON object.
pub(crate) fn ref_json(r: &Ref) -> Json {
    Json::object(&[
        Field::int("nr", i64::from(r.nr.get())),
        Field::string("source", r.source.as_str()),
        Field::opt_string("id", r.id.as_deref()),
    ])
}

/// The lines of an error for people: `lsty: message`, then the hint.
pub(crate) fn human_error(paint: &Painter, err: &Error) -> Vec<Vec<u8>> {
    paint.error_lines(err.message(), err.hint())
}

/// Writes `line` and a newline.
pub(crate) fn write_line(w: &mut dyn Write, line: &[u8]) -> errs::Result<()> {
    write_lines(w, &[line.to_vec()])
}

/// Writes the lines, each followed by a newline; nothing for no lines.
pub(crate) fn write_lines(
    w: &mut dyn Write,
    lines: &[Vec<u8>],
) -> errs::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let mut data = lines.join(&b'\n');
    data.push(b'\n');
    w.write_all(&data).map_err(|e| write_error(&e))
}

fn write_error(err: &io::Error) -> Error {
    Error::new(errs::Code::Io, format!("write output: {err}"))
}
