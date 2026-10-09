use lsty_core::errs::{Code, Error, Field as ErrField, Nr, Ref, Source};

use super::env::Stream;
use super::output::{
    Field, Json, error_fields, human_error, json_object, write_line,
    write_lines,
};
use super::paint::Painter;

const STRINGS: [(&str, &str); 14] = [
    ("plain", r#""plain""#),
    ("a\"b", r#""a\"b""#),
    ("a\\b", r#""a\\b""#),
    ("a\nb\rc\td", r#""a\nb\rc\td""#),
    ("\u{8}\u{c}", r#""\b\f""#),
    ("\u{0}", r#""\u0000""#),
    ("\u{1}\u{1f}", r#""\u0001\u001f""#),
    ("\u{b}\u{e}", r#""\u000b\u000e""#),
    ("\u{2028}\u{2029}", r#""\u2028\u2029""#),
    ("<>&", r#""<>&""#),
    ("\u{7f}", "\"\u{7f}\""),
    ("café ☃ 😀", r#""café ☃ 😀""#),
    ("\u{fffd}", "\"\u{fffd}\""),
    ("", r#""""#),
];

#[test]
fn json_strings() {
    for (text, want) in STRINGS {
        assert_eq!(Json::string(text).as_str(), want, "{text:?}");
    }
}

const BYTES: [(&[u8], &str); 6] = [
    (b"abc", "\"abc\""),
    (b"caf\xe9", "\"caf\u{fffd}\""),
    (b"a\xe2\x80b", "\"a\u{fffd}\u{fffd}b\""),
    (b"\xff\xfe", "\"\u{fffd}\u{fffd}\""),
    ("é".as_bytes(), "\"é\""),
    (b"\xf0\x9f\x98", "\"\u{fffd}\u{fffd}\u{fffd}\""),
];

#[test]
fn invalid_bytes_become_one_replacement_each() {
    for (bytes, want) in BYTES {
        assert_eq!(Json::bytes(bytes).as_str(), want, "{bytes:?}");
    }
}

/// An error with an item: the item's fields follow `hint`, the extra
/// fields follow in key order, and a repeated `nr` is kept.
#[test]
fn error_fields_with_an_item() {
    let item = Ref {
        nr: Nr::new(5).unwrap(),
        source: Source::Todo,
        id: Some("k3f9".to_owned()),
    };
    let err = Error::new(Code::NoItem, "no item 5")
        .with_item(item)
        .with_field("nr", ErrField::Int(5));
    assert_eq!(
        json_object(&error_fields(&err)),
        concat!(
            r#"{"code":"no_item","exit":3,"message":"no item 5","hint":null,"#,
            r#""nr":5,"source":"todo","id":"k3f9","nr":5}"#
        )
    );
}

#[test]
fn an_item_without_an_id_has_a_null_id() {
    let item = Ref {
        nr: Nr::new(2).unwrap(),
        source: Source::Done,
        id: None,
    };
    let err = Error::new(Code::NoItem, "x").with_item(item);
    assert!(
        json_object(&error_fields(&err))
            .ends_with(r#""nr":2,"source":"done","id":null}"#)
    );
}

#[test]
fn extra_fields_of_every_kind() {
    let r = |n| Ref {
        nr: Nr::new(n).unwrap(),
        source: Source::Todo,
        id: None,
    };
    let err = Error::new(Code::Blocked, "m")
        .with_hint("h")
        .with_field("a_null", ErrField::Null)
        .with_field("b_int", ErrField::Int(-3))
        .with_field("c_str", ErrField::Str("s\"".to_owned()))
        .with_field("d_strs", ErrField::Strs(vec!["x".into(), "y".into()]))
        .with_field("e_ref", ErrField::Ref(r(1)))
        .with_field("f_refs", ErrField::Refs(vec![r(2), r(3)]))
        .with_field("g_none", ErrField::Refs(vec![]));
    assert_eq!(
        json_object(&error_fields(&err)),
        concat!(
            r#"{"code":"blocked","exit":3,"message":"m","hint":"h","#,
            r#""a_null":null,"b_int":-3,"c_str":"s\"","#,
            r#""d_strs":["x","y"],"#,
            r#""e_ref":{"nr":1,"source":"todo","id":null},"#,
            r#""f_refs":[{"nr":2,"source":"todo","id":null},"#,
            r#"{"nr":3,"source":"todo","id":null}],"g_none":[]}"#
        )
    );
}

#[test]
fn field_constructors() {
    let fields = [
        Field::bool("t", true),
        Field::bool("f", false),
        Field::new("n", Json::null()),
        Field::int("i", 7),
        Field::string("s", "x"),
        Field::opt_string("o", None),
        Field::strs("l", &["a", "b"]),
        Field::array("r", [Json::int(1), Json::null()]),
        Field::new("obj", Json::object(&[Field::bool("k", true)])),
    ];
    assert_eq!(
        json_object(&fields),
        concat!(
            r#"{"t":true,"f":false,"n":null,"i":7,"s":"x","o":null,"#,
            r#""l":["a","b"],"r":[1,null],"obj":{"k":true}}"#
        )
    );
    assert_eq!(json_object(&[]), "{}");
    assert_eq!(Json::array([]).as_str(), "[]");
}

#[test]
fn human_error_lines() {
    let err = Error::new(Code::Usage, "bad").with_hint("try this");
    assert_eq!(
        human_error(&Painter::off(), &err),
        [b"lsty: bad".to_vec(), b"hint: try this".to_vec()]
    );
    let err = Error::new(Code::Io, "bad");
    assert_eq!(human_error(&Painter::off(), &err), [b"lsty: bad".to_vec()]);
}

#[test]
fn lines_keep_their_bytes() {
    let mut out = Vec::new();
    write_lines(
        &mut out,
        Stream::Stdout,
        &[b"caf\xe9".to_vec(), b"x".to_vec()],
    )
    .unwrap();
    assert_eq!(out, b"caf\xe9\nx\n");
    let mut out = Vec::new();
    write_lines(&mut out, Stream::Stdout, &[]).unwrap();
    assert_eq!(out, b"");
    write_lines(&mut out, Stream::Stdout, &[Vec::new()]).unwrap();
    assert_eq!(out, b"\n");
}

/// A stream that refuses every write.
struct Broken(std::io::ErrorKind);

impl std::io::Write for Broken {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(self.0.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_failed_write_names_the_stream_in_the_system_s_words() {
    let mut out = Broken(std::io::ErrorKind::BrokenPipe);
    let err =
        write_lines(&mut out, Stream::Stdout, &[b"x".to_vec()]).unwrap_err();
    assert_eq!(err.code(), Code::Io);
    assert_eq!(
        err.message(),
        "write output: write /dev/stdout: broken pipe"
    );
    let err = write_line(&mut out, Stream::Stderr, b"x").unwrap_err();
    assert_eq!(
        err.message(),
        "write output: write /dev/stderr: broken pipe"
    );
}
