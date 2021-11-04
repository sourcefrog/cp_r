// Copyright 2021 Martin Pool

//! Public API tests.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use cp_r::*;

#[test]
fn basic_copy() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let file_content = b"hello world\n";
    let file_name = "a file";
    let src_file_path = src.path().join(file_name);
    fs::write(&src_file_path, file_content).unwrap();

    let stats = CopyOptions::default()
        .copy_tree(src.path(), dest.path())
        .unwrap();

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

    let stats = CopyOptions::new()
        .copy_buffer_size(small_copy_buffer_size)
        .copy_tree(src.path(), dest.path())
        .unwrap();

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

    let stats = CopyOptions::default()
        .copy_tree(src.path(), dest.path())
        .unwrap();

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
fn clean_error_on_nonexistent_source() {
    let dest = tempfile::tempdir().unwrap();
    let err = CopyOptions::new()
        .copy_tree(Path::new("nothing"), &dest.path())
        .unwrap_err();
    println!("err = {:#?}", err);
    assert!(err.path().starts_with("nothing"));
    assert_eq!(err.kind(), ErrorKind::ReadDir);
    assert_eq!(err.io_error().kind(), io::ErrorKind::NotFound);
}

#[test]
fn create_destination_by_default() {
    let empty_src = tempfile::tempdir().unwrap();
    let dest_parent = tempfile::tempdir().unwrap();
    let dest = dest_parent.path().join("nonexistent_child");
    let stats = CopyOptions::new()
        .copy_tree(&empty_src.path(), &dest)
        .unwrap();
    assert!(dest.is_dir());
    assert_eq!(stats.dirs, 1);
    assert_eq!(stats.files, 0);
}

#[test]
fn create_destination_when_requested() {
    let empty_src = tempfile::tempdir().unwrap();
    let dest_parent = tempfile::tempdir().unwrap();
    let dest = dest_parent.path().join("nonexistent_child");
    let stats = CopyOptions::new()
        .create_destination(true)
        .copy_tree(&empty_src.path(), &dest)
        .unwrap();
    assert!(dest.is_dir());
    assert_eq!(stats.dirs, 1);
    assert_eq!(stats.files, 0);
}

#[test]
fn optionally_destination_must_exist() {
    // TODO: At least add an option to create the destination if it does not exist.
    // But, for now, it must.
    let dest_parent = tempfile::tempdir().unwrap();
    let dest = dest_parent.path().join("nonexistent_child");
    let err = CopyOptions::new()
        .create_destination(false)
        .copy_tree(Path::new("src"), &dest)
        .unwrap_err();
    println!("err = {:#?}", err);
    assert!(err.path().starts_with(&dest));
    assert_eq!(err.kind(), ErrorKind::WriteFile);
    assert_eq!(err.io_error().kind(), io::ErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn clean_error_failing_to_copy_devices() {
    let dest = tempfile::tempdir().unwrap();
    let err = CopyOptions::new()
        .copy_tree(&Path::new("/dev"), dest.path())
        .unwrap_err();
    println!("{:#?}", err);
    assert_eq!(err.kind(), ErrorKind::UnsupportedFileType);
    assert_eq!(err.io_error().kind(), io::ErrorKind::Unsupported);
    assert!(err.path().strip_prefix("/dev/").is_ok());
    assert!(format!("{}", err).starts_with("unsupported file type: /dev/"));
}

#[cfg(unix)]
#[test]
fn copy_dangling_symlink() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("dangling target", src.path().join("a_link")).unwrap();
    let stats = CopyOptions::new()
        .copy_tree(src.path(), dest.path())
        .unwrap();
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

#[test]
fn filter_by_path() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();

    fs::create_dir(&src.path().join("a")).unwrap();
    fs::create_dir(&src.path().join("b")).unwrap();
    fs::create_dir(&src.path().join("b/bb")).unwrap();
    fs::create_dir(&src.path().join("a").join("aa")).unwrap();

    let file_content = b"some file content\n";
    fs::write(&src.path().join("a/aa/aaafile"), &file_content).unwrap();

    // let mut filter_seen_paths : Vec<PathBuf> = Vec::new();

    // let options = CopyOptions::default().filter(|path, de| {
    //     filter_seen_paths.push(path.to_owned());
    //     if path == Path::new("b") {
    //         Ok(false)
    //     } else {Ok(true)}
    // });
    fn not_b(path: &Path, _: &fs::DirEntry) -> cp_r::Result<bool> {
        Ok(path != Path::new("b"))
    }
    let stats = CopyOptions::new()
        .filter(not_b)
        .copy_tree(src.path(), dest.path())
        .unwrap();

    assert_eq!(
        fs::read(&dest.path().join("a/aa/aaafile")).unwrap(),
        file_content
    );
    assert!(!dest.path().join("b").exists());
    assert_eq!(stats.files, 1);
    assert_eq!(stats.file_bytes, file_content.len() as u64);
    assert_eq!(stats.dirs, 2);
}

#[test]
fn filter_by_mut_closure() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();

    fs::create_dir(&src.path().join("a")).unwrap();
    fs::create_dir(&src.path().join("b")).unwrap();
    fs::create_dir(&src.path().join("b/bb")).unwrap();
    fs::create_dir(&src.path().join("a").join("aa")).unwrap();

    let file_content = b"some file content\n";
    fs::write(&src.path().join("a/aa/aaafile"), &file_content).unwrap();

    // Filter paths and also collect all the paths we've seen, as an example of a filter
    // that's more than a simple function pointer.
    let mut filter_seen_paths: Vec<PathBuf> = Vec::new();
    let stats = CopyOptions::default()
        .filter(move |path: &Path, _de| {
            filter_seen_paths.push(path.to_owned());
            Ok(path != Path::new("b"))
        })
        .copy_tree(src.path(), dest.path())
        .unwrap();

    assert_eq!(
        fs::read(&dest.path().join("a/aa/aaafile")).unwrap(),
        file_content
    );
    assert!(!dest.path().join("b").exists());
    assert_eq!(stats.files, 1);
    assert_eq!(stats.file_bytes, file_content.len() as u64);
    assert_eq!(stats.dirs, 2);
}
