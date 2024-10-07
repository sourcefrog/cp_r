use std::fs::{read_link, symlink_metadata};
use std::os::windows::fs::{symlink_dir, symlink_file};
use std::path::Path;

use super::*;

pub(super) fn copy_symlink(src: &Path, dest: &Path, _stats: &mut CopyStats) -> Result<()> {
    let target =
        read_link(src).map_err(|io| Error::from_io_error(io, ErrorKind::ReadSymlink, src))?;
    let target_meta = symlink_metadata(src.parent().unwrap().join(&target))
        .map_err(|io| Error::from_io_error(io, ErrorKind::ReadSymlink, &target))?;
    if target_meta.file_type().is_dir() {
        symlink_dir(target, dest)
            .map_err(|io| Error::from_io_error(io, ErrorKind::CreateSymlink, dest))
    } else {
        symlink_file(target, dest)
            .map_err(|io| Error::from_io_error(io, ErrorKind::CreateSymlink, dest))
    }
}
