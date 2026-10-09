use super::jsonw;

#[test]
fn an_object_is_written_in_place() {
    let mut out = String::new();
    jsonw::object(&mut out, |obj| {
        obj.int("n", -12);
        obj.bool("yes", true);
        obj.string("s", "a\"b\u{2028}");
        obj.opt_string("none", None);
        obj.array("list", |arr| {
            arr.item_bytes(b"x\xffy");
            arr.item_array(|inner| inner.item_bytes(b""));
        });
        obj.array("o", |_| {});
    });
    let want = "{\"n\":-12,\"yes\":true,\"s\":\"a\\\"b\\u2028\",\"none\":null,\
         \"list\":[\"x\u{fffd}y\",[\"\"]],\"o\":[]}";
    assert_eq!(out, want);
}

#[test]
fn a_missing_bytes_value_is_null_and_an_empty_one_is_kept() {
    let mut out = String::new();
    jsonw::object(&mut out, |obj| {
        obj.opt_bytes("a", None);
        obj.opt_bytes("b", Some(b""));
        obj.opt_bytes("c", Some(b"v"));
    });
    assert_eq!(out, r#"{"a":null,"b":"","c":"v"}"#);
}
