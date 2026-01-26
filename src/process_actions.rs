//! Process management actions (kill, affinity, priority, etc.)

use std::fs;
use std::io;
use std::process::Command;

/// Available signals for process management
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Term,  // SIGTERM (15) - Graceful termination
    Kill,  // SIGKILL (9) - Force kill
    Stop,  // SIGSTOP (19) - Pause process
    Cont,  // SIGCONT (18) - Resume process
}

impl Signal {
    fn number(&self) -> i32 {
        match self {
            Signal::Term => 15,
            Signal::Kill => 9,
            Signal::Stop => 19,
            Signal::Cont => 18,
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

    // Find the closing paren of comm field (which may contain spaces)
    let comm_end = content.find(')').ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "Invalid stat format")
    })?;

    let after_comm = &content[comm_end + 1..];
    let fields: Vec<&str> = after_comm.split_whitespace().collect();

    // nice is at index 16 (after comm and state)
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

/// Information about a thread's CPU assignment
#[derive(Debug, Clone)]
pub struct ThreadCpuInfo {
    pub tid: u32,
    #[allow(dead_code)] // Available for tooltip/expanded view
    pub name: String,
    pub current_cpu: Option<usize>,
}

/// Get CPU information for all threads of a process
/// Returns a list of threads with their current CPU assignments
pub fn get_thread_cpu_info(pid: u32) -> Vec<ThreadCpuInfo> {
    let task_dir = format!("/proc/{}/task", pid);
    let mut threads = Vec::new();

    if let Ok(entries) = fs::read_dir(&task_dir) {
        for entry in entries.flatten() {
            if let Ok(tid) = entry.file_name().to_string_lossy().parse::<u32>() {
                let stat_path = format!("/proc/{}/task/{}/stat", pid, tid);
                if let Ok(content) = fs::read_to_string(&stat_path) {
                    let (name, cpu) = parse_stat_for_cpu(&content);
                    threads.push(ThreadCpuInfo {
                        tid,
                        name,
                        current_cpu: cpu,
                    });
                }
            }
        }
    }

    // Sort by TID (main thread first)
    threads.sort_by_key(|t| t.tid);
    threads
}

/// Parse /proc/[pid]/stat or /proc/[pid]/task/[tid]/stat for CPU and name
/// Returns (comm, processor) where processor is field 39 (0-indexed: 38)
fn parse_stat_for_cpu(content: &str) -> (String, Option<usize>) {
    // Format: pid (comm) state ppid pgrp session tty_nr tpgid flags ...
    // The comm field can contain spaces and parentheses, so find it by parens
    let comm_start = content.find('(').unwrap_or(0);
    let comm_end = content.rfind(')').unwrap_or(content.len());

    let name = if comm_start < comm_end {
        content[comm_start + 1..comm_end].to_string()
    } else {
        "unknown".to_string()
    };

    // Fields after comm: state is index 0, then ppid(1), pgrp(2), ... processor(36)
    // processor is at position 38 counting from pid (0-indexed)
    // After the closing paren, we have: state ppid pgrp session tty_nr tpgid flags
    // minflt cminflt majflt cmajflt utime stime cutime cstime priority nice
    // num_threads itrealvalue starttime vsize rss rsslim startcode endcode
    // startstack kstkesp kstkeip signal blocked sigignore sigcatch wchan
    // nswap cnswap exit_signal processor ...
    let after_comm = &content[comm_end + 1..];
    let fields: Vec<&str> = after_comm.split_whitespace().collect();

    // processor is field index 36 after (state which is index 0)
    // state=0, ppid=1, pgrp=2, session=3, tty_nr=4, tpgid=5, flags=6,
    // minflt=7, cminflt=8, majflt=9, cmajflt=10, utime=11, stime=12,
    // cutime=13, cstime=14, priority=15, nice=16, num_threads=17,
    // itrealvalue=18, starttime=19, vsize=20, rss=21, rsslim=22,
    // startcode=23, endcode=24, startstack=25, kstkesp=26, kstkeip=27,
    // signal=28, blocked=29, sigignore=30, sigcatch=31, wchan=32,
    // nswap=33, cnswap=34, exit_signal=35, processor=36
    let cpu = fields.get(36).and_then(|s| s.parse().ok());

    (name, cpu)
}

/// CPU core type information
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreType {
    /// Intel Performance core
    PCore,
    /// Intel Efficiency core
    ECore,
    /// AMD X3D V-Cache core (large L3)
    X3D,
    /// Standard core (no special type detected)
    Standard,
}

impl CoreType {
    pub fn label(&self) -> &'static str {
        match self {
            CoreType::PCore => "P-Core",
            CoreType::ECore => "E-Core",
            CoreType::X3D => "X3D",
            CoreType::Standard => "",
        }
    }

    pub fn css_class(&self) -> Option<&'static str> {
        match self {
            CoreType::PCore => Some("accent"),
            CoreType::ECore => Some("dim-label"),
            CoreType::X3D => Some("success"),
            CoreType::Standard => None,
        }
    }
}

/// Information about a CPU core
#[derive(Debug, Clone)]
pub struct CpuCoreInfo {
    pub cpu_id: usize,
    pub core_type: CoreType,
    pub die_id: Option<usize>,
    #[allow(dead_code)] // Stored for potential future use (tooltips, detailed view)
    pub l3_cache_kb: Option<usize>,
}

