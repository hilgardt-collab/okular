# okular

A modern Linux process monitor built with GTK4 and libadwaita, designed for troubleshooting and monitoring system processes.

## Features

### Process List
- **Flat process view**: Shows processes with thread count displayed inline
- **Sortable columns**: Sort by Name, PID, CPU%, Memory, Disk I/O, or GPU%
- **Search/filter**: Quickly find processes by name or PID
- **Real-time updates**: Process data refreshes every 2 seconds
- **Double-click**: Open detailed process window for any process

### Process Window (double-click a process)
Detailed monitoring of a single process including:
- **Command line**: Full command with arguments
- **Thread count**: Number of threads in the process
- **State**: Running, Sleeping, Disk Sleep, Zombie, etc.
- **User**: Owner of the process
- **Resource graphs**: CPU, Memory, Disk I/O, GPU, and Network usage over time
- **CPU core distribution**: Visual display of thread distribution across CPU cores
- **Configurable history**: Track up to 60 minutes of history

### GPU Monitoring
- NVIDIA GPU utilization and memory usage per process (requires NVML)

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

1. **Search**: Use the search bar to filter processes by name or PID
2. **Sort**: Click column headers to sort the process list
3. **Details**: Double-click any process to open a detailed monitoring window
4. **History**: In the process window, use the dropdown to change the graph history duration

## Architecture

```
src/
├── main.rs            # Application entry point
├── window.rs          # Main window with process list
├── monitor.rs         # System monitoring (sysinfo, NVML, /proc)
├── process_list.rs    # Process list widget
├── process_window.rs  # Detailed single-process monitoring window
├── process_actions.rs # Process control (kill, priority, affinity)
├── detail_view.rs     # Detail panel with graphs and stats
└── context_menu.rs    # Right-click context menu
```

### Key Implementation Details

- **Thread Grouping**: Uses Linux TGID (Thread Group ID) from `/proc/<pid>/status` to group threads and display count
- **History Storage**: Uses `VecDeque` for O(1) insertion and removal of historical data points
- **GPU Monitoring**: Integrates with NVIDIA NVML for per-process GPU memory and utilization tracking

## License

GPL-3.0

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.
