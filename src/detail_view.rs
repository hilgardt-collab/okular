use gtk4::prelude::*;
use gtk4::{Box as GtkBox, DrawingArea, Label, Orientation, ScrolledWindow, Separator};
use std::cell::RefCell;
use std::rc::Rc;

use crate::monitor::{ProcessHistory, format_bytes};

/// Colors for the graphs
const CPU_COLOR: (f64, f64, f64) = (0.204, 0.396, 0.643); // Blue
const MEMORY_COLOR: (f64, f64, f64) = (0.584, 0.345, 0.698); // Purple
const DISK_READ_COLOR: (f64, f64, f64) = (0.180, 0.545, 0.341); // Green
const DISK_WRITE_COLOR: (f64, f64, f64) = (0.902, 0.494, 0.133); // Orange

/// Graph configuration
const GRAPH_LEFT_MARGIN: f64 = 55.0;  // Space for Y-axis labels
const GRAPH_BOTTOM_MARGIN: f64 = 20.0; // Space for X-axis labels
const GRAPH_RIGHT_MARGIN: f64 = 10.0;
const GRAPH_TOP_MARGIN: f64 = 5.0;

/// Format a value for Y-axis display
fn format_y_value(value: f64, is_percentage: bool, is_bytes: bool) -> String {
    if is_percentage {
        format!("{:.0}%", value)
    } else if is_bytes {
        format_bytes(value as u64)
    } else {
        format!("{:.1}", value)
    }
}

/// Calculate nice Y-axis tick values
fn calculate_y_ticks(max_value: f64, is_percentage: bool) -> Vec<f64> {
    if is_percentage {
        // For percentages, use fixed ticks
        let max_tick = if max_value <= 25.0 {
            25.0
        } else if max_value <= 50.0 {
            50.0
        } else if max_value <= 100.0 {
            100.0
        } else {
            ((max_value / 50.0).ceil() * 50.0).max(100.0)
        };
        vec![0.0, max_tick * 0.25, max_tick * 0.5, max_tick * 0.75, max_tick]
    } else {
        // For other values, calculate nice intervals
        if max_value <= 0.0 {
            return vec![0.0];
        }

        let magnitude = 10_f64.powf(max_value.log10().floor());
        let normalized = max_value / magnitude;

        let nice_max = if normalized <= 1.0 {
            magnitude
        } else if normalized <= 2.0 {
            2.0 * magnitude
        } else if normalized <= 5.0 {
            5.0 * magnitude
        } else {
            10.0 * magnitude
        };

        vec![0.0, nice_max * 0.25, nice_max * 0.5, nice_max * 0.75, nice_max]
    }
}

/// Graph data with metadata
#[derive(Clone)]
struct GraphData {
    values: Vec<f64>,
    max_value: f64,
    is_percentage: bool,
    is_bytes: bool,
    num_samples: usize,
    sample_interval_secs: u64,
}

impl Default for GraphData {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            max_value: 100.0,
            is_percentage: false,
            is_bytes: false,
            num_samples: 60,
            sample_interval_secs: 2,
        }
    }
}

/// A single graph widget with axis labels
struct GraphWidget {
    drawing_area: DrawingArea,
    data: Rc<RefCell<GraphData>>,
    color: (f64, f64, f64),
}

