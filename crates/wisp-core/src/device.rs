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

/// This computer's memory in megabytes, if the OS says.
pub fn total_memory_mb() -> Option<u64> {
    let output = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
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

#[cfg(test)]
mod tests {
    #[test]
    fn knows_this_computers_memory() {
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            assert!(super::total_memory_mb().is_some_and(|mb| mb >= 1024));
        }
    }
}
