use gtk4::prelude::*;
use gtk4::{Box as GtkBox, DrawingArea, Label, Orientation, ScrolledWindow};
use std::cell::RefCell;
use std::rc::Rc;

use crate::monitor::{ProcessHistory, format_bytes};

/// Colors for the graphs
const CPU_COLOR: (f64, f64, f64) = (0.204, 0.396, 0.643); // Blue
const MEMORY_COLOR: (f64, f64, f64) = (0.584, 0.345, 0.698); // Purple
const DISK_READ_COLOR: (f64, f64, f64) = (0.180, 0.545, 0.341); // Green
const DISK_WRITE_COLOR: (f64, f64, f64) = (0.902, 0.494, 0.133); // Orange

/// A single graph widget
struct GraphWidget {
    drawing_area: DrawingArea,
    data: Rc<RefCell<Vec<f64>>>,
    max_value: Rc<RefCell<f64>>,
}

impl GraphWidget {
    fn new(color: (f64, f64, f64)) -> Self {
        let drawing_area = DrawingArea::new();
        // Set minimum size but allow expansion
        drawing_area.set_size_request(-1, 100);
        drawing_area.set_hexpand(true);
        drawing_area.set_vexpand(true);

        let data: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
        let max_value = Rc::new(RefCell::new(100.0));

        let data_clone = data.clone();
        let max_value_clone = max_value.clone();

        drawing_area.set_draw_func(move |_widget, cr, width, height| {
            let data = data_clone.borrow();
            let max_val = *max_value_clone.borrow();
            let width_f = width as f64;
            let height_f = height as f64;

            // Background - dark gray
            cr.set_source_rgb(0.12, 0.12, 0.12);
            let _ = cr.paint();

            // Grid lines
            cr.set_source_rgba(0.25, 0.25, 0.25, 0.8);
            cr.set_line_width(1.0);
            for i in 1..4 {
                let y = (height_f / 4.0) * i as f64;
                cr.move_to(0.0, y);
                cr.line_to(width_f, y);
                let _ = cr.stroke();
            }

            // If we have data, draw it
            if data.len() >= 2 {
                let num_points = data.len();
                let step = width_f / (num_points - 1) as f64;

                // Fill area under curve
                cr.move_to(0.0, height_f);
                for (i, &value) in data.iter().enumerate() {
                    let x = i as f64 * step;
                    let normalized = if max_val > 0.0 {
                        (value / max_val).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let y = height_f - (normalized * height_f);
                    cr.line_to(x, y);
                }
                cr.line_to(width_f, height_f);
                cr.close_path();
                cr.set_source_rgba(color.0, color.1, color.2, 0.3);
                let _ = cr.fill();

                // Draw line on top
                cr.set_source_rgb(color.0, color.1, color.2);
                cr.set_line_width(2.0);
                for (i, &value) in data.iter().enumerate() {
                    let x = i as f64 * step;
                    let normalized = if max_val > 0.0 {
                        (value / max_val).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let y = height_f - (normalized * height_f);
                    if i == 0 {
                        cr.move_to(x, y);
                    } else {
                        cr.line_to(x, y);
                    }
                }
                let _ = cr.stroke();
            } else if data.len() == 1 {
                // Single data point - draw a horizontal line
                let normalized = if max_val > 0.0 {
                    (data[0] / max_val).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let y = height_f - (normalized * height_f);
                cr.set_source_rgb(color.0, color.1, color.2);
                cr.set_line_width(2.0);
                cr.move_to(0.0, y);
                cr.line_to(width_f, y);
                let _ = cr.stroke();
            }

            // Border
            cr.set_source_rgba(0.3, 0.3, 0.3, 1.0);
            cr.set_line_width(1.0);
            cr.rectangle(0.5, 0.5, width_f - 1.0, height_f - 1.0);
            let _ = cr.stroke();
        });

        Self {
            drawing_area,
            data,
            max_value,
        }
    }

    fn update(&self, new_data: &[f64], max: f64) {
        *self.data.borrow_mut() = new_data.to_vec();
        *self.max_value.borrow_mut() = max.max(0.001); // Avoid division by zero
        self.drawing_area.queue_draw();
    }
}

/// Detail view panel showing graphs for a selected process
pub struct DetailView {
    pub widget: ScrolledWindow,
    #[allow(dead_code)]
    container: GtkBox,
    title_label: Label,
    cpu_graph: GraphWidget,
    memory_graph: GraphWidget,
    disk_read_graph: GraphWidget,
    disk_write_graph: GraphWidget,
    current_value_labels: CurrentValueLabels,
}

struct CurrentValueLabels {
    cpu: Label,
    memory: Label,
    disk_read: Label,
    disk_write: Label,
}

impl DetailView {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Vertical, 8);
        container.set_margin_top(12);
        container.set_margin_bottom(12);
        container.set_margin_start(12);
        container.set_margin_end(12);

        // Title
        let title_label = Label::new(Some("Select a process"));
        title_label.add_css_class("title-2");
        title_label.set_halign(gtk4::Align::Start);
        container.append(&title_label);

        // Create graphs
        let cpu_graph = GraphWidget::new(CPU_COLOR);
        let memory_graph = GraphWidget::new(MEMORY_COLOR);
        let disk_read_graph = GraphWidget::new(DISK_READ_COLOR);
        let disk_write_graph = GraphWidget::new(DISK_WRITE_COLOR);

        // CPU section
        let (cpu_section, cpu_value_label) = Self::create_graph_section("CPU Usage", &cpu_graph);
        container.append(&cpu_section);

        // Memory section
        let (memory_section, memory_value_label) = Self::create_graph_section("Memory", &memory_graph);
        container.append(&memory_section);

        // Disk Read section
        let (disk_read_section, disk_read_value_label) = Self::create_graph_section("Disk Read", &disk_read_graph);
        container.append(&disk_read_section);

        // Disk Write section
        let (disk_write_section, disk_write_value_label) = Self::create_graph_section("Disk Write", &disk_write_graph);
        container.append(&disk_write_section);

        let current_value_labels = CurrentValueLabels {
            cpu: cpu_value_label,
            memory: memory_value_label,
            disk_read: disk_read_value_label,
            disk_write: disk_write_value_label,
        };

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
            cpu_graph,
            memory_graph,
            disk_read_graph,
            disk_write_graph,
            current_value_labels,
        }
    }

