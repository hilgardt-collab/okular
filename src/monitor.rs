use sysinfo::{System, ProcessesToUpdate, ProcessRefreshKind};
use std::collections::{HashMap, VecDeque};
use std::fs;

/// Read the Thread Group ID (TGID) from /proc/<pid>/status
/// Returns None if the file cannot be read or parsed
fn read_tgid(pid: u32) -> Option<u32> {
    let status_path = format!("/proc/{}/status", pid);
    let content = fs::read_to_string(status_path).ok()?;

    for line in content.lines() {
        if let Some(tgid_str) = line.strip_prefix("Tgid:") {
            return tgid_str.trim().parse().ok();
        }
    }
    None
}

/// Read total network bytes (rx, tx) from /proc/net/dev
/// Sums all non-loopback interfaces
fn read_network_totals() -> (u64, u64) {
    let mut rx_total = 0u64;
    let mut tx_total = 0u64;

    if let Ok(content) = fs::read_to_string("/proc/net/dev") {
        for line in content.lines().skip(2) {
            // Format: "iface: rx_bytes rx_packets ... tx_bytes tx_packets ..."
            let line = line.trim();
            if line.starts_with("lo:") {
                continue; // Skip loopback
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                // First field is "iface:" or "iface" depending on spacing
                // rx_bytes is at index 1, tx_bytes is at index 9
                let iface_and_rx: Vec<&str> = parts[0].split(':').collect();
                let rx_idx = if iface_and_rx.len() > 1 && !iface_and_rx[1].is_empty() {
                    // "iface:12345" format - rx is part of first field
                    if let Ok(rx) = iface_and_rx[1].parse::<u64>() {
                        rx_total += rx;
                    }
                    0 // tx_bytes is at parts[8]
                } else {
                    // "iface: 12345" format - rx is at parts[1]
                    if let Ok(rx) = parts[1].parse::<u64>() {
                        rx_total += rx;
                    }
                    1 // tx_bytes is at parts[9]
                };
                let tx_idx = rx_idx + 8;
                if let Some(tx_str) = parts.get(tx_idx) {
                    if let Ok(tx) = tx_str.parse::<u64>() {
                        tx_total += tx;
                    }
                }
            }
        }
    }

    (rx_total, tx_total)
}

/// Represents a single process with its resource usage
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub gpu_percent: Option<f32>,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    /// Child processes/threads
    pub children: Vec<ProcessInfo>,
    /// Whether this is a group (has children aggregated)
    pub is_group: bool,
}

impl ProcessInfo {
    /// Get total CPU including children
    pub fn total_cpu(&self) -> f32 {
        self.cpu_percent + self.children.iter().map(|c| c.cpu_percent).sum::<f32>()
    }

    /// Get total memory - for groups (threads), just use parent's memory since threads share memory space
    pub fn total_memory(&self) -> u64 {
        // Threads share the same memory space, so don't sum children's memory
        self.memory_bytes
    }

    /// Get total disk read including children
    pub fn total_disk_read(&self) -> u64 {
        self.disk_read_bytes + self.children.iter().map(|c| c.disk_read_bytes).sum::<u64>()
    }

    /// Get total disk write including children
    pub fn total_disk_write(&self) -> u64 {
        self.disk_write_bytes + self.children.iter().map(|c| c.disk_write_bytes).sum::<u64>()
    }

    /// Get total disk I/O including children
    pub fn total_disk_io(&self) -> u64 {
        self.total_disk_read() + self.total_disk_write()
    }

    /// Get total GPU percent (max of self and children)
    pub fn total_gpu(&self) -> f32 {
        let self_gpu = self.gpu_percent.unwrap_or(0.0);
        let children_max = self.children.iter()
            .filter_map(|c| c.gpu_percent)
            .fold(0.0_f32, f32::max);
        self_gpu.max(children_max)
    }

    /// Get total network RX including children
    pub fn total_net_rx(&self) -> u64 {
        self.net_rx_bytes + self.children.iter().map(|c| c.net_rx_bytes).sum::<u64>()
    }

    /// Get total network TX including children
    pub fn total_net_tx(&self) -> u64 {
        self.net_tx_bytes + self.children.iter().map(|c| c.net_tx_bytes).sum::<u64>()
    }

