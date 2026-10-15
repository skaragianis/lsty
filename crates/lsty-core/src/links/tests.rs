use std::{
    ffi::OsStr,
    fs,
    os::unix::fs::{PermissionsExt as _, symlink},
    path::{Path, PathBuf},
};

use super::{Kind, classify, editor_command};
use crate::fsio::test_util::dir;

fn missing(path: &Path) -> Kind {
    Kind::Missing(format!("{} does not exist", path.display()))
}

fn refused(path: &Path) -> Kind {
    Kind::Refused(format!("won't open executable: {}", path.display()))
}

fn file(root: &Path, name: &str, content: &[u8], mode: u32) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, content).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    path
}

#[test]
fn plain_paths() {
    let (_tmp, root) = dir();
    file(&root, "notes.md", b"# notes\n", 0o644);
    file(&root, "photo.png", b"\x89PNG\r\n\x1a\n\x00\x00", 0o644);
    fs::create_dir(root.join("folder")).unwrap();
    let rows = [
        ("gone.md", missing(&root.join("gone.md"))),
        ("notes.md", Kind::Text),
        ("photo.png", Kind::Other),
        ("folder", Kind::Other),
    ];
    for (name, want) in rows {
        assert_eq!(classify(&root.join(name)), want, "{name}");
    }
}

#[test]
fn programs_are_refused() {
    let (_tmp, root) = dir();
    file(&root, "run.sh", b"#!/bin/sh\n", 0o755);
    file(&root, "tool.desktop", b"[Desktop Entry]\n", 0o644);
    file(&root, "Tool.EXE", b"MZ", 0o644);
    file(&root, "x.jar", b"PK", 0o644);
    fs::create_dir(root.join("My.app")).unwrap();
    file(&root, "dir.app.txt", b"x", 0o644);
    for name in ["run.sh", "tool.desktop", "Tool.EXE", "x.jar", "My.app"] {
        let path = root.join(name);
        assert_eq!(classify(&path), refused(&path), "{name}");
    }
    assert_eq!(classify(&root.join("dir.app.txt")), Kind::Text);
}

#[test]
fn every_launcher_extension_is_refused_in_any_case() {
    let (_tmp, root) = dir();
    for ext in super::LAUNCHER_EXTS {
        for name in [format!("x{ext}"), format!("x{}", ext.to_uppercase())] {
            file(&root, &name, b"x", 0o644);
            let path = root.join(&name);
            assert_eq!(classify(&path), refused(&path), "{name}");
        }
    }
    file(&root, ".desktop", b"x", 0o644);
    assert_eq!(
        classify(&root.join(".desktop")),
        refused(&root.join(".desktop"))
    );
    file(&root, "x.app\u{130}mage", b"x", 0o644);
    let odd = root.join("x.app\u{130}mage");
    assert_eq!(classify(&odd), refused(&odd));
}

#[test]
fn a_script_without_execute_bits_and_an_empty_file_are_text() {
    let (_tmp, root) = dir();
    file(&root, "run.sh", b"#!/bin/sh\n", 0o644);
    file(&root, "empty", b"", 0o644);
    assert_eq!(classify(&root.join("run.sh")), Kind::Text);
    assert_eq!(classify(&root.join("empty")), Kind::Text);
}

#[test]
fn a_symlink_is_judged_by_its_target() {
    let (_tmp, root) = dir();
    let tool = file(&root, "tool", b"#!/bin/sh\n", 0o755);
    file(&root, "notes.md", b"n", 0o644);
    symlink(&tool, root.join("link")).unwrap();
    symlink(root.join("notes.md"), root.join("notes-link")).unwrap();
    symlink(root.join("notes.md"), root.join("go.desktop")).unwrap();
    symlink(root.join("nowhere"), root.join("dangling")).unwrap();
    let rows = [
        ("link", refused(&root.join("link"))),
        ("notes-link", Kind::Text),
        ("go.desktop", refused(&root.join("go.desktop"))),
        ("dangling", missing(&root.join("dangling"))),
    ];
    for (name, want) in rows {
        assert_eq!(classify(&root.join(name)), want, "{name}");
    }
}

#[test]
fn text_is_valid_utf8_without_nul() {
    let (_tmp, root) = dir();
    file(&root, "latin", b"caf\xe9", 0o644);
    file(&root, "nul", b"a\0b", 0o644);
    file(&root, "utf8", "caf\u{e9} \u{1f600}".as_bytes(), 0o644);
    assert_eq!(classify(&root.join("latin")), Kind::Other);
    assert_eq!(classify(&root.join("nul")), Kind::Other);
    assert_eq!(classify(&root.join("utf8")), Kind::Text);
}

#[test]
fn a_rune_cut_by_the_sample_does_not_matter() {
    let (_tmp, root) = dir();
    let mut cut = vec![b'a'; super::SNIFF_SIZE - 1];
    cut.extend_from_slice("\u{e9}".as_bytes());
    file(&root, "cut", &cut, 0o644);
    assert_eq!(classify(&root.join("cut")), Kind::Text);
    let mut bad = vec![b'a'; super::SNIFF_SIZE - 3];
    bad.extend_from_slice(b"\xff\xfe\xfd");
    file(&root, "bad", &bad, 0o644);
    assert_eq!(classify(&root.join("bad")), Kind::Other);
    let mut late = vec![b'a'; super::SNIFF_SIZE];
    late.push(0xff);
    file(&root, "late", &late, 0o644);
    assert_eq!(classify(&root.join("late")), Kind::Text);
}