impl GraphWidget {
    fn new(color: (f64, f64, f64), is_percentage: bool, is_bytes: bool) -> Self {
        let drawing_area = DrawingArea::new();
        drawing_area.set_size_request(-1, 120);
        drawing_area.set_hexpand(true);
        drawing_area.set_vexpand(true);

        let data = Rc::new(RefCell::new(GraphData {
            is_percentage,
            is_bytes,
            ..Default::default()
        }));

        let data_clone = data.clone();
        let color_clone = color;

        drawing_area.set_draw_func(move |_widget, cr, width, height| {
            let data = data_clone.borrow();
            let width_f = width as f64;
            let height_f = height as f64;

            // Calculate graph area
            let graph_left = GRAPH_LEFT_MARGIN;
            let graph_right = width_f - GRAPH_RIGHT_MARGIN;
            let graph_top = GRAPH_TOP_MARGIN;
            let graph_bottom = height_f - GRAPH_BOTTOM_MARGIN;
            let graph_width = graph_right - graph_left;
            let graph_height = graph_bottom - graph_top;

            // Background
            cr.set_source_rgb(0.12, 0.12, 0.12);
            let _ = cr.paint();

            // Calculate Y-axis ticks
            let y_ticks = calculate_y_ticks(data.max_value, data.is_percentage);
            let y_max = *y_ticks.last().unwrap_or(&100.0);

            // Draw grid lines and Y-axis labels
            cr.set_source_rgba(0.3, 0.3, 0.3, 0.8);
            cr.set_line_width(1.0);

            for &tick in &y_ticks {
                let y = graph_bottom - (tick / y_max) * graph_height;

                // Grid line
                cr.move_to(graph_left, y);
                cr.line_to(graph_right, y);
                let _ = cr.stroke();

                // Y-axis label
                cr.set_source_rgba(0.7, 0.7, 0.7, 1.0);
                let label = format_y_value(tick, data.is_percentage, data.is_bytes);
                if let Ok(extents) = cr.text_extents(&label) {
                    cr.move_to(graph_left - extents.width() - 5.0, y + extents.height() / 2.0);
                    let _ = cr.show_text(&label);
                }
                cr.set_source_rgba(0.3, 0.3, 0.3, 0.8);
            }

            // Draw X-axis labels (time)
            let total_time_secs = data.num_samples as u64 * data.sample_interval_secs;
            cr.set_source_rgba(0.7, 0.7, 0.7, 1.0);

            // Show labels at 0%, 50%, 100% of the time range
            let time_labels = [
                (0.0, format!("{}s", total_time_secs)),
                (0.5, format!("{}s", total_time_secs / 2)),
                (1.0, "now".to_string()),
            ];

            for (pos, label) in &time_labels {
                let x = graph_left + pos * graph_width;
                if let Ok(extents) = cr.text_extents(label) {
                    let x_centered = if *pos == 0.0 {
                        x
                    } else if *pos == 1.0 {
                        x - extents.width()
                    } else {
                        x - extents.width() / 2.0
                    };
                    cr.move_to(x_centered, height_f - 3.0);
                    let _ = cr.show_text(label);
                }
            }

            // Draw data if we have any
            if data.values.len() >= 2 {
                let num_points = data.values.len();
                let step = graph_width / (num_points - 1) as f64;

                // Fill area under curve
                cr.move_to(graph_left, graph_bottom);
                for (i, &value) in data.values.iter().enumerate() {
                    let x = graph_left + i as f64 * step;
                    let normalized = if y_max > 0.0 {
                        (value / y_max).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let y = graph_bottom - (normalized * graph_height);
                    cr.line_to(x, y);
                }
                cr.line_to(graph_right, graph_bottom);
                cr.close_path();
                cr.set_source_rgba(color_clone.0, color_clone.1, color_clone.2, 0.3);
                let _ = cr.fill();

                // Draw line on top
                cr.set_source_rgb(color_clone.0, color_clone.1, color_clone.2);
                cr.set_line_width(2.0);
                for (i, &value) in data.values.iter().enumerate() {
                    let x = graph_left + i as f64 * step;
                    let normalized = if y_max > 0.0 {
                        (value / y_max).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let y = graph_bottom - (normalized * graph_height);
                    if i == 0 {
                        cr.move_to(x, y);
                    } else {
                        cr.line_to(x, y);
                    }
                }
                let _ = cr.stroke();
            } else if data.values.len() == 1 {
                // Single data point - draw a dot
                let normalized = if y_max > 0.0 {
                    (data.values[0] / y_max).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let y = graph_bottom - (normalized * graph_height);
                cr.set_source_rgb(color_clone.0, color_clone.1, color_clone.2);
                cr.arc(graph_right, y, 3.0, 0.0, 2.0 * std::f64::consts::PI);
                let _ = cr.fill();
            }

            // Border around graph area
            cr.set_source_rgba(0.4, 0.4, 0.4, 1.0);
            cr.set_line_width(1.0);
            cr.rectangle(graph_left, graph_top, graph_width, graph_height);
            let _ = cr.stroke();
        });

        Self {
            drawing_area,
            data,
            color,
        }
    }

    fn update(&self, values: &[f64], num_samples: usize, sample_interval_secs: u64) {
        let mut data = self.data.borrow_mut();
        data.values = values.to_vec();
        data.num_samples = num_samples;
        data.sample_interval_secs = sample_interval_secs;

        // Auto-scale: find max value with some headroom
        let max_val = values.iter().cloned().fold(0.0_f64, f64::max);
        // Ensure minimum of 1.0 to avoid division issues and provide meaningful scale
        data.max_value = max_val.max(1.0);

        self.drawing_area.queue_draw();
    }

    #[allow(dead_code)]
    fn color(&self) -> (f64, f64, f64) {
        self.color
    }
}

/// Statistics for a metric
struct MetricStats {
    current: f64,
    min: f64,
    max: f64,
    avg: f64,
}

impl MetricStats {
    fn from_data(data: &[f64]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }
        let current = *data.last()?;
        let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg = data.iter().sum::<f64>() / data.len() as f64;
        Some(Self { current, min, max, avg })
    }
}

/// Detail view panel showing graphs for a selected process
pub struct DetailView {
    pub widget: ScrolledWindow,
    #[allow(dead_code)]
    container: GtkBox,
    // Process info section
    title_label: Label,
    info_labels: ProcessInfoLabels,
    // Graphs
    cpu_graph: GraphWidget,
    memory_graph: GraphWidget,
    disk_read_graph: GraphWidget,
    disk_write_graph: GraphWidget,
    // Stats labels
    cpu_stats: StatsLabels,
    memory_stats: StatsLabels,
    disk_read_stats: StatsLabels,
    disk_write_stats: StatsLabels,
}

struct ProcessInfoLabels {
    command: Label,
    threads: Label,
    state: Label,
    user: Label,
}

struct StatsLabels {
    current: Label,
    min: Label,
    max: Label,
    avg: Label,
}

impl StatsLabels {
    fn new() -> Self {
        let make_label = || {
            let label = Label::new(Some("-"));
            label.set_halign(gtk4::Align::End);
            label.add_css_class("monospace");
            label
        };
        Self {
            current: make_label(),
            min: make_label(),
            max: make_label(),
            avg: make_label(),
        }
    }

    fn update(&self, stats: Option<MetricStats>, is_percentage: bool, is_bytes: bool) {
        if let Some(stats) = stats {
            let format_val = |v: f64| {
                if is_percentage {
                    format!("{:.1}%", v)
                } else if is_bytes {
                    format_bytes(v as u64)
                } else {
                    format!("{:.1}", v)
                }
            };
            self.current.set_label(&format_val(stats.current));
            self.min.set_label(&format_val(stats.min));
            self.max.set_label(&format_val(stats.max));
            self.avg.set_label(&format_val(stats.avg));
        } else {
            self.current.set_label("-");
            self.min.set_label("-");
            self.max.set_label("-");
            self.avg.set_label("-");
        }
    }
}

impl DetailView {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Vertical, 12);
        container.set_margin_top(12);
        container.set_margin_bottom(12);
        container.set_margin_start(12);
        container.set_margin_end(12);

        // Title
        let title_label = Label::new(Some("Select a process"));
        title_label.add_css_class("title-2");
        title_label.set_halign(gtk4::Align::Start);
        title_label.set_wrap(true);
        title_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        container.append(&title_label);

        // Process info section
        let info_box = GtkBox::new(Orientation::Vertical, 4);
        info_box.add_css_class("card");
        info_box.set_margin_bottom(8);

        let info_labels = ProcessInfoLabels {
            command: Self::create_info_row(&info_box, "Command"),
            threads: Self::create_info_row(&info_box, "Threads"),
            state: Self::create_info_row(&info_box, "State"),
            user: Self::create_info_row(&info_box, "User"),
        };
        container.append(&info_box);

        // Separator
        let sep = Separator::new(Orientation::Horizontal);
        sep.set_margin_top(4);
        sep.set_margin_bottom(4);
        container.append(&sep);

        // Create graphs
        let cpu_graph = GraphWidget::new(CPU_COLOR, true, false);
        let memory_graph = GraphWidget::new(MEMORY_COLOR, false, true);
        let disk_read_graph = GraphWidget::new(DISK_READ_COLOR, false, true);
        let disk_write_graph = GraphWidget::new(DISK_WRITE_COLOR, false, true);

        // Create stats labels
        let cpu_stats = StatsLabels::new();
        let memory_stats = StatsLabels::new();
        let disk_read_stats = StatsLabels::new();
        let disk_write_stats = StatsLabels::new();

        // CPU section
        let cpu_section = Self::create_graph_section("CPU Usage", &cpu_graph, &cpu_stats);
        container.append(&cpu_section);

        // Memory section
        let memory_section = Self::create_graph_section("Memory", &memory_graph, &memory_stats);
        container.append(&memory_section);

        // Disk Read section
        let disk_read_section = Self::create_graph_section("Disk Read", &disk_read_graph, &disk_read_stats);
        container.append(&disk_read_section);

        // Disk Write section
        let disk_write_section = Self::create_graph_section("Disk Write", &disk_write_graph, &disk_write_stats);
        container.append(&disk_write_section);

        // Wrap in scrolled window
        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&container)
            .vexpand(true)
            .hexpand(true)
            .build();

