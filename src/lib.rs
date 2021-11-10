// Copyright 2021 Martin Pool

//! Copy a directory tree, including mtimes and permissions.
//!
//! To copy a tree, first configure parameters on [CopyOptions] and then call
//! [CopyOptions::copy_tree].
//!
//! # Features
//!
//! * Minimal dependencies: currently just `filetime` to support copying mtimes.
//! * Returns [CopyStats] describing how much data and how many files were
//!   copied.
//! * Tested on Linux, macOS and Windows.
//! * Copies mtimes and permissions.
//! * Takes an optional callback to decide which entries are copied or skipped,
//!   [CopyOptions::filter].
//! * Takes an optional callback to show progress or record which files are copied,
//!   [CopyOptions::after_entry_copied].
//!
//! # Missing features that could be added
//!
//! * Options to _not_ copy mtimes or permissions.
//! * Continue copying after an error.
//! * Callbacks for progress or logging, error handling, etc.
//! * Overwrite existing directories or files.
//! * Copy single files: don't assume the source path is a directory.
//!
//! # Example
//!
//! ```
//! use std::path::Path;
//! use cp_r::{CopyOptions, CopyStats};
//! use tempfile;
//!
//! // Copy this crate's `src` directory.
//! let dest = tempfile::tempdir().unwrap();
//! let stats = CopyOptions::new().copy_tree(Path::new("src"), dest.path()).unwrap();
//! assert_eq!(stats.files, 1, "only one file in src");
//! assert_eq!(stats.dirs, 0, "no children");
//! assert_eq!(stats.symlinks, 0, "no symlinks");
//! ```
//!
//! # Release history
//!
//! ## unreleased
//!
//! API changes:
//!
//! * Remove `copy_buffer_size`, `file_buffers_copied`: these are too niche to have in the public
//!   API, and anyhow become meaningless when we use [std::fs::copy].
//!
//! ## 0.3.1
//!
//! Released 2021-11-07
//!
//! API changes:
//!
//! * [CopyOptions::copy_tree] consumes `self` (rather than taking `&mut self`),
//!   which reduces lifetime issues in accessing values owned by callbacks.
//!
//! New features:
//!
//! * [CopyOptions::after_entry_copied] callback added, which can be used for
//!   example to draw a progress bar.
//!
//! ## 0.3.0
//!
//! Released 2021-11-06
//!
//! API changes:
//!
//! * [CopyOptions] builder functions now return `self` rather than `&mut self`.
//! * The actual copy operation is run by calling [CopyOptions::copy_tree],
//!   rather than passing the options as a parameter to `copy_tree`.
//! * Rename `with_copy_buffer_size` to `copy_buffer_size`.
//!
//! New features:
//! * A new option to provide a filter on which entries should be copied,
//!   through [CopyOptions::filter].
//!
//! ## 0.2.0
//! * `copy_tree` will create the immediate destination directory by default,
//!   but this can be controlled by [CopyOptions::create_destination]. The
//!   destination, if created, is counted in [CopyStats::dirs] and inherits its
//!   permissions from the source.
//!
//! ## 0.1.1
//! * [Error] implements [std::error::Error] and [std::fmt::Display].
//!
//! * [Error] is tested to be compatible with [Anyhow](https://docs.rs/anyhow).
//!   (There is only a dev-dependency on Anyhow; users of this library won't
//!   pull it in.)
//!
//! ## 0.1.0
//! * Initial release.

#![warn(missing_docs)]

use std::collections::VecDeque;
use std::fmt;
use std::fs::{self, DirEntry};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const COPY_BUFFER_SIZE: usize = 1 << 20;

