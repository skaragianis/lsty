use std::fs;

use super::test_util::{dir, put};
use super::{read, read_in};
use crate::errs::Code;

const RAW: &[u8] = b"\xEF\xBB\xBFone\r\ntwo\r\nlast";

#[test]
fn read_keeps_every_byte() {
    // 04.1
    let (_tmp, d) = dir();
    let path = d.join("todo.txt");
    fs::write(&path, RAW).unwrap();
    assert_eq!(read(&path).unwrap().as_deref(), Some(RAW));
}

#[test]
fn read_of_a_missing_file() {
    // 04.2
    let (_tmp, d) = dir();
    assert_eq!(read(&d.join("none.txt")).unwrap(), None);
}

#[test]
fn read_of_a_folder() {
    // 04.3
    let (_tmp, d) = dir();
    let err = read(&d).unwrap_err();
    assert_eq!(err.code(), Code::Io);
    let want = format!("read {0}: read {0}: is a directory", d.display());
    assert_eq!(err.message(), want);
}

#[test]
fn read_of_a_file_over_4_gib_is_refused() {
    let (_tmp, d) = dir();
    let path = d.join("todo.txt");
    let file = fs::File::create(&path).unwrap();
    file.set_len(u64::from(u32::MAX) + 1).unwrap();
    let err = read(&path).unwrap_err();
    assert_eq!(err.code(), Code::Io);
    let want = format!("read {0}: read {0}: file too large", path.display());
    assert_eq!(err.message(), want);
}

#[test]
fn read_of_an_unreadable_file() {
    let (_tmp, d) = dir();
    let path = d.join("todo.txt");
    put(&path, "x", 0o000);
    if fs::File::open(&path).is_ok() {
        return; // running as root
    }
    let err = read(&path).unwrap_err();
    assert_eq!(err.code(), Code::Io);
    let want =
        format!("read {0}: open {0}: permission denied", path.display());
    assert_eq!(err.message(), want);
}

#[test]
fn read_through_a_regular_file_is_an_error() {
    let (_tmp, d) = dir();
    put(&d.join("f"), "", 0o644);
    let path = d.join("f").join("todo.txt");
    let err = read(&path).unwrap_err();
    let want = format!("read {0}: open {0}: not a directory", path.display());
    assert_eq!(err.message(), want);
}

#[test]
fn read_in_opens_a_relative_path_from_the_folder_given() {
    let (_tmp, d) = dir();
    fs::write(d.join("keys.toml"), RAW).unwrap();
    let path = std::path::Path::new("keys.toml");
    assert_eq!(read_in(&d, path).unwrap().unwrap(), RAW);
    assert_eq!(
        read_in(std::path::Path::new("/"), &d.join(path))
            .unwrap()
            .unwrap(),
        RAW
    );
}

#[test]
fn read_in_says_none_when_missing_and_words_a_failure() {
    let (_tmp, d) = dir();
    let gone = std::path::Path::new("gone.toml");
    assert_eq!(read_in(&d, gone).unwrap(), None);
    fs::create_dir(d.join("folder")).unwrap();
    let err = read_in(&d, std::path::Path::new("folder")).unwrap_err();
    assert_eq!(err.message(), "read folder: is a directory");
}