        Self {
            widget: scrolled,
            container,
            title_label,
            info_labels,
            cpu_graph,
            memory_graph,
            disk_read_graph,
            disk_write_graph,
            cpu_stats,
            memory_stats,
            disk_read_stats,
            disk_write_stats,
        }
    }

    fn create_info_row(parent: &GtkBox, label_text: &str) -> Label {
        let row = GtkBox::new(Orientation::Horizontal, 8);
        row.set_margin_start(8);
        row.set_margin_end(8);
        row.set_margin_top(2);
        row.set_margin_bottom(2);

        let label = Label::new(Some(label_text));
        label.set_halign(gtk4::Align::Start);
        label.set_width_chars(10);
        label.add_css_class("dim-label");
        row.append(&label);

        let value = Label::new(Some("-"));
        value.set_halign(gtk4::Align::Start);
        value.set_hexpand(true);
        value.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        value.set_selectable(true);
        row.append(&value);

        parent.append(&row);
        value
    }

    fn create_graph_section(title: &str, graph: &GraphWidget, stats: &StatsLabels) -> GtkBox {
        let section = GtkBox::new(Orientation::Vertical, 4);
        section.set_vexpand(true);

        // Header with title
        let header = GtkBox::new(Orientation::Horizontal, 8);
        let label = Label::new(Some(title));
        label.add_css_class("heading");
        label.set_halign(gtk4::Align::Start);
        header.append(&label);
        section.append(&header);

        // Graph
        section.append(&graph.drawing_area);

        // Stats row
        let stats_box = GtkBox::new(Orientation::Horizontal, 16);
        stats_box.set_margin_top(4);
        stats_box.set_halign(gtk4::Align::Fill);
        stats_box.set_hexpand(true);

        let add_stat = |parent: &GtkBox, name: &str, label: &Label| {
            let stat_box = GtkBox::new(Orientation::Horizontal, 4);
            stat_box.set_hexpand(true);

            let name_label = Label::new(Some(name));
            name_label.add_css_class("dim-label");
            name_label.set_halign(gtk4::Align::Start);
            stat_box.append(&name_label);

            label.set_halign(gtk4::Align::End);
            label.set_hexpand(true);
            stat_box.append(label);

            parent.append(&stat_box);
        };

        add_stat(&stats_box, "Current:", &stats.current);
        add_stat(&stats_box, "Min:", &stats.min);
        add_stat(&stats_box, "Max:", &stats.max);
        add_stat(&stats_box, "Avg:", &stats.avg);

        section.append(&stats_box);

        section
    }

    /// Update the detail view for a process
    pub fn update(&self, name: &str, pid: u32, history: Option<&ProcessHistory>, process_info: Option<&ProcessDetails>) {
        self.title_label.set_label(&format!("{} (PID: {})", name, pid));

        // Update process info
        if let Some(info) = process_info {
            self.info_labels.command.set_label(&info.command);
            self.info_labels.command.set_tooltip_text(Some(&info.command));
            self.info_labels.threads.set_label(&format!("{}", info.thread_count));
            self.info_labels.state.set_label(&info.state);
            self.info_labels.user.set_label(&info.user);
        } else {
            self.info_labels.command.set_label("-");
            self.info_labels.command.set_tooltip_text(None);
            self.info_labels.threads.set_label("-");
            self.info_labels.state.set_label("-");
            self.info_labels.user.set_label("-");
        }

        if let Some(history) = history {
            let num_samples = history.cpu_history.len().max(1);
            let sample_interval = 2; // 2 seconds

            // CPU
            let cpu_data: Vec<f64> = history.cpu_history.iter().map(|&v| v as f64).collect();
            self.cpu_graph.update(&cpu_data, num_samples, sample_interval);
            self.cpu_stats.update(MetricStats::from_data(&cpu_data), true, false);

            // Memory
            let memory_data: Vec<f64> = history.memory_history.iter().map(|&v| v as f64).collect();
            self.memory_graph.update(&memory_data, num_samples, sample_interval);
            self.memory_stats.update(MetricStats::from_data(&memory_data), false, true);

            // Disk read
            let disk_read_data: Vec<f64> = history.disk_read_history.iter().map(|&v| v as f64).collect();
            self.disk_read_graph.update(&disk_read_data, num_samples, sample_interval);
            self.disk_read_stats.update(MetricStats::from_data(&disk_read_data), false, true);

            // Disk write
            let disk_write_data: Vec<f64> = history.disk_write_history.iter().map(|&v| v as f64).collect();
            self.disk_write_graph.update(&disk_write_data, num_samples, sample_interval);
            self.disk_write_stats.update(MetricStats::from_data(&disk_write_data), false, true);
        } else {
            // No history yet - show empty graphs
            self.cpu_graph.update(&[], 60, 2);
            self.memory_graph.update(&[], 60, 2);
            self.disk_read_graph.update(&[], 60, 2);
            self.disk_write_graph.update(&[], 60, 2);
            self.cpu_stats.update(None, true, false);
            self.memory_stats.update(None, false, true);
            self.disk_read_stats.update(None, false, true);
            self.disk_write_stats.update(None, false, true);
        }
    }

    /// Clear the detail view
    pub fn clear(&self) {
        self.title_label.set_label("Select a process");
        self.info_labels.command.set_label("-");
        self.info_labels.command.set_tooltip_text(None);
        self.info_labels.threads.set_label("-");
        self.info_labels.state.set_label("-");
        self.info_labels.user.set_label("-");

        self.cpu_graph.update(&[], 60, 2);
        self.memory_graph.update(&[], 60, 2);
        self.disk_read_graph.update(&[], 60, 2);
        self.disk_write_graph.update(&[], 60, 2);

        self.cpu_stats.update(None, true, false);
        self.memory_stats.update(None, false, true);
        self.disk_read_stats.update(None, false, true);
        self.disk_write_stats.update(None, false, true);
    }
}

