//! Physical device memory shared by global and server pack selection.

/// Reads physical RAM without using current free memory or process memory pressure.
pub fn physical_bytes() -> u64 {
    static BYTES: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *BYTES.get_or_init(|| {
        let result = read_physical_bytes();
        if result.is_none() {
            tracing::warn!(
                "physical memory unavailable; resource packs use the lowest automatic tier"
            );
        }
        result.unwrap_or(0)
    })
}

/// Logs how much memory iOS still lets this process allocate before it ends the app, which it
/// does without any crash report the app can write. A fall towards zero in the client log is
/// the only in-app record of such an exit, such as while a server's large packs load.
#[cfg(target_os = "ios")]
pub fn spawn_headroom_log() {
    /// Samples often enough to catch a pack load's climb before the exit.
    const PERIOD: std::time::Duration = std::time::Duration::from_secs(2);
    /// Headroom change that is worth a line, and the level below which every sample is.
    const STEP: u64 = 64 << 20;
    const LOW: u64 = 512 << 20;
    unsafe extern "C" {
        /// libSystem's remaining allocation budget before the process's memory limit.
        fn os_proc_available_memory() -> usize;
    }
    let spawned = std::thread::Builder::new()
        .name("memory-headroom".into())
        .spawn(|| {
            let mut logged: Option<u64> = None;
            let mut lowest = u64::MAX;
            loop {
                // SAFETY: the call takes no arguments and only reads the process's budget.
                let available = unsafe { os_proc_available_memory() } as u64;
                lowest = lowest.min(available);
                if logged.is_none_or(|last| last.abs_diff(available) >= STEP) || available < LOW {
                    let mib = |bytes: u64| bytes >> 20;
                    if available < LOW {
                        tracing::warn!(
                            available_mib = mib(available),
                            lowest_mib = mib(lowest),
                            "memory headroom low; iOS ends the app when it reaches zero"
                        );
                    } else {
                        tracing::info!(
                            available_mib = mib(available),
                            lowest_mib = mib(lowest),
                            "memory headroom"
                        );
                    }
                    logged = Some(available);
                }
                std::thread::sleep(PERIOD);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("could not start the memory headroom log: {error}");
    }
}

/// Reads the kernel's byte-valued `hw.memsize` property.
#[cfg(target_os = "macos")]
fn read_physical_bytes() -> Option<u64> {
    use std::ffi::{c_char, c_int, c_void};
    unsafe extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            old: *mut c_void,
            old_length: *mut usize,
            new: *mut c_void,
            new_length: usize,
        ) -> c_int;
    }
    let mut bytes = 0u64;
    let mut length = size_of::<u64>();
    // SAFETY: the name is NUL-terminated and `bytes` has the `length` the kernel may write.
    let result = unsafe {
        sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&raw mut bytes).cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    (result == 0 && length == size_of::<u64>()).then_some(bytes)
}

#[cfg(target_os = "linux")]
fn read_physical_bytes() -> Option<u64> {
    meminfo_total_bytes(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

/// `MemTotal` from `/proc/meminfo`, which the kernel reports in KiB.
#[cfg(any(target_os = "linux", test))]
fn meminfo_total_bytes(meminfo: &str) -> Option<u64> {
    let line = meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?;
    line.split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}

#[cfg(target_os = "windows")]
fn read_physical_bytes() -> Option<u64> {
    crate::desktop::windows::physical_memory()
}

/// Unsupported platforms retain the lowest automatic tier until a native reader exists.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn read_physical_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_total_is_read_in_kib() {
        let meminfo = "MemFree:  1000 kB\nMemTotal:       16318060 kB\nSwapTotal: 0 kB\n";
        assert_eq!(meminfo_total_bytes(meminfo), Some(16_318_060 * 1024));
        assert_eq!(meminfo_total_bytes("MemFree: 1 kB\n"), None);
        assert_eq!(meminfo_total_bytes("MemTotal: lots kB\n"), None);
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    #[test]
    fn native_reader_reports_installed_memory() {
        let bytes = read_physical_bytes().expect("physical memory");
        assert!(bytes >= 256 << 20, "{bytes}");
    }
}
