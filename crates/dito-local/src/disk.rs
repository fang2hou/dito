use std::path::Path;

/// Free bytes on the volume containing `path` (or its nearest existing ancestor).
pub(crate) fn free_space(path: &Path) -> u64 {
    let mut existing = path;
    while !existing.exists() {
        match existing.parent() {
            Some(parent) => existing = parent,
            None => return 0,
        }
    }
    let Ok(c_path) = std::ffi::CString::new(existing.as_os_str().as_encoded_bytes()) else { return 0 };
    // SAFETY: `statvfs` is a plain C struct of integers, for which the all-zero bit pattern is valid.
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is a valid NUL-terminated path and `stats` is a properly sized, writable `statvfs`.
    let status = unsafe { libc::statvfs(c_path.as_ptr(), &mut stats) };
    if status == 0 { stats.f_bavail as u64 * stats.f_frsize as u64 } else { 0 }
}

/// Bytes of every file under `path`; an unreadable entry counts as nothing.
pub(crate) fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => dir_size(&entry.path()),
            Ok(_) => entry.metadata().map_or(0, |meta| meta.len()),
            Err(_) => 0,
        })
        .sum()
}

pub(crate) fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