    /// Get child count
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

/// History entry for graphing
#[derive(Debug, Clone, Default)]
pub struct ProcessHistory {
    pub cpu_history: VecDeque<f32>,
    pub memory_history: VecDeque<u64>,
    pub disk_read_history: VecDeque<u64>,
    pub disk_write_history: VecDeque<u64>,
    pub gpu_mem_history: VecDeque<f32>,    // Per-process GPU memory %
    pub gpu_util_history: VecDeque<f32>,   // System-wide GPU utilization %
    pub net_rx_history: VecDeque<u64>,
    pub net_tx_history: VecDeque<u64>,
}

impl ProcessHistory {
    pub fn add_sample(
        &mut self,
        cpu: f32,
        memory: u64,
        disk_read: u64,
        disk_write: u64,
        gpu_mem: f32,
        gpu_util: f32,
        net_rx: u64,
        net_tx: u64,
        max_samples: usize,
    ) {
        self.cpu_history.push_back(cpu);
        self.memory_history.push_back(memory);
        self.disk_read_history.push_back(disk_read);
        self.disk_write_history.push_back(disk_write);
        self.gpu_mem_history.push_back(gpu_mem);
        self.gpu_util_history.push_back(gpu_util);
        self.net_rx_history.push_back(net_rx);
        self.net_tx_history.push_back(net_tx);

        // Keep only the last max_samples - O(1) pop_front instead of O(n) remove(0)
        while self.cpu_history.len() > max_samples {
            self.cpu_history.pop_front();
        }
        while self.memory_history.len() > max_samples {
            self.memory_history.pop_front();
        }
        while self.disk_read_history.len() > max_samples {
            self.disk_read_history.pop_front();
        }
        while self.disk_write_history.len() > max_samples {
            self.disk_write_history.pop_front();
        }
        while self.gpu_mem_history.len() > max_samples {
            self.gpu_mem_history.pop_front();
        }
        while self.gpu_util_history.len() > max_samples {
            self.gpu_util_history.pop_front();
        }
        while self.net_rx_history.len() > max_samples {
            self.net_rx_history.pop_front();
        }
        while self.net_tx_history.len() > max_samples {
            self.net_tx_history.pop_front();
        }
    }

    /// Trim history to new max samples
    pub fn trim_to(&mut self, max_samples: usize) {
        while self.cpu_history.len() > max_samples {
            self.cpu_history.pop_front();
        }
        while self.memory_history.len() > max_samples {
            self.memory_history.pop_front();
        }
        while self.disk_read_history.len() > max_samples {
            self.disk_read_history.pop_front();
        }
        while self.disk_write_history.len() > max_samples {
            self.disk_write_history.pop_front();
        }
        while self.gpu_mem_history.len() > max_samples {
            self.gpu_mem_history.pop_front();
        }
        while self.gpu_util_history.len() > max_samples {
            self.gpu_util_history.pop_front();
        }
        while self.net_rx_history.len() > max_samples {
            self.net_rx_history.pop_front();
        }
        while self.net_tx_history.len() > max_samples {
            self.net_tx_history.pop_front();
        }
    }
}

/// System monitor that collects process information
pub struct SystemMonitor {
    system: System,
    process_history: HashMap<u32, ProcessHistory>,
    nvml: Option<nvml_wrapper::Nvml>,
    cpu_count: usize,
    max_samples: usize,
    // Network tracking (system-wide rates)
    last_net_rx: u64,
    last_net_tx: u64,
    net_rx_rate: u64,
    net_tx_rate: u64,
    // GPU utilization (system-wide)
    gpu_utilization: f32,
}

impl SystemMonitor {
    pub fn new() -> Self {
        // Try to initialize NVML for GPU monitoring
        let nvml = nvml_wrapper::Nvml::init().ok();
        if nvml.is_some() {
            eprintln!("NVIDIA GPU monitoring enabled");
        }

        let mut system = System::new();

        // Get CPU count for normalization
        system.refresh_cpu_all();
        let cpu_count = system.cpus().len().max(1);

        // Initial refresh to populate CPU usage (needs two samples)
        let refresh_kind = ProcessRefreshKind::new()
            .with_cpu()
            .with_memory()
            .with_disk_usage();
        system.refresh_processes_specifics(ProcessesToUpdate::All, refresh_kind);

        // Initialize network tracking
        let (net_rx, net_tx) = read_network_totals();

        Self {
            system,
            process_history: HashMap::new(),
            nvml,
            cpu_count,
            max_samples: 60, // Default: 2 minutes at 2-second intervals
            last_net_rx: net_rx,
            last_net_tx: net_tx,
            net_rx_rate: 0,
            net_tx_rate: 0,
            gpu_utilization: 0.0,
        }
    }

