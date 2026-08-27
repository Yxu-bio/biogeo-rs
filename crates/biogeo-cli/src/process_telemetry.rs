use std::time::Duration;

const WINDOWS_TICKS_PER_SECOND: f64 = 10_000_000.0;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CpuTimes {
    user_ticks: u64,
    kernel_ticks: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessTelemetryStart {
    cpu_times: Option<CpuTimes>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcessTelemetry {
    pub provider: &'static str,
    pub available: bool,
    pub peak_working_set_bytes: Option<u64>,
    pub cpu_user_seconds: Option<f64>,
    pub cpu_kernel_seconds: Option<f64>,
    pub cpu_total_seconds: Option<f64>,
    pub average_logical_cores_used: Option<f64>,
}

impl ProcessTelemetryStart {
    pub fn capture() -> Self {
        Self {
            cpu_times: query_cpu_times(),
        }
    }

    pub fn finish(self, elapsed: Duration) -> ProcessTelemetry {
        let peak_working_set_bytes = query_peak_working_set_bytes();
        let cpu_delta = self
            .cpu_times
            .zip(query_cpu_times())
            .and_then(|(start, end)| cpu_time_delta(start, end));
        let (cpu_user_seconds, cpu_kernel_seconds, cpu_total_seconds) = cpu_delta
            .map_or((None, None, None), |(user, kernel)| {
                (Some(user), Some(kernel), Some(user + kernel))
            });
        let average_logical_cores_used = cpu_total_seconds.and_then(|total| {
            let elapsed = elapsed.as_secs_f64();
            (elapsed > 0.0).then_some(total / elapsed)
        });

        ProcessTelemetry {
            provider: telemetry_provider(),
            available: peak_working_set_bytes.is_some() && cpu_delta.is_some(),
            peak_working_set_bytes,
            cpu_user_seconds,
            cpu_kernel_seconds,
            cpu_total_seconds,
            average_logical_cores_used,
        }
    }
}

fn cpu_time_delta(start: CpuTimes, end: CpuTimes) -> Option<(f64, f64)> {
    let user_ticks = end.user_ticks.checked_sub(start.user_ticks)?;
    let kernel_ticks = end.kernel_ticks.checked_sub(start.kernel_ticks)?;
    Some((
        user_ticks as f64 / WINDOWS_TICKS_PER_SECOND,
        kernel_ticks as f64 / WINDOWS_TICKS_PER_SECOND,
    ))
}

#[cfg(windows)]
fn telemetry_provider() -> &'static str {
    "windows_process_api"
}

#[cfg(not(windows))]
fn telemetry_provider() -> &'static str {
    "unavailable"
}

#[cfg(windows)]
fn query_peak_working_set_bytes() -> Option<u64> {
    use std::mem::size_of;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let counter_size = u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS>()).ok()?;
    counters.cb = counter_size;
    let succeeded =
        unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counter_size) };
    (succeeded != 0).then_some(counters.PeakWorkingSetSize as u64)
}

#[cfg(not(windows))]
fn query_peak_working_set_bytes() -> Option<u64> {
    None
}

#[cfg(windows)]
fn query_cpu_times() -> Option<CpuTimes> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let succeeded = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    (succeeded != 0).then(|| CpuTimes {
        user_ticks: filetime_ticks(user),
        kernel_ticks: filetime_ticks(kernel),
    })
}

#[cfg(not(windows))]
fn query_cpu_times() -> Option<CpuTimes> {
    None
}

#[cfg(windows)]
fn filetime_ticks(value: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_delta_uses_one_hundred_nanosecond_windows_ticks() {
        let delta = cpu_time_delta(
            CpuTimes {
                user_ticks: 5_000_000,
                kernel_ticks: 10_000_000,
            },
            CpuTimes {
                user_ticks: 25_000_000,
                kernel_ticks: 15_000_000,
            },
        )
        .unwrap();
        assert_eq!(delta, (2.0, 0.5));
    }

    #[test]
    fn cpu_delta_rejects_counter_regression() {
        assert_eq!(
            cpu_time_delta(
                CpuTimes {
                    user_ticks: 2,
                    kernel_ticks: 1,
                },
                CpuTimes {
                    user_ticks: 1,
                    kernel_ticks: 2,
                },
            ),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_process_queries_return_real_values() {
        let start = ProcessTelemetryStart::capture();
        let telemetry = start.finish(Duration::from_millis(1));
        assert_eq!(telemetry.provider, "windows_process_api");
        assert!(telemetry.available);
        assert!(
            telemetry
                .peak_working_set_bytes
                .is_some_and(|value| value > 0)
        );
        assert!(
            telemetry
                .cpu_total_seconds
                .is_some_and(|value| value >= 0.0)
        );
    }
}