/// Additional process details read from /proc
#[derive(Debug, Clone)]
pub struct ProcessDetails {
    pub command: String,
    pub thread_count: u32,
    pub state: String,
    pub user: String,
}

impl ProcessDetails {
    /// Read process details from /proc/<pid>
    pub fn from_pid(pid: u32) -> Option<Self> {
        // Read command line
        let cmdline_path = format!("/proc/{}/cmdline", pid);
        let command = std::fs::read_to_string(&cmdline_path)
            .ok()
            .map(|s| s.replace('\0', " ").trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "[unknown]".to_string());

        // Read status for thread count, state, and UID
        let status_path = format!("/proc/{}/status", pid);
        let status_content = std::fs::read_to_string(&status_path).ok()?;

        let mut thread_count = 1u32;
        let mut state = "Unknown".to_string();
        let mut uid = 0u32;

        for line in status_content.lines() {
            if let Some(threads_str) = line.strip_prefix("Threads:") {
                thread_count = threads_str.trim().parse().unwrap_or(1);
            } else if let Some(state_str) = line.strip_prefix("State:") {
                state = match state_str.trim().chars().next() {
                    Some('R') => "Running".to_string(),
                    Some('S') => "Sleeping".to_string(),
                    Some('D') => "Disk Sleep".to_string(),
                    Some('Z') => "Zombie".to_string(),
                    Some('T') => "Stopped".to_string(),
                    Some('t') => "Tracing Stop".to_string(),
                    Some('X') => "Dead".to_string(),
                    Some('I') => "Idle".to_string(),
                    _ => state_str.trim().to_string(),
                };
            } else if let Some(uid_str) = line.strip_prefix("Uid:") {
                // Format: real, effective, saved, filesystem - we want real UID
                if let Some(real_uid) = uid_str.split_whitespace().next() {
                    uid = real_uid.parse().unwrap_or(0);
                }
            }
        }

        // Convert UID to username
        let user = uid_to_username(uid);

        Some(Self {
            command,
            thread_count,
            state,
            user,
        })
    }
}

/// Convert UID to username by reading /etc/passwd
fn uid_to_username(uid: u32) -> String {
    if let Ok(content) = std::fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                if let Ok(line_uid) = parts[2].parse::<u32>() {
                    if line_uid == uid {
                        return parts[0].to_string();
                    }
                }
            }
        }
    }
    format!("{}", uid)
}
