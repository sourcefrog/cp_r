#![cfg(windows)]

use std::fs::{create_dir, metadata, read_link, read_to_string, symlink_metadata, write};

use tempfile::TempDir;

use cp_r::CopyOptions;

#[test]
fn copy_file_symlink() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path();
    write(src.join("target"), b"hello").unwrap();
    std::os::windows::fs::symlink_file("target", src.join("link")).unwrap();

    let dest_tmp = TempDir::new().unwrap();
    let dest = dest_tmp.path();
    CopyOptions::new().copy_tree(src, dest).unwrap();

    assert_eq!(read_to_string(dest.join("link")).unwrap(), "hello");
    assert_eq!(read_link(dest.join("link")).unwrap(), dest.join("target"));
}

#[test]
fn copy_dir_symlink() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path();
    create_dir(src.join("target")).unwrap();
    std::os::windows::fs::symlink_dir("target", src.join("link")).unwrap();

    let dest_tmp = TempDir::new().unwrap();
    let dest = dest_tmp.path();
    CopyOptions::new().copy_tree(&src, &dest).unwrap();

    assert!(symlink_metadata(dest.join("link")).unwrap().is_symlink());
    assert!(metadata(dest.join("link")).unwrap().is_dir());
    assert_eq!(read_link(dest.join("link")).unwrap(), dest.join("target"));
}
