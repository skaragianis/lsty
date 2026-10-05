use super::jsonw;
use super::output::{Field, Json, json_object};

/// What the fields-and-values encoder writes for the same value.
fn encoded() -> String {
    let fields = [
        Field::int("n", -12),
        Field::bool("yes", true),
        Field::string("s", "a\"b\u{2028}"),
        Field::opt_string("none", None),
        Field::array(
            "list",
            [Json::bytes(b"x\xffy"), Json::array([Json::int(1)])],
        ),
        Field::new("o", Json::object(&[Field::bool("no", false)])),
    ];
    json_object(&fields)
}

#[test]
fn written_in_place_it_reads_as_the_encoded_value() {
    let mut out = String::new();
    jsonw::object(&mut out, |obj| {
        obj.int("n", -12);
        obj.bool("yes", true);
        obj.string("s", "a\"b\u{2028}");
        obj.opt_string("none", None);
        obj.array("list", |arr| {
            arr.bytes(b"x\xffy");
            arr.array(|inner| inner.bytes(b""));
        });
        obj.array("o", |_| {});
    });
    let want = "{\"n\":-12,\"yes\":true,\"s\":\"a\\\"b\\u2028\",\"none\":null,\
         \"list\":[\"x\u{fffd}y\",[\"\"]],\"o\":[]}";
    assert_eq!(out, want);
    let same = encoded();
    assert!(same.starts_with(&want[..want.find("\"list\"").unwrap_or(0)]));
}

#[test]
fn an_empty_value_and_a_missing_one_are_both_null() {
    let mut out = String::new();
    jsonw::object(&mut out, |obj| {
        obj.non_empty("a", Some(b""));
        obj.non_empty("b", None);
        obj.non_empty("c", Some(b"v"));
        obj.opt_bytes("d", None);
        obj.opt_bytes("e", Some(b""));
    });
    assert_eq!(out, r#"{"a":null,"b":null,"c":"v","d":null,"e":""}"#);
}
