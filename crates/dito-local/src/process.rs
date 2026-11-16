//! CPU time and memory of this process, read from the kernel.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessUsage {
    /// User and system time of every thread since launch.
    pub cpu: Duration,
    /// The physical footprint, the figure Activity Monitor shows as Memory.
    pub memory_bytes: u64,
}

pub fn process_usage() -> ProcessUsage {
    ProcessUsage { cpu: cpu_time(), memory_bytes: footprint_bytes() }
}

pub fn process_usage_for(pid: u32) -> ProcessUsage {
    // SAFETY: the V2 layout is plain data and proc_pid_rusage writes only to this buffer.
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    // SAFETY: the buffer matches RUSAGE_INFO_V2; an exited or inaccessible PID returns an error.
    let status = unsafe { libc::proc_pid_rusage(pid as i32, libc::RUSAGE_INFO_V2, (&raw mut info).cast()) };
    if status != 0 {
        return ProcessUsage::default();
    }
    ProcessUsage {
        cpu: Duration::from_nanos(info.ri_user_time.saturating_add(info.ri_system_time)),
        memory_bytes: info.ri_phys_footprint,
    }
}

pub(crate) fn cpu_time() -> Duration {
    // SAFETY: `libc::rusage` is plain data for which all-zero bytes are valid.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: `usage` is a valid, writable `rusage` for the duration of the call.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return Duration::ZERO;
    }
    let time = |t: libc::timeval| Duration::new(t.tv_sec as u64, t.tv_usec as u32 * 1000);
    time(usage.ru_utime) + time(usage.ru_stime)
}

pub(crate) fn footprint_bytes() -> u64 {
    // SAFETY: `libc::rusage_info_v2` is plain data for which all-zero bytes are valid.
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    // SAFETY: the buffer is a writable `rusage_info_v2`, the layout the V2 flavor fills.
    let status = unsafe { libc::proc_pid_rusage(libc::getpid(), libc::RUSAGE_INFO_V2, (&raw mut info).cast()) };
    if status == 0 { info.ri_phys_footprint } else { 0 }
}