/// Options for copying file trees.
///
/// Default options may be OK for many callers:
/// * Preserve mtime and permissions.
/// * Create the destination if it does not exist.
pub struct CopyOptions<'f> {
    // TODO: Continue or stop on error?
    // TODO: Option controlling whether to copy mtimes?
    // TODO: Copy permissions?
    create_destination: bool,

    // I agree with Clippy that the callbacks are complex types, but stable Rust
    // seems to have no other way to spell it, because you can't make a type or
    // trait alias for a Fn.
    #[allow(clippy::type_complexity)]
    filter: Option<Box<dyn FnMut(&Path, &DirEntry) -> Result<bool> + 'f>>,

    #[allow(clippy::type_complexity)]
    after_entry_copied: Option<Box<dyn FnMut(&Path, &fs::FileType, &CopyStats) + 'f>>,
}

impl<'f> Default for CopyOptions<'f> {
    fn default() -> CopyOptions<'f> {
        CopyOptions {
            create_destination: true,
            filter: None,
            after_entry_copied: None,
        }
    }
}

impl<'f> CopyOptions<'f> {
    /// Construct reasonable default options.
    pub fn new() -> CopyOptions<'f> {
        CopyOptions::default()
    }

    /// Set whether to create the destination if it does not exist (the default), or return an error.
    ///
    /// Only the immediate destination is created, not all its parents.
    pub fn create_destination(self, create_destination: bool) -> CopyOptions<'f> {
        CopyOptions {
            create_destination,
            ..self
        }
    }

    /// Set a filter callback that can determine which files should be copied.
    ///
    /// The filter can return
    /// * `Ok(true)` to copy an entry (and recursively continue into directories)
    /// * `Ok(false)` to skip an entry (and anything inside the directory)
    /// * `Err(_)` to stop copying and return this error
    ///
    /// The path is relative to the top of the tree. The [std::fs::DirEntry] gives access to the file type and other metadata of the source file.
    ///
    /// ```
    /// use std::fs;
    /// use std::path::Path;
    /// use cp_r::CopyOptions;
    ///
    /// let src = tempfile::tempdir().unwrap();
    /// fs::write(src.path().join("transient.tmp"), b"hello?").unwrap();
    /// fs::write(src.path().join("permanent.txt"), b"hello?").unwrap();
    /// let dest = tempfile::tempdir().unwrap();
    ///
    /// let stats = CopyOptions::new()
    ///     .filter(
    ///         |path, _| Ok(path.extension().and_then(|s| s.to_str()) != Some("tmp")))
    ///     .copy_tree(&src.path(), &dest.path())
    ///     .unwrap();
    ///
    /// assert!(dest.path().join("permanent.txt").exists());
    /// assert!(!dest.path().join("transient.tmp").exists());
    /// assert_eq!(stats.filtered_out, 1);
    /// assert_eq!(stats.files, 1);
    /// ```
    ///
    /// *Note:* Due to limitations in the current Rust compiler's type inference
    /// for closures, filter closures may give errors about lifetimes if they are
    /// assigned to to a variable rather than declared inline in the parameter.
    pub fn filter<F>(self, filter: F) -> CopyOptions<'f>
    where
        F: FnMut(&Path, &DirEntry) -> Result<bool> + 'f,
    {
        CopyOptions {
            filter: Some(Box::new(filter)),
            ..self
        }
    }

    /// Set a progress callback that's called after each entry is successfully copied.
    ///
    /// The callback is passed:
    /// * The path, relative to the top of the tree, that was just copied.
    /// * The [std::fs::FileType] of the entry that was copied.
    /// * The [stats](CopyStats) so far, including the number of files copied.
    pub fn after_entry_copied<F>(self, after_entry_copied: F) -> CopyOptions<'f>
    where
        F: FnMut(&Path, &fs::FileType, &CopyStats) + 'f,
    {
        CopyOptions {
            after_entry_copied: Some(Box::new(after_entry_copied)),
            ..self
        }
    }

    /// Copy the tree according to the options.
    ///
    /// Returns [CopyStats] describing how many files were copied, etc.
    pub fn copy_tree(mut self, src: &Path, dest: &Path) -> Result<CopyStats> {
        let mut stats = CopyStats::default();

        if self.create_destination && !dest.is_dir() {
            copy_dir(src, dest, &mut stats)?;
        }

        let mut subdir_queue: VecDeque<PathBuf> = VecDeque::new();
        subdir_queue.push_back(PathBuf::from(""));
        let mut copy_buf = vec![0u8; COPY_BUFFER_SIZE];

        while let Some(subdir) = subdir_queue.pop_front() {
            let subdir_full_path = src.join(&subdir);
            for entry in fs::read_dir(&subdir_full_path).map_err(|io| Error {
                path: subdir_full_path.clone(),
                io,
                kind: ErrorKind::ReadDir,
            })? {
                let dir_entry = entry.map_err(|io| Error {
                    path: subdir_full_path.clone(),
                    io,
                    kind: ErrorKind::ReadDir,
                })?;
                let entry_subpath = subdir.join(dir_entry.file_name());
                if let Some(filter) = &mut self.filter {
                    if !filter(&entry_subpath, &dir_entry)? {
                        stats.filtered_out += 1;
                        continue;
                    }
                }
                let src_fullpath = src.join(&entry_subpath);
                let dest_fullpath = dest.join(&entry_subpath);
                let file_type = dir_entry.file_type().map_err(|io| Error {
                    path: src_fullpath.clone(),
                    io,
                    kind: ErrorKind::ReadDir,
                })?;
                if file_type.is_file() {
                    copy_file(&src_fullpath, &dest_fullpath, &mut copy_buf, &mut stats)?
                } else if file_type.is_dir() {
                    copy_dir(&src_fullpath, &dest_fullpath, &mut stats)?;
                    subdir_queue.push_back(entry_subpath.clone());
                } else if file_type.is_symlink() {
                    copy_symlink(&src_fullpath, &dest_fullpath, &mut stats)?
                } else {
                    return Err(Error::new(
                        ErrorKind::UnsupportedFileType,
                        src_fullpath,
                        format!("unsupported file type {:?}", file_type),
                    ));
                }
                if let Some(ref mut f) = self.after_entry_copied {
                    f(&entry_subpath, &file_type, &stats);
                }
            }
        }
        Ok(stats)
    }
}

