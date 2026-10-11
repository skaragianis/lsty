//! The JSON envelope and the lines printed for humans.

use std::{
    borrow::Borrow,
    fmt::Write as _,
    io::{self, Write},
};

use lsty_core::errs::{self, Error, Field as ErrField, Ref};

use crate::jsonw;

/// The version of the JSON envelope.
pub(crate) const ENVELOPE_VERSION: u8 = 1;

/// A JSON value, already encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Json(String);

impl Json {
    /// A string.
    pub(crate) fn string(value: &str) -> Self {
        let mut out = String::with_capacity(value.len() + 2);
        push_json_string(&mut out, value);
        Self(out)
    }

    /// A string made from bytes that may not be UTF-8: each invalid
    /// byte becomes U+FFFD.
    pub(crate) fn bytes(value: &[u8]) -> Self {
        let mut out = String::with_capacity(value.len() + 2);
        push_json_bytes(&mut out, value);
        Self(out)
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
        let mut out = String::new();
        jsonw::array(&mut out, |arr| {
            for value in values {
                arr.raw(&value.0);
            }
        });
        Self(out)
    }

    /// An object whose keys keep the order of `fields`.
    pub(crate) fn object(fields: &[Field]) -> Self {
        Self(json_object(fields))
    }

    /// A value that is already JSON text.
    pub(crate) const fn raw(text: String) -> Self {
        Self(text)
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

    /// A count field.
    pub(crate) fn count(key: &'static str, value: usize) -> Self {
        Self::int(key, i64::try_from(value).unwrap_or(i64::MAX))
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
    /// Lines for standard error, printed after `lines`.
    pub notes: Vec<Vec<u8>>,
}

impl Output {
    /// Builds only what is printed: the fields under `--json`, else the
    /// lines.
    pub(crate) fn either(
        json: bool,
        fields: impl FnOnce() -> Vec<Field>,
        lines: impl FnOnce() -> Vec<Vec<u8>>,
    ) -> Self {
        if json {
            Self {
                fields: fields(),
                ..Self::default()
            }
        } else {
            Self {
                lines: lines(),
                ..Self::default()
            }
        }
    }

    /// An output of only these lines.
    pub(crate) fn lines<S: AsRef<[u8]>>(
        lines: impl IntoIterator<Item = S>,
    ) -> Self {
        Self {
            lines: lines.into_iter().map(|l| l.as_ref().to_vec()).collect(),
            ..Self::default()
        }
    }
}

/// Writes `bytes` to `out` as a JSON string, one U+FFFD for each invalid
/// byte.
pub(crate) fn push_json_bytes(out: &mut String, bytes: &[u8]) {
    out.push('"');
    for chunk in bytes.utf8_chunks() {
        push_escaped(out, chunk.valid());
        out.extend(chunk.invalid().iter().map(|_| '\u{fffd}'));
    }
    out.push('"');
}

/// Writes `text` to `out` as a JSON string.
pub(crate) fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    push_escaped(out, text);
    out.push('"');
}

fn push_escaped(out: &mut String, text: &str) {
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
                write!(out, "\\u{:04x}", u32::from(c)).ok();
            }
            c => out.push(c),
        }
    }
}

/// The fields written as one JSON object.
pub(crate) fn json_object(fields: &[Field]) -> String {
    let mut out = String::new();
    jsonw::object(&mut out, |obj| {
        for field in fields {
            obj.raw(field.key, field.json());
        }
    });
    out
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
        fields.extend(ref_fields(item));
    }
    if let Some((nr, source)) = err.place() {
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
    Json::object(&ref_fields(r))
}

fn ref_fields(r: &Ref) -> [Field; 3] {
    [
        Field::int("nr", i64::from(r.nr.get())),
        Field::string("source", r.source.as_str()),
        Field::opt_string("id", r.id.as_deref()),
    ]
}

/// Writes the lines, each followed by a newline; nothing for no lines.
/// `device` names the stream in the message of a failed write.
pub(crate) fn write_lines(
    w: &mut dyn Write,
    device: &str,
    lines: &[impl Borrow<[u8]>],
) -> errs::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let mut data = lines.join(&b'\n');
    data.push(b'\n');
    w.write_all(&data).map_err(|e| write_error(device, &e))
}

/// `write output: write /dev/stdout: broken pipe`.
fn write_error(device: &str, err: &io::Error) -> Error {
    let reason = errs::io_text(err);
    Error::new(
        errs::Code::Io,
        format!("write output: write {device}: {reason}"),
    )
}