    /// Set the maximum number of history samples to keep
    pub fn set_max_samples(&mut self, max_samples: usize) {
        self.max_samples = max_samples;
        // Trim existing histories
        for history in self.process_history.values_mut() {
            history.trim_to(max_samples);
        }
    }

    /// Get current max samples setting
    #[allow(dead_code)]
    pub fn max_samples(&self) -> usize {
        self.max_samples
    }

    /// Get CPU count
    #[allow(dead_code)]
    pub fn cpu_count(&self) -> usize {
        self.cpu_count
    }

    /// Get current network RX rate (bytes per refresh interval)
    #[allow(dead_code)]
    pub fn net_rx_rate(&self) -> u64 {
        self.net_rx_rate
    }

    /// Get current network TX rate (bytes per refresh interval)
    #[allow(dead_code)]
    pub fn net_tx_rate(&self) -> u64 {
        self.net_tx_rate
    }

    /// Get current GPU utilization (system-wide, percentage)
    #[allow(dead_code)]
    pub fn gpu_utilization(&self) -> f32 {
        self.gpu_utilization
    }

    /// Refresh process data and return top 150 processes by CPU usage, grouped by TGID
    pub fn refresh(&mut self) -> Vec<ProcessInfo> {
        let refresh_kind = ProcessRefreshKind::new()
            .with_cpu()
            .with_memory()
            .with_disk_usage();
        self.system.refresh_processes_specifics(ProcessesToUpdate::All, refresh_kind);

        // Update network rates (system-wide)
        let (net_rx, net_tx) = read_network_totals();
        self.net_rx_rate = net_rx.saturating_sub(self.last_net_rx);
        self.net_tx_rate = net_tx.saturating_sub(self.last_net_tx);
        self.last_net_rx = net_rx;
        self.last_net_tx = net_tx;

        // Update GPU utilization (system-wide)
        self.gpu_utilization = self.get_gpu_utilization();

        // Get GPU memory usage per process if available
        let gpu_usage = self.get_gpu_process_usage();

        // Normalize CPU by dividing by CPU count
        let cpu_divisor = self.cpu_count as f32;

        // First pass: collect all processes with their TGID
        // TGID (Thread Group ID) identifies which thread group a process belongs to
        // - If PID == TGID: this is the thread group leader (main process)
        // - If PID != TGID: this is a thread belonging to the group with that TGID
        let mut all_processes: HashMap<u32, (ProcessInfo, Option<u32>)> = HashMap::new();

        for (pid, proc) in self.system.processes() {
            let pid_u32 = pid.as_u32();
            let tgid = read_tgid(pid_u32);
            let normalized_cpu = proc.cpu_usage() / cpu_divisor;

            let info = ProcessInfo {
                pid: pid_u32,
                name: proc.name().to_string_lossy().to_string(),
                cpu_percent: normalized_cpu,
                memory_bytes: proc.memory(),
                disk_read_bytes: proc.disk_usage().read_bytes,
                disk_write_bytes: proc.disk_usage().written_bytes,
                gpu_percent: gpu_usage.get(&pid_u32).copied(),
                // Per-process network stats require eBPF or netfilter accounting
                // For now, we track system-wide rates in the monitor
                net_rx_bytes: 0,
                net_tx_bytes: 0,
                children: Vec::new(),
                is_group: false,
            };

            all_processes.insert(pid_u32, (info, tgid));
        }

        // Second pass: identify threads (PID != TGID) and group leaders (PID == TGID)
        let mut thread_group_leaders: HashMap<u32, ProcessInfo> = HashMap::new();
        let mut threads_by_tgid: HashMap<u32, Vec<ProcessInfo>> = HashMap::new();

        for (pid, (proc_info, tgid)) in all_processes {
            match tgid {
                Some(tgid) if tgid != pid => {
                    // This is a thread (PID != TGID), group it under its TGID
                    threads_by_tgid
                        .entry(tgid)
                        .or_default()
                        .push(proc_info);
                }
                _ => {
                    // This is a thread group leader (PID == TGID) or TGID unknown
                    thread_group_leaders.insert(pid, proc_info);
                }
            }
        }

        // Third pass: attach threads to their group leaders
        for (tgid, threads) in threads_by_tgid {
            if let Some(leader) = thread_group_leaders.get_mut(&tgid) {
                leader.is_group = true;
                leader.children = threads;
            }
            // If the leader doesn't exist (rare race condition), threads are dropped
        }

        // Convert to vec and sort by total CPU usage
        let mut processes: Vec<ProcessInfo> = thread_group_leaders.into_values().collect();
        processes.sort_by(|a, b| {
            b.total_cpu().partial_cmp(&a.total_cpu()).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take top 150
        processes.truncate(150);

        // Update history for tracked processes (use total values for groups)
        let max_samples = self.max_samples;
        let net_rx = self.net_rx_rate;
        let net_tx = self.net_tx_rate;
        let gpu_util = self.gpu_utilization;
        for proc in &processes {
            let history = self.process_history.entry(proc.pid).or_default();
            history.add_sample(
                proc.total_cpu(),
                proc.total_memory(),
                proc.total_disk_read(),
                proc.total_disk_write(),
                proc.total_gpu(),    // Per-process GPU memory
                gpu_util,            // System-wide GPU utilization
                net_rx,              // System-wide network
                net_tx,
                max_samples,
            );
        }

        // Clean up history for processes that no longer exist
        let current_pids: std::collections::HashSet<u32> = processes.iter().map(|p| p.pid).collect();
        self.process_history.retain(|pid, _| current_pids.contains(pid));

        processes
    }

    /// Get history for a specific process
    pub fn get_history(&self, pid: u32) -> Option<&ProcessHistory> {
        self.process_history.get(&pid)
    }

    /// Get GPU usage per process (NVIDIA only)
    fn get_gpu_process_usage(&self) -> HashMap<u32, f32> {
        use nvml_wrapper::enums::device::UsedGpuMemory;

        let mut usage = HashMap::new();

        if let Some(ref nvml) = self.nvml {
            // Try to get device count
            if let Ok(device_count) = nvml.device_count() {
                for i in 0..device_count {
                    if let Ok(device) = nvml.device_by_index(i) {
                        // Get running compute processes
                        if let Ok(processes) = device.running_compute_processes() {
                            for proc in processes {
                                if let Ok(mem_info) = device.memory_info() {
                                    if mem_info.total > 0 {
                                        let mem_used = match proc.used_gpu_memory {
                                            UsedGpuMemory::Used(bytes) => bytes,
                                            UsedGpuMemory::Unavailable => 0,
                                        };
                                        let percent = (mem_used as f32 / mem_info.total as f32) * 100.0;
                                        usage.insert(proc.pid, percent);
                                    }
                                }
                            }
                        }
                        // Also check graphics processes - take max of compute and graphics usage
                        if let Ok(processes) = device.running_graphics_processes() {
                            for proc in processes {
                                if let Ok(mem_info) = device.memory_info() {
                                    if mem_info.total > 0 {
                                        let mem_used = match proc.used_gpu_memory {
                                            UsedGpuMemory::Used(bytes) => bytes,
                                            UsedGpuMemory::Unavailable => 0,
                                        };
                                        let percent = (mem_used as f32 / mem_info.total as f32) * 100.0;
                                        usage
                                            .entry(proc.pid)
                                            .and_modify(|existing| *existing = existing.max(percent))
                                            .or_insert(percent);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        usage
    }

    /// Get overall GPU utilization (NVIDIA only)
    fn get_gpu_utilization(&self) -> f32 {
        if let Some(ref nvml) = self.nvml {
            if let Ok(device_count) = nvml.device_count() {
                let mut total_util = 0.0f32;
                let mut count = 0;
                for i in 0..device_count {
                    if let Ok(device) = nvml.device_by_index(i) {
                        if let Ok(utilization) = device.utilization_rates() {
                            total_util += utilization.gpu as f32;
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    return total_util / count as f32;
                }
            }
        }
        0.0
    }
}

/// Format bytes to human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
