# Ocular

A modern Linux process monitor built with GTK4 and libadwaita, designed for troubleshooting and monitoring system processes.

## Features

### Process List
- **Hierarchical view**: Threads are properly grouped under their parent process using Linux TGID
- **Sortable columns**: Sort by Name, PID, CPU%, Memory, Disk I/O, or GPU%
- **Search/filter**: Quickly find processes by name or PID
- **Real-time updates**: Process data refreshes every 2 seconds

### Process Details
When a process is selected, view detailed information including:
- **Command line**: Full command with arguments
- **Thread count**: Number of threads in the process
- **State**: Running, Sleeping, Disk Sleep, Zombie, etc.
- **User**: Owner of the process

### Resource Graphs
Interactive graphs with:
- **CPU Usage**: Percentage of CPU utilization over time
- **Memory**: Memory consumption with auto-scaled units (KB/MB/GB)
- **Disk Read/Write**: I/O activity tracking

Graph features:
- Auto-scaling Y-axis with "nice" tick values
- X-axis time labels showing history duration
- Statistics row showing Current, Min, Max, and Average values
- Configurable history duration (1, 2, 5, 10, or 30 minutes)

### GPU Monitoring
- NVIDIA GPU memory usage per process (requires NVML)

## Requirements

### Runtime Dependencies
- GTK4 (4.12+)
- libadwaita (1.4+)
- Linux kernel with `/proc` filesystem

### Optional
- NVIDIA drivers with NVML for GPU monitoring

### Build Dependencies
- Rust 1.70+
- GTK4 development libraries
- libadwaita development libraries

#### Arch Linux
```bash
sudo pacman -S gtk4 libadwaita
```

#### Ubuntu/Debian
```bash
sudo apt install libgtk-4-dev libadwaita-1-dev
```

#### Fedora
```bash
sudo dnf install gtk4-devel libadwaita-devel
```

## Building

```bash
# Clone the repository
git clone https://github.com/hilgardt-collab/ocular.git
cd ocular

# Build release version
cargo build --release

# The binary will be at target/release/ocular
```

## Running

```bash
# Run directly
cargo run --release

# Or run the built binary
./target/release/ocular
```

## Usage

1. **Process List**: Click on any process to view its details
2. **Search**: Use the search bar to filter processes by name or PID
3. **Sort**: Click column headers to sort the process list
4. **History**: Use the dropdown to change the graph history duration
5. **Expand/Collapse**: Click the expander arrows to show/hide threads

## Architecture

```
src/
├── main.rs          # Application entry point
├── window.rs        # Main window and UI setup
├── monitor.rs       # System monitoring (sysinfo, NVML, /proc)
├── process_list.rs  # Process list widget with tree view
└── detail_view.rs   # Detail panel with graphs and stats
```

### Key Implementation Details

- **Thread Grouping**: Uses Linux TGID (Thread Group ID) from `/proc/<pid>/status` to correctly group threads under their parent process
- **History Storage**: Uses `VecDeque` for O(1) insertion and removal of historical data points
- **GPU Monitoring**: Integrates with NVIDIA NVML for per-process GPU memory tracking

## License

GPL-3.0

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.
