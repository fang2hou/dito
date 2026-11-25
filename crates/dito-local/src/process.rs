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
        cpu: Duration::from_nanos(ticks_to_nanos(info.ri_user_time.saturating_add(info.ri_system_time))),
        memory_bytes: info.ri_phys_footprint,
    }
}

/// `rusage_info` times are Mach absolute time units, not nanoseconds: 125/3 ns per tick on Apple Silicon, 1 on Intel.
fn ticks_to_nanos(ticks: u64) -> u64 {
    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    unsafe extern "C" {
        fn mach_timebase_info(info: *mut Timebase) -> i32;
    }
    static TIMEBASE: std::sync::OnceLock<(u32, u32)> = std::sync::OnceLock::new();
    let (numer, denom) = *TIMEBASE.get_or_init(|| {
        let mut info = Timebase { numer: 0, denom: 0 };
        // SAFETY: `info` matches `mach_timebase_info_data_t` and is writable for the duration of the call.
        let status = unsafe { mach_timebase_info(&raw mut info) };
        if status == 0 && info.denom != 0 { (info.numer, info.denom) } else { (1, 1) }
    });
    scale(ticks, numer, denom)
}

fn scale(ticks: u64, numer: u32, denom: u32) -> u64 {
    u64::try_from(u128::from(ticks) * u128::from(numer) / u128::from(denom)).unwrap_or(u64::MAX)
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

#[cfg(test)]
mod tests {
    #[test]
    fn apple_silicon_ticks_become_nanoseconds() {
        assert_eq!(super::scale(24_000_000, 125, 3), 1_000_000_000);
        assert_eq!(super::scale(5, 1, 1), 5);
    }

    #[test]
    fn the_cpu_time_read_by_pid_matches_the_one_read_by_the_process_itself() {
        let mut x = 0u64;
        while super::cpu_time() < std::time::Duration::from_millis(200) {
            x = std::hint::black_box(x.wrapping_mul(31).wrapping_add(7));
        }
        let own = super::cpu_time().as_secs_f64();
        let by_pid = super::process_usage_for(std::process::id()).cpu.as_secs_f64();
        assert!((0.5..2.0).contains(&(by_pid / own)), "by pid {by_pid} s, own {own} s");
    }
}
