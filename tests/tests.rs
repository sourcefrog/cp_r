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
    assert_eq!(stats.file_buffer_reads, 1);

    assert_eq!(
        fs::metadata(&dest_file_path).unwrap().modified().unwrap(),
        fs::metadata(&src_file_path).unwrap().modified().unwrap()
    );
}

#[test]
fn larger_file_with_small_buffer_causes_multiple_reads() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let file_content = b"some file content\n".repeat(1000);
    let file_name = "a file";
    let small_copy_buffer_size = 200;

    fs::write(&src.path().join(file_name), &file_content).unwrap();

    let options = CopyOptions::new().with_copy_buffer_size(small_copy_buffer_size);
    let stats = copy_tree(src.path(), dest.path(), &options).unwrap();

    assert_eq!(
        fs::read(&dest.path().join(file_name)).unwrap(),
        file_content
    );
    assert_eq!(stats.files, 1);
    assert_eq!(stats.file_bytes, file_content.len() as u64);
    assert_eq!(
        stats.file_buffer_reads,
        file_content.len() / small_copy_buffer_size,
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

    let stats = copy_tree(src.path(), dest.path(), &CopyOptions::default()).unwrap();

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

#[test]
fn destination_must_already_exist() {
    // TODO: At least add an option to create the destination if it does not exist.
    // But, for now, it must.
    let dest_parent = tempfile::tempdir().unwrap();
    let dest = dest_parent.path().join("nonexistent_child");
    let err = copy_tree(Path::new("src"), &dest, &CopyOptions::new()).unwrap_err();
    println!("err = {:#?}", err);
    assert!(err.path().starts_with(&dest));
    assert_eq!(err.kind(), ErrorKind::WriteFile);
    // The actual IO error may vary per platform, but we can at least call it.
    assert_eq!(err.io_error().kind(), io::ErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn clean_error_failing_to_copy_devices() {
    let dest = tempfile::tempdir().unwrap();
    let err = copy_tree(&Path::new("/dev"), dest.path(), &Default::default()).unwrap_err();
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
    let stats = copy_tree(src.path(), dest.path(), &CopyOptions::new()).unwrap();
    println!("{:#?}", stats);
    assert_eq!(
        stats,
        CopyStats {
            files: 0,
            dirs: 0,
            symlinks: 1,
            file_bytes: 0,
            file_buffer_reads: 0,
        }
    );
}
