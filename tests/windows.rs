#![cfg(windows)]

use std::fs::{create_dir, metadata, read_link, read_to_string, symlink_metadata, write};
use std::os::windows::fs::{symlink_dir, symlink_file};
use std::path::Path;

use tempfile::TempDir;

use cp_r::CopyOptions;

#[test]
fn copy_file_symlink() {
    let tmp = TempDir::with_prefix("src").unwrap();
    let src = tmp.path();
    write(src.join("target"), b"hello").unwrap();
    symlink_file("target", src.join("link")).unwrap();

    let dest_tmp = TempDir::with_prefix("dest").unwrap();
    let dest = dest_tmp.path();
    CopyOptions::new().copy_tree(src, dest).unwrap();

    assert_eq!(read_to_string(dest.join("link")).unwrap(), "hello");
    assert_eq!(read_link(dest.join("link")).unwrap(), Path::new("target"));
}

#[test]
fn copy_dir_symlink() {
    let tmp = TempDir::with_prefix("src").unwrap();
    let src = tmp.path();
    create_dir(src.join("target")).unwrap();
    symlink_dir("target", src.join("link")).unwrap();
    println!(
        "source symlink target is {:?}",
        read_link(src.join("link")).unwrap()
    );

    let dest_tmp = TempDir::with_prefix("dest").unwrap();
    let dest = dest_tmp.path();
    CopyOptions::new().copy_tree(&src, &dest).unwrap();
    println!(
        "dest symlink target is {:?}",
        read_link(dest.join("link")).unwrap()
    );

    assert!(symlink_metadata(dest.join("link")).unwrap().is_symlink());
    assert!(metadata(dest.join("link")).unwrap().is_dir());
    assert_eq!(read_link(dest.join("link")).unwrap(), Path::new("target"));
}