    fn create_graph_section(title: &str, graph: &GraphWidget) -> (GtkBox, Label) {
        let section = GtkBox::new(Orientation::Vertical, 4);
        section.set_vexpand(true);

        let header = GtkBox::new(Orientation::Horizontal, 8);
        let label = Label::new(Some(title));
        label.add_css_class("heading");
        label.set_halign(gtk4::Align::Start);
        header.append(&label);

        let value_label = Label::new(Some("-"));
        value_label.set_halign(gtk4::Align::End);
        value_label.set_hexpand(true);
        value_label.add_css_class("numeric");
        header.append(&value_label);

        section.append(&header);
        section.append(&graph.drawing_area);

        (section, value_label)
    }

    /// Update the detail view for a process
    pub fn update(&self, name: &str, pid: u32, history: Option<&ProcessHistory>) {
        self.title_label.set_label(&format!("{} (PID: {})", name, pid));

        if let Some(history) = history {
            // CPU - percentage (0-100+)
            let cpu_data: Vec<f64> = history.cpu_history.iter().map(|&v| v as f64).collect();
            let cpu_max = cpu_data.iter().cloned().fold(100.0_f64, f64::max);
            self.cpu_graph.update(&cpu_data, cpu_max);
            if let Some(&current) = history.cpu_history.last() {
                self.current_value_labels.cpu.set_label(&format!("{:.1}%", current));
            }

            // Memory - bytes
            let memory_data: Vec<f64> = history.memory_history.iter().map(|&v| v as f64).collect();
            let memory_max = memory_data.iter().cloned().fold(1.0_f64, f64::max);
            self.memory_graph.update(&memory_data, memory_max);
            if let Some(&current) = history.memory_history.last() {
                self.current_value_labels.memory.set_label(&format_bytes(current));
            }

            // Disk read - bytes
            let disk_read_data: Vec<f64> = history.disk_read_history.iter().map(|&v| v as f64).collect();
            let disk_read_max = disk_read_data.iter().cloned().fold(1.0_f64, f64::max);
            self.disk_read_graph.update(&disk_read_data, disk_read_max);
            if let Some(&current) = history.disk_read_history.last() {
                self.current_value_labels.disk_read.set_label(&format!("{}/s", format_bytes(current)));
            }

            // Disk write - bytes
            let disk_write_data: Vec<f64> = history.disk_write_history.iter().map(|&v| v as f64).collect();
            let disk_write_max = disk_write_data.iter().cloned().fold(1.0_f64, f64::max);
            self.disk_write_graph.update(&disk_write_data, disk_write_max);
            if let Some(&current) = history.disk_write_history.last() {
                self.current_value_labels.disk_write.set_label(&format!("{}/s", format_bytes(current)));
            }
        } else {
            // No history yet - show empty graphs
            self.cpu_graph.update(&[], 100.0);
            self.memory_graph.update(&[], 1.0);
            self.disk_read_graph.update(&[], 1.0);
            self.disk_write_graph.update(&[], 1.0);
            self.current_value_labels.cpu.set_label("-");
            self.current_value_labels.memory.set_label("-");
            self.current_value_labels.disk_read.set_label("-");
            self.current_value_labels.disk_write.set_label("-");
        }
    }

    /// Clear the detail view
    pub fn clear(&self) {
        self.title_label.set_label("Select a process");
        self.cpu_graph.update(&[], 100.0);
        self.memory_graph.update(&[], 1.0);
        self.disk_read_graph.update(&[], 1.0);
        self.disk_write_graph.update(&[], 1.0);
        self.current_value_labels.cpu.set_label("-");
        self.current_value_labels.memory.set_label("-");
        self.current_value_labels.disk_read.set_label("-");
        self.current_value_labels.disk_write.set_label("-");
    }
}
