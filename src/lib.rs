// Copyright 2021 Martin Pool

//! Copy a directory tree, including mtimes and permissions.
//!
//! # Features
//!
//! * Minimal dependencies: currently just `filetime` to support copying mtimes.
//! * Returns [CopyStats] describing how much data and how many files were copied.
//! * Tested on Linux, macOS and Windows.
//! * Copies mtimes and permissions.
//!
//! # Missing features that could be added
//!
//! * Options to _not_ copy mtimes or permissions.
//! * Continue copying after an error.
//! * Callbacks for logging, error handling, filtering, etc.
//! * Overwrite existing directories or files.

use std::collections::VecDeque;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct CopyOptions {
    // TODO: Continue or stop on error?
    // TODO: Option controlling whether to copy mtimes?
    // TODO: Copy permissions?
    // TODO: Option to create destination directory...
    pub copy_buffer_size: usize,
}

impl Default for CopyOptions {
    fn default() -> CopyOptions {
        CopyOptions {
            copy_buffer_size: 8 << 20,
        }
    }
}

/// Counters of how many things were copied.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CopyStats {
    pub files: usize,
    pub dirs: usize,
    pub symlinks: usize,
    pub file_bytes: u64,
    pub file_blocks: usize,
}

#[derive(Debug)]
pub struct Error {
    path: PathBuf,
    io: io::Error,
    kind: ErrorKind,
}

impl Error {
    /// Construct a new error.
    fn new(kind: ErrorKind, path: PathBuf, message: String) -> Error {
        let io_kind: io::ErrorKind = match kind {
            ErrorKind::UnsupportedFileType => io::ErrorKind::Unsupported,
            other => unimplemented!("unhandled {:?}", other),
        };
        Error {
            path,
            kind,
            io: io::Error::new(io_kind, message),
        }
    }

    /// The path where this error occurred.
    pub fn path(&self) -> &Path {
        // TODO: Be consistent about whether this is relative to the root etc.
        &self.path
    }

    /// The IO error that caused this error, or a description of this error as an IO error, if it
    /// was not directly caused by one.
    pub fn io_error(&self) -> &io::Error {
        &self.io
    }

    /// The kind of error that occurred.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

/// Various kinds of errors that can occur while copying a tree.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Error listing a source directory.
    ReadDir,
    /// Error opening or reading a source file.
    ReadFile,
    /// Error creating or writing a destination file.
    WriteFile,
    /// Error creating a destination directory.
    CreateDir,
    /// Error reading a symlink.
    ReadSymlink,
    /// Error creating a symlink in the destination.
    CreateSymlink,
    /// The source tree contains a type of file that this library can't copy, such as a Unix
    /// FIFO.
    UnsupportedFileType,
}

pub fn copy_tree(src: &Path, dest: &Path, options: &CopyOptions) -> Result<CopyStats, Error> {
    let mut stats = CopyStats::default();
    let mut subdir_queue: VecDeque<PathBuf> = VecDeque::new();
    subdir_queue.push_back(PathBuf::from(""));
    assert!(options.copy_buffer_size > 0);
    let mut copy_buf = vec![0u8; options.copy_buffer_size];

    while let Some(subdir) = subdir_queue.pop_front() {
        let subdir_full_path = src.join(&subdir);
        for entry in fs::read_dir(&subdir_full_path).map_err(|io| Error {
            path: subdir_full_path.clone(),
            io,
            kind: ErrorKind::ReadDir,
        })? {
            let entry = entry.map_err(|io| Error {
                path: subdir_full_path.clone(),
                io,
                kind: ErrorKind::ReadDir,
            })?;
            let entry_subpath = subdir.join(entry.file_name());
            let src_fullpath = src.join(&entry_subpath);
            let dest_fullpath = dest.join(&entry_subpath);
            match entry.file_type().map_err(|io| Error {
                path: src_fullpath.clone(),
                io,
                kind: ErrorKind::ReadDir,
            })? {
                t if t.is_file() => {
                    copy_file(&src_fullpath, &dest_fullpath, &mut copy_buf, &mut stats)?
                }
                t if t.is_dir() => {
                    copy_dir(&src_fullpath, &dest_fullpath, &mut stats)?;
                    subdir_queue.push_back(entry_subpath);
                }
                t if t.is_symlink() => copy_symlink(&src_fullpath, &dest_fullpath, &mut stats)?,
                t => {
                    return Err(Error::new(
                        ErrorKind::UnsupportedFileType,
                        src_fullpath,
                        format!("unsupported file type {:?}", t),
                    ))
                }
            }
        }
    }
    Ok(stats)
}

fn copy_file(src: &Path, dest: &Path, buf: &mut [u8], stats: &mut CopyStats) -> Result<(), Error> {
    let mut inf = fs::OpenOptions::new()
        .read(true)
        .open(src)
        .map_err(|io| Error {
            kind: ErrorKind::ReadFile,
            path: src.to_owned(),
            io,
        })?;
    let mut outf = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(dest)
        .map_err(|io| Error {
            kind: ErrorKind::WriteFile,
            path: dest.to_owned(),
            io,
        })?;
    loop {
        let len = inf.read(buf).map_err(|io| Error {
            kind: ErrorKind::ReadFile,
            path: src.to_owned(),
            io,
        })?;
        if len == 0 {
            break;
        }
        stats.file_bytes += len as u64;
        stats.file_blocks += 1;
        outf.write_all(&buf[..len]).map_err(|io| Error {
            kind: ErrorKind::WriteFile,
            path: dest.to_owned(),
            io,
        })?;
    }

    let inf_metadata = inf.metadata().map_err(|io| Error {
        kind: ErrorKind::ReadFile,
        path: src.to_owned(),
        io,
    })?;

    let src_mtime = filetime::FileTime::from_last_modification_time(&inf_metadata);
    filetime::set_file_handle_times(&outf, None, Some(src_mtime)).map_err(|io| Error {
        kind: ErrorKind::WriteFile,
        path: dest.to_owned(),
        io,
    })?;

    outf.set_permissions(inf_metadata.permissions())
        .map_err(|io| Error {
            kind: ErrorKind::WriteFile,
            path: dest.to_owned(),
            io,
        })?;

    stats.files += 1;
    Ok(())
}

fn copy_dir(_src: &Path, dest: &Path, stats: &mut CopyStats) -> Result<(), Error> {
    fs::create_dir(dest)
        .map_err(|io| Error {
            kind: ErrorKind::CreateDir,
            path: dest.to_owned(),
            io,
        })
        .map(|()| stats.dirs += 1)
}

#[cfg(unix)]
fn copy_symlink(src: &Path, dest: &Path, stats: &mut CopyStats) -> Result<(), Error> {
    let target = fs::read_link(src).map_err(|io| Error {
        kind: ErrorKind::ReadSymlink,
        path: src.to_owned(),
        io,
    })?;
    std::os::unix::fs::symlink(target, dest).map_err(|io| Error {
        kind: ErrorKind::CreateSymlink,
        path: dest.to_owned(),
        io,
    })?;
    stats.symlinks += 1;
    Ok(())
}

#[cfg(windows)]
fn copy_symlink(_src: &Path, _dest: &Path, _stats: &mut CopyStats) -> Result<(), Error> {
    unimplemented!("symlinks are not yet supported on Windows");
}
