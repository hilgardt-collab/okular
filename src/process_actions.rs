//! Process management actions (kill, affinity, priority, etc.)

use std::fs;
use std::io;
use std::process::Command;

/// Available signals for process termination
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Term,  // SIGTERM (15) - Graceful termination
    Kill,  // SIGKILL (9) - Force kill
    Stop,  // SIGSTOP (19) - Pause process
    Cont,  // SIGCONT (18) - Resume process
    Hup,   // SIGHUP (1) - Hangup
}

impl Signal {
    pub fn as_str(&self) -> &'static str {
        match self {
            Signal::Term => "TERM",
            Signal::Kill => "KILL",
            Signal::Stop => "STOP",
            Signal::Cont => "CONT",
            Signal::Hup => "HUP",
        }
    }

    pub fn number(&self) -> i32 {
        match self {
            Signal::Term => 15,
            Signal::Kill => 9,
            Signal::Stop => 19,
            Signal::Cont => 18,
            Signal::Hup => 1,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Signal::Term => "Request graceful termination",
            Signal::Kill => "Force kill immediately",
            Signal::Stop => "Pause the process",
            Signal::Cont => "Resume paused process",
            Signal::Hup => "Hangup signal",
        }
    }
}

/// Send a signal to a process
pub fn send_signal(pid: u32, signal: Signal) -> io::Result<()> {
    let output = Command::new("kill")
        .arg(format!("-{}", signal.number()))
        .arg(pid.to_string())
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("Failed to send signal: {}", stderr.trim()),
        ))
    }
}

/// Kill a process (SIGTERM first, then SIGKILL if force is true)
pub fn kill_process(pid: u32, force: bool) -> io::Result<()> {
    if force {
        send_signal(pid, Signal::Kill)
    } else {
        send_signal(pid, Signal::Term)
    }
}

/// Get the number of CPU cores
pub fn get_cpu_count() -> usize {
    // Read from /proc/cpuinfo or use a simpler method
    if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
        content.matches("processor").count().max(1)
    } else {
        // Fallback: try sysconf
        1
    }
}

/// Get current CPU affinity for a process
/// Returns a bitmask where bit N means CPU N is allowed
pub fn get_cpu_affinity(pid: u32) -> io::Result<Vec<bool>> {
    let output = Command::new("taskset")
        .arg("-p")
        .arg(pid.to_string())
        .output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to get CPU affinity",
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Output format: "pid XXXX's current affinity mask: ff"
    if let Some(mask_str) = stdout.split(':').last() {
        let mask_str = mask_str.trim();
        if let Ok(mask) = u64::from_str_radix(mask_str, 16) {
            let cpu_count = get_cpu_count();
            let mut affinity = Vec::with_capacity(cpu_count);
            for i in 0..cpu_count {
                affinity.push((mask & (1 << i)) != 0);
            }
            return Ok(affinity);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "Failed to parse CPU affinity",
    ))
}

/// Set CPU affinity for a process
/// cpus is a list of CPU indices (0-based)
pub fn set_cpu_affinity(pid: u32, cpus: &[usize]) -> io::Result<()> {
    if cpus.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Must select at least one CPU",
        ));
    }

    // Build CPU list string (e.g., "0,2,3")
    let cpu_list: String = cpus
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let output = Command::new("taskset")
        .arg("-pc")
        .arg(&cpu_list)
        .arg(pid.to_string())
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("Failed to set CPU affinity: {}", stderr.trim()),
        ))
    }
}

/// Priority levels (nice values)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    VeryHigh,  // -20 (requires root)
    High,      // -10 (requires root)
    Normal,    // 0
    Low,       // 10
    VeryLow,   // 19
}

impl Priority {
    pub fn nice_value(&self) -> i32 {
        match self {
            Priority::VeryHigh => -20,
            Priority::High => -10,
            Priority::Normal => 0,
            Priority::Low => 10,
            Priority::VeryLow => 19,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::VeryHigh => "Very High (-20)",
            Priority::High => "High (-10)",
            Priority::Normal => "Normal (0)",
            Priority::Low => "Low (10)",
            Priority::VeryLow => "Very Low (19)",
        }
    }

    pub fn all() -> &'static [Priority] {
        &[
            Priority::VeryHigh,
            Priority::High,
            Priority::Normal,
            Priority::Low,
            Priority::VeryLow,
        ]
    }
}

/// Get current priority (nice value) for a process
pub fn get_priority(pid: u32) -> io::Result<i32> {
    let stat_path = format!("/proc/{}/stat", pid);
    let content = fs::read_to_string(&stat_path)?;

    // Parse stat file - nice value is field 19 (0-indexed 18)
    // Format: pid (comm) state ppid pgrp session tty_nr tpgid flags minflt cminflt majflt cmajflt
    //         utime stime cutime cstime priority nice ...
    let _parts: Vec<&str> = content.split_whitespace().collect();

    // Find the closing paren of comm field (which may contain spaces)
    let comm_end = content.find(')').ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "Invalid stat format")
    })?;

    let after_comm = &content[comm_end + 1..];
    let fields: Vec<&str> = after_comm.split_whitespace().collect();

    // nice is at index 17 (after comm and state)
    // state=0, ppid=1, pgrp=2, session=3, tty_nr=4, tpgid=5, flags=6,
    // minflt=7, cminflt=8, majflt=9, cmajflt=10, utime=11, stime=12,
    // cutime=13, cstime=14, priority=15, nice=16
    if fields.len() > 16 {
        fields[16]
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid nice value"))
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Stat file too short",
        ))
    }
}

/// Set priority (nice value) for a process
pub fn set_priority(pid: u32, priority: Priority) -> io::Result<()> {
    let nice_value = priority.nice_value();

    let output = Command::new("renice")
        .arg("-n")
        .arg(nice_value.to_string())
        .arg("-p")
        .arg(pid.to_string())
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("Failed to set priority: {}", stderr.trim()),
        ))
    }
}

/// Get the command line for a process
pub fn get_command_line(pid: u32) -> Option<String> {
    let cmdline_path = format!("/proc/{}/cmdline", pid);
    fs::read_to_string(&cmdline_path)
        .ok()
        .map(|s| s.replace('\0', " ").trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Check if a process is still running
pub fn is_process_running(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
}

/// Get the executable path for a process
pub fn get_executable_path(pid: u32) -> Option<String> {
    let exe_path = format!("/proc/{}/exe", pid);
    fs::read_link(&exe_path)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}
