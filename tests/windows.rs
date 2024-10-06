#![cfg(windows)]

use std::fs::{read_link, read_to_string, write};
use std::path::Path;

use tempfile::TempDir;

use cp_r::CopyOptions;

#[test]
fn copy_file_symlink() {
    let tmp = TempDir::new().unwrap();
    write(tmp.path().join("target"), b"hello").unwrap();
    std::os::windows::fs::symlink_file("target", tmp.path().join("link")).unwrap();

    let dest = TempDir::new().unwrap();
    CopyOptions::new()
        .copy_tree(&tmp.path(), &dest.path())
        .unwrap();

    assert_eq!(read_to_string(dest.path().join("link")).unwrap(), "hello");
    assert_eq!(
        read_link(&dest.path().join("link")).unwrap(),
        Path::new("target")
    );
}
