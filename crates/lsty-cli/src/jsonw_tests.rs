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