/// Get detailed information about all CPU cores
pub fn get_cpu_core_info() -> Vec<CpuCoreInfo> {
    let cpu_count = get_cpu_count();
    let mut cores = Vec::with_capacity(cpu_count);

    // Try to detect Intel hybrid (P-core/E-core)
    let intel_core_types = detect_intel_hybrid_cores();

    // Try to detect AMD X3D cores
    let amd_x3d_cores = detect_amd_x3d_cores(cpu_count);

    for i in 0..cpu_count {
        let core_type = if let Some(ref types) = intel_core_types {
            types.get(i).cloned().unwrap_or(CoreType::Standard)
        } else if let Some(ref x3d) = amd_x3d_cores {
            if x3d.contains(&i) {
                CoreType::X3D
            } else {
                CoreType::Standard
            }
        } else {
            CoreType::Standard
        };

        let die_id = fs::read_to_string(format!("/sys/devices/system/cpu/cpu{}/topology/die_id", i))
            .ok()
            .and_then(|s| s.trim().parse().ok());

        let l3_cache_kb = fs::read_to_string(format!("/sys/devices/system/cpu/cpu{}/cache/index3/size", i))
            .ok()
            .and_then(|s| {
                let s = s.trim();
                if s.ends_with('K') {
                    s[..s.len()-1].parse().ok()
                } else if s.ends_with('M') {
                    s[..s.len()-1].parse::<usize>().ok().map(|m| m * 1024)
                } else {
                    s.parse().ok()
                }
            });

        cores.push(CpuCoreInfo {
            cpu_id: i,
            core_type,
            die_id,
            l3_cache_kb,
        });
    }

    cores
}

/// Detect Intel hybrid cores (P-core vs E-core)
/// Returns None if not an Intel hybrid CPU
fn detect_intel_hybrid_cores() -> Option<Vec<CoreType>> {
    let types_dir = std::path::Path::new("/sys/devices/system/cpu/types");
    if !types_dir.exists() {
        return None;
    }

    let cpu_count = get_cpu_count();
    let mut core_types = vec![CoreType::Standard; cpu_count];

    // Look for intel_pcore and intel_ecore directories
    if let Ok(entries) = fs::read_dir(types_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            let core_type = if name_str.contains("pcore") || name_str.contains("atom") == false && name_str.contains("intel") {
                // Check if it's specifically a performance core
                if name_str.to_lowercase().contains("pcore") || name_str.to_lowercase().contains("core") && !name_str.to_lowercase().contains("ecore") {
                    Some(CoreType::PCore)
                } else {
                    None
                }
            } else if name_str.to_lowercase().contains("ecore") || name_str.to_lowercase().contains("atom") {
                Some(CoreType::ECore)
            } else {
                None
            };

            if let Some(ct) = core_type {
                // Read the cpulist file
                let cpulist_path = entry.path().join("cpulist");
                if let Ok(cpulist) = fs::read_to_string(&cpulist_path) {
                    for cpu in parse_cpu_list(&cpulist) {
                        if cpu < cpu_count {
                            core_types[cpu] = ct.clone();
                        }
                    }
                }
            }
        }
    }

    // Check if we found any hybrid cores
    if core_types.iter().any(|t| *t != CoreType::Standard) {
        Some(core_types)
    } else {
        None
    }
}

/// Detect AMD X3D cores based on L3 cache size
/// X3D CCDs have significantly larger L3 cache (96MB vs 32MB per CCD)
fn detect_amd_x3d_cores(cpu_count: usize) -> Option<Vec<usize>> {
    // Check if this is an AMD CPU
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").ok()?;
    if !cpuinfo.contains("AMD") {
        return None;
    }

    // Group cores by their L3 cache size
    let mut cache_sizes: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();

    for i in 0..cpu_count {
        let cache_path = format!("/sys/devices/system/cpu/cpu{}/cache/index3/size", i);
        if let Ok(size_str) = fs::read_to_string(&cache_path) {
            let size_str = size_str.trim();
            let size_kb = if size_str.ends_with('K') {
                size_str[..size_str.len()-1].parse::<usize>().unwrap_or(0)
            } else if size_str.ends_with('M') {
                size_str[..size_str.len()-1].parse::<usize>().unwrap_or(0) * 1024
            } else {
                size_str.parse::<usize>().unwrap_or(0)
            };

            cache_sizes.entry(size_kb).or_default().push(i);
        }
    }

    // If there are different L3 cache sizes, the larger ones are likely X3D
    if cache_sizes.len() > 1 {
        let max_size = *cache_sizes.keys().max()?;
        // X3D cores typically have 96MB L3 cache per CCD (98304 KB)
        // Regular CCDs have ~32MB (32768 KB)
        // Consider cores with largest cache as X3D if it's significantly larger
        let min_size = *cache_sizes.keys().min()?;
        if max_size > min_size * 2 {
            return Some(cache_sizes.get(&max_size)?.clone());
        }
    }

    None
}

/// Parse a CPU list string like "0-3,8-11" into individual CPU numbers
fn parse_cpu_list(list: &str) -> Vec<usize> {
    let mut cpus = Vec::new();
    for part in list.trim().split(',') {
        let part = part.trim();
        if part.contains('-') {
            let mut range = part.split('-');
            if let (Some(start), Some(end)) = (range.next(), range.next()) {
                if let (Ok(start), Ok(end)) = (start.parse::<usize>(), end.parse::<usize>()) {
                    for i in start..=end {
                        cpus.push(i);
                    }
                }
            }
        } else if let Ok(cpu) = part.parse::<usize>() {
            cpus.push(cpu);
        }
    }
    cpus
}
