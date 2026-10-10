use super::{lower, simple_lower, simple_upper};

#[test]
fn the_simple_lower_case_is_one_character() {
    let rows = [
        ('A', 'a'),
        ('a', 'a'),
        ('\u{C9}', '\u{E9}'),
        ('\u{130}', 'i'),
        ('1', '1'),
        ('\u{DF}', '\u{DF}'),
    ];
    for (c, want) in rows {
        assert_eq!(simple_lower(c), want, "{c:?}");
    }
}

#[test]
fn the_simple_upper_case_is_one_character() {
    let rows = [
        ('a', 'A'),
        ('A', 'A'),
        ('\u{E9}', '\u{C9}'),
        ('\u{DF}', '\u{DF}'),
        ('1', '1'),
        ('\u{1F80}', '\u{1F88}'),
        ('\u{1FB3}', '\u{1FBC}'),
        ('\u{1FF3}', '\u{1FFC}'),
        ('\u{1F87}', '\u{1F8F}'),
        ('\u{1F90}', '\u{1F98}'),
        ('\u{1F97}', '\u{1F9F}'),
        ('\u{1FA0}', '\u{1FA8}'),
        ('\u{1FA7}', '\u{1FAF}'),
        ('\u{1FC3}', '\u{1FCC}'),
        ('\u{149}', '\u{149}'),
        ('\u{FB01}', '\u{FB01}'),
        ('\u{1F88}', '\u{1F88}'),
    ];
    for (c, want) in rows {
        assert_eq!(simple_upper(c), want, "{c:?}");
    }
}

#[test]
fn text_is_lower_cased_a_character_at_a_time() {
    assert_eq!(lower("Due:\u{130}X"), "due:ix");
}
