use std::fs;

use super::test_util::{dir, put};
use super::{hash, read};
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
fn hash_of_equal_and_different_data() {
    // 04.22
    assert_eq!(hash(b"abc"), hash(b"abc"));
    assert_ne!(hash(b"abc"), hash(b"abd"));
    assert_eq!(hash(b"")[..4], [0xe3, 0xb0, 0xc4, 0x42]);
}