/// Counters of how many things were copied.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct CopyStats {
    /// The number of plain files copied.
    pub files: usize,
    /// The number of directories copied.
    pub dirs: usize,
    /// The number of symlinks copied.
    pub symlinks: usize,
    /// The number of bytes of file content copied, across all files.
    pub file_bytes: u64,
    /// The number of entries filtered out by the [CopyOptions::filter] callback.
    pub filtered_out: usize,
}

/// An error from copying a tree.
///
/// At present this library does not support continuing after an error, so only the first error is
/// returned.
#[derive(Debug)]
pub struct Error {
    path: PathBuf,
    io: io::Error,
    kind: ErrorKind,
}

/// A [std::result::Result] possibly containing a cp_r [Error].
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Construct a new error.
    pub fn new(kind: ErrorKind, path: PathBuf, message: String) -> Error {
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

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.io)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ErrorKind::*;
        let kind_msg = match self.kind {
            ReadDir => "reading source directory",
            ReadFile => "reading source file",
            WriteFile => "writing file",
            CreateDir => "creating directory",
            ReadSymlink => "reading symlink",
            CreateSymlink => "creating symlink",
            UnsupportedFileType => "unsupported file type",
        };
        write!(f, "{}: {}: {}", kind_msg, self.path.display(), self.io)
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

fn copy_file(src: &Path, dest: &Path, buf: &mut [u8], stats: &mut CopyStats) -> Result<()> {
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

fn copy_dir(_src: &Path, dest: &Path, stats: &mut CopyStats) -> Result<()> {
    fs::create_dir(dest)
        .map_err(|io| Error {
            kind: ErrorKind::CreateDir,
            path: dest.to_owned(),
            io,
        })
        .map(|()| stats.dirs += 1)
}

#[cfg(unix)]
fn copy_symlink(src: &Path, dest: &Path, stats: &mut CopyStats) -> Result<()> {
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
fn copy_symlink(_src: &Path, _dest: &Path, _stats: &mut CopyStats) -> Result<()> {
    unimplemented!("symlinks are not yet supported on Windows");
}
