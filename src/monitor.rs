use sysinfo::{System, ProcessesToUpdate, ProcessRefreshKind};
use std::collections::HashMap;

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

    /// Get total disk I/O including children
    pub fn total_disk_io(&self) -> u64 {
        let self_total = self.disk_read_bytes + self.disk_write_bytes;
        let children_total: u64 = self.children.iter()
            .map(|c| c.disk_read_bytes + c.disk_write_bytes)
            .sum();
        self_total + children_total
    }

    /// Get child count
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

/// History entry for graphing
#[derive(Debug, Clone, Default)]
pub struct ProcessHistory {
    pub cpu_history: Vec<f32>,
    pub memory_history: Vec<u64>,
    pub disk_read_history: Vec<u64>,
    pub disk_write_history: Vec<u64>,
}

impl ProcessHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_sample(&mut self, cpu: f32, memory: u64, disk_read: u64, disk_write: u64, max_samples: usize) {
        self.cpu_history.push(cpu);
        self.memory_history.push(memory);
        self.disk_read_history.push(disk_read);
        self.disk_write_history.push(disk_write);

        // Keep only the last max_samples
        while self.cpu_history.len() > max_samples {
            self.cpu_history.remove(0);
        }
        while self.memory_history.len() > max_samples {
            self.memory_history.remove(0);
        }
        while self.disk_read_history.len() > max_samples {
            self.disk_read_history.remove(0);
        }
        while self.disk_write_history.len() > max_samples {
            self.disk_write_history.remove(0);
        }
    }

    /// Trim history to new max samples
    pub fn trim_to(&mut self, max_samples: usize) {
        while self.cpu_history.len() > max_samples {
            self.cpu_history.remove(0);
        }
        while self.memory_history.len() > max_samples {
            self.memory_history.remove(0);
        }
        while self.disk_read_history.len() > max_samples {
            self.disk_read_history.remove(0);
        }
        while self.disk_write_history.len() > max_samples {
            self.disk_write_history.remove(0);
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

        Self {
            system,
            process_history: HashMap::new(),
            nvml,
            cpu_count,
            max_samples: 60, // Default: 2 minutes at 2-second intervals
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

    /// Refresh process data and return top 150 processes by CPU usage, grouped by parent
    pub fn refresh(&mut self) -> Vec<ProcessInfo> {
        let refresh_kind = ProcessRefreshKind::new()
            .with_cpu()
            .with_memory()
            .with_disk_usage();
        self.system.refresh_processes_specifics(ProcessesToUpdate::All, refresh_kind);

        // Get GPU utilization per process if available
        let gpu_usage = self.get_gpu_process_usage();

        // Normalize CPU by dividing by CPU count
        let cpu_divisor = self.cpu_count as f32;

        // First pass: collect all processes with their parent info
        let mut all_processes: HashMap<u32, (ProcessInfo, Option<u32>)> = HashMap::new();

        for (pid, proc) in self.system.processes() {
            let pid_u32 = pid.as_u32();
            let parent_pid = proc.parent().map(|p| p.as_u32());
            let normalized_cpu = proc.cpu_usage() / cpu_divisor;

            let info = ProcessInfo {
                pid: pid_u32,
                name: proc.name().to_string_lossy().to_string(),
                cpu_percent: normalized_cpu,
                memory_bytes: proc.memory(),
                disk_read_bytes: proc.disk_usage().read_bytes,
                disk_write_bytes: proc.disk_usage().written_bytes,
                gpu_percent: gpu_usage.get(&pid_u32).copied(),
                children: Vec::new(),
                is_group: false,
            };

            all_processes.insert(pid_u32, (info, parent_pid));
        }

        // Second pass: group children under parents with the same name
        // A process is considered a "thread" if its parent has the same name
        let mut grouped: HashMap<u32, ProcessInfo> = HashMap::new();
        let mut children_pids: std::collections::HashSet<u32> = std::collections::HashSet::new();

        for (pid, (proc_info, parent_pid)) in &all_processes {
            if let Some(parent) = parent_pid {
                // Check if parent exists and has the same name (likely a thread)
                if let Some((parent_info, _)) = all_processes.get(parent) {
                    if parent_info.name == proc_info.name {
                        // This is a thread - will be added as child of parent
                        children_pids.insert(*pid);
                        continue;
                    }
                }
            }
        }

        // Third pass: build the grouped structure
        for (pid, (mut proc_info, _parent_pid)) in all_processes.clone() {
            if children_pids.contains(&pid) {
                // This is a child thread, skip for now
                continue;
            }

            // Find all children (threads with same name)
            let mut children: Vec<ProcessInfo> = Vec::new();
            for (child_pid, (child_info, child_parent)) in &all_processes {
                if children_pids.contains(child_pid) {
                    if let Some(parent) = child_parent {
                        if *parent == pid {
                            children.push(child_info.clone());
                        }
                    }
                }
            }

            if !children.is_empty() {
                proc_info.is_group = true;
                proc_info.children = children;
            }

            grouped.insert(pid, proc_info);
        }

        // Convert to vec and sort by total CPU usage
        let mut processes: Vec<ProcessInfo> = grouped.into_values().collect();
        processes.sort_by(|a, b| {
            b.total_cpu().partial_cmp(&a.total_cpu()).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take top 150
        processes.truncate(150);

        // Update history for tracked processes (use total values for groups)
        let max_samples = self.max_samples;
        for proc in &processes {
            let history = self.process_history.entry(proc.pid).or_insert_with(ProcessHistory::new);
            history.add_sample(
                proc.total_cpu(),
                proc.total_memory(),
                proc.disk_read_bytes + proc.children.iter().map(|c| c.disk_read_bytes).sum::<u64>(),
                proc.disk_write_bytes + proc.children.iter().map(|c| c.disk_write_bytes).sum::<u64>(),
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
                        // Also check graphics processes
                        if let Ok(processes) = device.running_graphics_processes() {
                            for proc in processes {
                                if let Ok(mem_info) = device.memory_info() {
                                    if mem_info.total > 0 {
                                        let mem_used = match proc.used_gpu_memory {
                                            UsedGpuMemory::Used(bytes) => bytes,
                                            UsedGpuMemory::Unavailable => 0,
                                        };
                                        let percent = (mem_used as f32 / mem_info.total as f32) * 100.0;
                                        usage.entry(proc.pid).or_insert(percent);
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
