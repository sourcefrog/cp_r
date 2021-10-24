// Copyright 2021 Martin Pool

//! Public API tests.

use std::fs;
use std::io;
use std::path::Path;

use cp_r::*;

#[test]
fn basic_copy() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let file_content = b"hello world\n";
    let file_name = "a file";
    let src_file_path = src.path().join(file_name);
    fs::write(&src_file_path, file_content).unwrap();

    let options = CopyOptions::default();
    let stats = copy_tree(src.path(), dest.path(), &options).unwrap();

    let dest_file_path = &dest.path().join(file_name);
    assert_eq!(fs::read(&dest_file_path).unwrap(), file_content);
    assert_eq!(stats.files, 1);
    assert_eq!(stats.file_bytes, file_content.len() as u64);
    assert_eq!(stats.file_blocks, 1);

    assert_eq!(
        fs::metadata(&dest_file_path).unwrap().modified().unwrap(),
        fs::metadata(&src_file_path).unwrap().modified().unwrap()
    );
}

#[test]
fn larger_file() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let file_content = b"some file content\n".repeat(1000);
    let file_name = "a file";
    fs::write(&src.path().join(file_name), &file_content).unwrap();

    let options = CopyOptions {
        copy_buffer_size: 200,
        ..Default::default()
    };
    let stats = copy_tree(src.path(), dest.path(), &options).unwrap();

    assert_eq!(
        fs::read(&dest.path().join(file_name)).unwrap(),
        file_content
    );
    assert_eq!(stats.files, 1);
    assert_eq!(stats.file_bytes, file_content.len() as u64);
    assert_eq!(
        stats.file_blocks,
        file_content.len() / options.copy_buffer_size
    );
}

#[test]
fn subdirs() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();

    fs::create_dir(&src.path().join("a")).unwrap();
    fs::create_dir(&src.path().join("b")).unwrap();
    fs::create_dir(&src.path().join("b/bb")).unwrap();
    fs::create_dir(&src.path().join("a").join("aa")).unwrap();

    let file_content = b"some file content\n";
    fs::write(&src.path().join("a/aa/aaafile"), &file_content).unwrap();

    let options = CopyOptions {
        ..Default::default()
    };
    let stats = copy_tree(src.path(), dest.path(), &options).unwrap();

    assert_eq!(
        fs::read(&dest.path().join("a/aa/aaafile")).unwrap(),
        file_content
    );
    assert!(fs::metadata(&dest.path().join("b/bb"))
        .unwrap()
        .file_type()
        .is_dir());
    assert_eq!(stats.files, 1);
    assert_eq!(stats.file_bytes, file_content.len() as u64);
    assert_eq!(stats.dirs, 4);
}

#[cfg(unix)]
#[test]
fn clean_error_failing_to_copy_devices() {
    let dest = tempfile::tempdir().unwrap();
    let options = CopyOptions {
        ..Default::default()
    };
    let err = copy_tree(&Path::new("/dev"), dest.path(), &options).unwrap_err();
    println!("{:#?}", err);
    assert_eq!(err.kind(), ErrorKind::UnsupportedFileType);
    assert_eq!(err.io_error().kind(), io::ErrorKind::Unsupported);
    assert!(err.path().strip_prefix("/dev/").is_ok());
}

#[cfg(unix)]
#[test]
fn copy_dangling_symlink() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("dangling target", src.path().join("a_link")).unwrap();
    let options = CopyOptions {
        ..Default::default()
    };
    let stats = copy_tree(src.path(), dest.path(), &options).unwrap();
    println!("{:#?}", stats);
    assert_eq!(
        stats,
        CopyStats {
            files: 0,
            dirs: 0,
            symlinks: 1,
            file_bytes: 0,
            file_blocks: 0,
        }
    );
}