#[test]
fn the_editor_command_splits_at_spaces() {
    let path = Path::new("/t/x.txt");
    let words = |e: Option<&str>| -> Vec<String> {
        editor_command(e.map(OsStr::new), path)
            .iter()
            .map(|w| w.to_string_lossy().into_owned())
            .collect()
    };
    assert_eq!(words(None), ["vi", "/t/x.txt"]);
    assert_eq!(words(Some("")), ["vi", "/t/x.txt"]);
    assert_eq!(words(Some("  ")), ["vi", "/t/x.txt"]);
    assert_eq!(words(Some("nano")), ["nano", "/t/x.txt"]);
    assert_eq!(words(Some("code  --wait")), ["code", "--wait", "/t/x.txt"]);
}

type WordRow = (&'static [u8], &'static [&'static [u8]]);

const WORD_ROWS: [WordRow; 7] = [
    (b"", &[]),
    (b"  a \t b\n", &[b"a", b"b"]),
    (b"/bin/ed\xe9 -w", &[b"/bin/ed\xe9", b"-w"]),
    (b"a\xffb c", &[b"a\xffb", b"c"]),
    ("a\u{a0}b".as_bytes(), &[b"a", b"b"]),
    ("a\u{3000}\u{3000}b".as_bytes(), &[b"a", b"b"]),
    (b"\xe9\xa0 x", &[b"\xe9\xa0", b"x"]),
];

#[test]
fn settings_split_at_white_space_and_keep_invalid_bytes() {
    use std::os::unix::ffi::OsStrExt as _;
    for (input, want) in WORD_ROWS {
        let got: Vec<Vec<u8>> = super::split_words(OsStr::from_bytes(input))
            .iter()
            .map(|w| w.as_bytes().to_vec())
            .collect();
        let want: Vec<Vec<u8>> = want.iter().map(|w| w.to_vec()).collect();
        assert_eq!(got, want, "{input:?}");
    }
}

#[test]
fn a_path_through_a_file_is_not_missing() {
    let (_tmp, root) = dir();
    file(&root, "notes.md", b"n", 0o644);
    // The folder part is a file: the system says "not a directory", which
    // is no reason to call the path missing.
    assert_eq!(classify(&root.join("notes.md/child")), Kind::Other);
}

#[test]
fn only_the_last_name_gives_the_extension() {
    let (_tmp, root) = dir();
    fs::create_dir(root.join("My.app")).unwrap();
    fs::create_dir(root.join("tool.desktop")).unwrap();
    file(&root, "My.app/readme", b"text", 0o644);
    file(&root, "tool.desktop/notes.md", b"text", 0o644);
    assert_eq!(classify(&root.join("My.app/readme")), Kind::Text);
    assert_eq!(classify(&root.join("tool.desktop/notes.md")), Kind::Text);
    assert_eq!(super::extension(Path::new("a.d/name")), b"");
    assert_eq!(super::extension(Path::new("a/b.txt")), b".txt");
    assert_eq!(super::extension(Path::new("/x/.desktop")), b".desktop");
}

#[test]
fn linked_files_are_resolved_in_line_order() {
    let snap = crate::list::test_util::snapshot(
        &["Read file:a.md file:~/b.md file:/c/../d.md"],
        &[],
    );
    let got =
        super::linked_files(&snap, &snap.tasks()[0], Some(Path::new("/h")));
    let want = [
        ("a.md", "/d/a.md"),
        ("~/b.md", "/h/b.md"),
        ("/c/../d.md", "/d.md"),
    ]
    .map(|(raw, path)| super::LinkedFile {
        raw: raw.into(),
        path: path.into(),
    });
    assert_eq!(got, want);
}

/// What `links` makes of a line, with the todo file in `/d`.
fn links_of(raw: &str) -> Vec<super::Link> {
    let snap = crate::list::test_util::snapshot(&[raw], &[]);
    super::links(&snap, &snap.tasks()[0], Some(Path::new("/h")))
}

fn file_link(raw: &str, path: &str) -> super::Link {
    super::Link::File(super::LinkedFile {
        raw: raw.into(),
        path: path.into(),
    })
}

fn url_link(url: &str) -> super::Link {
    super::Link::Url(url.into())
}

#[test]
fn files_and_urls_come_in_line_order() {
    let got = links_of(
        "See https://a.org/x, file:b.md (ftp://h/p). mailto:a@b.org \
         file:~/c.md",
    );
    let want = [
        url_link("https://a.org/x"),
        file_link("b.md", "/d/b.md"),
        url_link("ftp://h/p"),
        file_link("~/c.md", "/h/c.md"),
    ];
    assert_eq!(got, want);
}

#[test]
fn file_values_are_decoded() {
    let rows = [
        ("file:docs/contract%20v2.pdf", "/d/docs/contract v2.pdf"),
        ("file:a%2Gb", "/d/a%2Gb"),
        ("file:100%", "/d/100%"),
    ];
    for (raw, path) in rows {
        let got = links_of(raw);
        let value = raw.trim_start_matches("file:");
        assert_eq!(got, [file_link(value, path)], "{raw}");
    }
}

#[test]
fn quoted_words_and_other_tags_link_nowhere() {
    let got = links_of("`https://a.org` `file:x` color:red h:1 http:/x");
    assert_eq!(got, []);
}

#[test]
fn linked_files_skip_urls() {
    let snap = crate::list::test_util::snapshot(
        &["https://a.org file:b.md https://c.org"],
        &[],
    );
    let got = super::linked_files(&snap, &snap.tasks()[0], None);
    assert_eq!(got.len(), 1);
}
