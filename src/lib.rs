// Copyright 2021-2024 Martin Pool

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
//! * A callback that can decide whether to continue after an error.
//! * Overwrite existing directories or files.
//! * Copy single files: don't assume the source path is a directory.
//! * A dry-run mode.
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
//! assert_eq!(stats.files, 2);
//! assert_eq!(stats.dirs, 0, "no children");
//! assert_eq!(stats.symlinks, 0, "no symlinks");
//! ```
//!
//! # Release history
//!
//! ## Unreleased
//!
//! * New: Copy symlinks on Windows.
//!
//! ## 0.5.1
//!
//! Released 2022-03-24.
//!
//! * Change: Ignore errors when trying to set the mtime on copied files. This can
//!   happen on Windows if the file is read-only.
//!
//! ## 0.5.0
//!
//! Released 2022-02-15
//!
//! ### API changes
//!
//! * The callback passed to [CopyOptions::after_entry_copied] now returns `Result<()>`
//!   (previously `()`), so it can return an Err to abort copying.
//!
//! ## 0.4.0
//!
//! Released 2021-11-30
//!
//! ### API changes
//!
//! * Remove `copy_buffer_size`, `file_buffers_copied`: these are too niche to have in the public
//!   API, and anyhow become meaningless when we use [std::fs::copy].
//!
//! * New [ErrorKind::DestinationDoesNotExist].
//!
//! * [Error::io_error] returns `Option<io::Error>` (previously just an `io::Error`):
//!   errors from this crate may not have a direct `io::Error` source.
//!
//! * [CopyOptions::copy_tree] arguments are relaxed to `AsRef<Path>` so that they will accept
//!   `&str`, `PathBuf`, `tempfile::TempDir`, etc.
//!
//! ### Improvements
//!
//! * Use [std::fs::copy], which is more efficient, and makes this crate simpler.
//!
//! ## 0.3.1
//!
//! Released 2021-11-07
//!
//! ### API changes
//!
//! * [CopyOptions::copy_tree] consumes `self` (rather than taking `&mut self`),
//!   which reduces lifetime issues in accessing values owned by callbacks.
//!
//! ### New features
//!
//! * [CopyOptions::after_entry_copied] callback added, which can be used for
//!   example to draw a progress bar.
//!
//! ## 0.3.0
//!
//! Released 2021-11-06
//!
//! ### API changes
//!
//! * [CopyOptions] builder functions now return `self` rather than `&mut self`.
//! * The actual copy operation is run by calling [CopyOptions::copy_tree],
//!   rather than passing the options as a parameter to `copy_tree`.
//! * Rename `with_copy_buffer_size` to `copy_buffer_size`.
//!
//! ### New features
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
use std::io;
use std::path::{Path, PathBuf};

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows::copy_symlink;

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
    after_entry_copied: Option<Box<dyn FnMut(&Path, &fs::FileType, &CopyStats) -> Result<()> + 'f>>,
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
    #[must_use]
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
    #[must_use]
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
    ///
    /// If the callback returns an error, it will abort the copy and the same
    /// error will be returned from [CopyOptions::copy_tree].
    #[must_use]
    pub fn after_entry_copied<F>(self, after_entry_copied: F) -> CopyOptions<'f>
    where
        F: FnMut(&Path, &fs::FileType, &CopyStats) -> Result<()> + 'f,
    {
        CopyOptions {
            after_entry_copied: Some(Box::new(after_entry_copied)),
            ..self
        }
    }

    /// Copy the tree according to the options.
    ///
    /// Returns [CopyStats] describing how many files were copied, etc.
    pub fn copy_tree<P, Q>(mut self, src: P, dest: Q) -> Result<CopyStats>
    where
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        let src = src.as_ref();
        let dest = dest.as_ref();

        let mut stats = CopyStats::default();

        // TODO: Handle the src not being a dir: copy that single entry.
        if self.create_destination {
            if !dest.is_dir() {
                copy_dir(src, dest, &mut stats)?;
            }
        } else if !dest.is_dir() {
            return Err(Error::new(ErrorKind::DestinationDoesNotExist, dest));
        }

        let mut subdir_queue: VecDeque<PathBuf> = VecDeque::new();
        subdir_queue.push_back(PathBuf::from(""));

        while let Some(subdir) = subdir_queue.pop_front() {
            let subdir_full_path = src.join(&subdir);
            for entry in fs::read_dir(&subdir_full_path)
                .map_err(|io| Error::from_io_error(io, ErrorKind::ReadDir, &subdir_full_path))?
            {
                let dir_entry = entry.map_err(|io| {
                    Error::from_io_error(io, ErrorKind::ReadDir, &subdir_full_path)
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
                let file_type = dir_entry
                    .file_type()
                    .map_err(|io| Error::from_io_error(io, ErrorKind::ReadDir, &src_fullpath))?;
                if file_type.is_file() {
                    copy_file(&src_fullpath, &dest_fullpath, &mut stats)?
                } else if file_type.is_dir() {
                    copy_dir(&src_fullpath, &dest_fullpath, &mut stats)?;
                    subdir_queue.push_back(entry_subpath.clone());
                } else if file_type.is_symlink() {
                    copy_symlink(&src_fullpath, &dest_fullpath, &mut stats)?
                } else {
                    // TODO: Include the file type.
                    return Err(Error::new(ErrorKind::UnsupportedFileType, src_fullpath));
                }
                if let Some(ref mut f) = self.after_entry_copied {
                    f(&entry_subpath, &file_type, &stats)?;
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
/// returned by [CopyOptions::copy_tree].
#[derive(Debug)]
pub struct Error {
    path: PathBuf,
    /// The original IO error, if any.
    io: Option<io::Error>,
    kind: ErrorKind,
}

/// A [std::result::Result] possibly containing a `cp_r` [Error].
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Construct a new error with no source.
    pub fn new<P>(kind: ErrorKind, path: P) -> Error
    where
        P: Into<PathBuf>,
    {
        Error {
            path: path.into(),
            kind,
            io: None,
        }
    }

    /// Construct a new error from a [std::io::Error].
    pub fn from_io_error<P>(io: io::Error, kind: ErrorKind, path: P) -> Error
    where
        P: Into<PathBuf>,
    {
        Error {
            path: path.into(),
            kind,
            io: Some(io),
        }
    }

    /// The path where this error occurred.
    pub fn path(&self) -> &Path {
        // TODO: Be consistent about whether this is relative to the root etc.
        &self.path
    }

    /// The IO error that caused this error, if any.
    pub fn io_error(&self) -> Option<&io::Error> {
        self.io.as_ref()
    }

    /// The kind of error that occurred.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // It seems like you should be able to spell this like `self.io.as_ref().into()` but that
        // doesn't work and I'm not sure why...
        if let Some(io) = &self.io {
            Some(io)
        } else {
            None
        }
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
            CopyFile => "copying file",
            DestinationDoesNotExist => "destination directory does not exist",
            Interrupted => "interrupted",
        };
        if let Some(io) = &self.io {
            write!(f, "{}: {}: {}", kind_msg, self.path.display(), io)
        } else {
            write!(f, "{}: {}", kind_msg, self.path.display())
        }
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
    /// Error in copying a file: might be a read or write error.
    CopyFile,
    /// Error creating a destination directory.
    CreateDir,
    /// Error reading a symlink.
    ReadSymlink,
    /// Error creating a symlink in the destination.
    CreateSymlink,
    /// The source tree contains a type of file that this library can't copy, such as a Unix
    /// FIFO.
    UnsupportedFileType,
    /// The destination directory does not exist.
    DestinationDoesNotExist,
    /// The copy was interrupted by the user.
    ///
    /// This is not currently generated internally by `cp_r` but can be returned
    /// by a callback.
    Interrupted,
}

fn copy_file(src: &Path, dest: &Path, stats: &mut CopyStats) -> Result<()> {
    // TODO: Optionally first check and error if the destination exists.
    let bytes_copied =
        fs::copy(src, dest).map_err(|io| Error::from_io_error(io, ErrorKind::CopyFile, src))?;
    stats.file_bytes += bytes_copied;

    let src_metadata = src
        .metadata()
        .map_err(|io| Error::from_io_error(io, ErrorKind::ReadFile, src))?;
    let src_mtime = filetime::FileTime::from_last_modification_time(&src_metadata);
    // It's OK if we can't set the mtime.
    let _ = filetime::set_file_mtime(dest, src_mtime);

    // Permissions should have already been set by fs::copy.
    stats.files += 1;
    Ok(())
}

fn copy_dir(_src: &Path, dest: &Path, stats: &mut CopyStats) -> Result<()> {
    fs::create_dir(dest)
        .map_err(|io| Error::from_io_error(io, ErrorKind::CreateDir, dest))
        .map(|()| stats.dirs += 1)
}

#[cfg(unix)]
fn copy_symlink(src: &Path, dest: &Path, stats: &mut CopyStats) -> Result<()> {
    let target =
        fs::read_link(src).map_err(|io| Error::from_io_error(io, ErrorKind::ReadSymlink, src))?;
    std::os::unix::fs::symlink(target, dest)
        .map_err(|io| Error::from_io_error(io, ErrorKind::CreateSymlink, dest))?;
    stats.symlinks += 1;
    Ok(())
}
