/// Installed RAM in bytes (0 when unknown).
pub fn physical_memory_bytes() -> u64 {
    let mut mem: u64 = 0;
    let mut len = size_of::<u64>();
    // SAFETY: the name is NUL-terminated and `mem`/`len` describe a valid u64 buffer.
    let rc = unsafe {
        libc::sysctlbyname(c"hw.memsize".as_ptr(), (&mut mem as *mut u64).cast(), &mut len, std::ptr::null_mut(), 0)
    };
    if rc == 0 { mem } else { 0 }
}

/// Seconds east of UTC for the local time zone, from `localtime_r` (safe to call from any thread).
pub fn local_utc_offset_secs() -> i32 {
    // SAFETY: a null argument asks `time` only to return the current time.
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    // SAFETY: `libc::tm` is plain data for which all-zero bytes are valid.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: `now` and `tm` are valid for the duration of the call, and `localtime_r` is re-entrant.
    if unsafe { libc::localtime_r(&now, &mut tm) }.is_null() { 0 } else { tm.tm_gmtoff as i32 }
}
