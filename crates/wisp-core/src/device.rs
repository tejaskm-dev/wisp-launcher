use serde::Serialize;

/// The computer Wisp is running on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// The name people know this computer by, e.g. "Sam's MacBook Pro".
    pub name: String,
    /// "macOS", "Windows" or "Linux".
    pub os: String,
}

impl DeviceInfo {
    pub fn current() -> Self {
        let name = whoami::devicename()
            .ok()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "This computer".to_string());
        let os = match std::env::consts::OS {
            "macos" => "macOS",
            "windows" => "Windows",
            "linux" => "Linux",
            other => other,
        };
        Self {
            name,
            os: os.to_string(),
        }
    }
}

/// A program to run for its output, without a console window flashing up on Windows (where
/// Wisp's own window has no console, so each console program would otherwise open one).
pub(crate) fn quiet_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// This computer's memory in megabytes, if the OS says. Asked once: it doesn't change.
pub fn total_memory_mb() -> Option<u64> {
    static TOTAL: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *TOTAL.get_or_init(ask_total_memory_mb)
}

fn ask_total_memory_mb() -> Option<u64> {
    let output = |program: &str, args: &[&str]| {
        quiet_command(program)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let bytes: u64 = match std::env::consts::OS {
        "macos" => output("/usr/sbin/sysctl", &["-n", "hw.memsize"])?
            .parse()
            .ok()?,
        "linux" => {
            let info = std::fs::read_to_string("/proc/meminfo").ok()?;
            let kilobytes: u64 = info
                .lines()
                .find_map(|line| line.strip_prefix("MemTotal:"))?
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse()
                .ok()?;
            kilobytes * 1024
        }
        "windows" => output(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ],
        )?
        .parse()
        .ok()?,
        _ => return None,
    };
    Some(bytes / (1024 * 1024))
}

/// Free space on the drive holding `path`, in bytes, if the OS says. Asked of the system's own
/// tools and kept for a few minutes, since it's wanted on every sync.
pub fn free_space(path: &std::path::Path) -> Option<u64> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static LAST: Mutex<Option<(std::path::PathBuf, Instant, Option<u64>)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((asked, at, free)) = last.as_ref()
        && asked == path
        && at.elapsed() < Duration::from_secs(300)
    {
        return *free;
    }
    let free = ask_free_space(path);
    *last = Some((path.to_path_buf(), Instant::now(), free));
    free
}

fn ask_free_space(path: &std::path::Path) -> Option<u64> {
    let run = |program: &str, args: &[&std::ffi::OsStr]| {
        quiet_command(program)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    if cfg!(windows) {
        // The drive letter's free bytes.
        let drive = path.to_string_lossy().chars().next().filter(char::is_ascii_alphabetic)?;
        let command = format!("(Get-PSDrive {drive}).Free");
        let out = run("powershell", &["-NoProfile".as_ref(), "-NonInteractive".as_ref(), "-Command".as_ref(), command.as_ref()])?;
        return out.trim().parse().ok();
    }
    // POSIX df: the second line's fourth column is the space available, in 1K blocks.
    let out = run("df", &["-Pk".as_ref(), path.as_os_str()])?;
    let kilobytes: u64 = out.lines().nth(1)?.split_whitespace().nth(3)?.parse().ok()?;
    Some(kilobytes * 1024)
}

#[cfg(test)]
mod tests {
    #[test]
    fn knows_this_computers_memory() {
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            assert!(super::total_memory_mb().is_some_and(|mb| mb >= 1024));
        }
    }
}
