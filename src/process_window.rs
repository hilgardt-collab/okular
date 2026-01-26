//! Isolated process window for detailed monitoring of a single process

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, DropDown, Label, Orientation,
    ScrolledWindow, Separator, StringList, Window,
};
use libadwaita as adw;
use adw::prelude::*;
use glib::ControlFlow;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::detail_view::{DetailView, ProcessDetails};
use crate::monitor::SystemMonitor;
use crate::process_actions::{
    self, get_cpu_affinity, get_cpu_core_info, kill_process, set_cpu_affinity,
    set_priority, Priority, CoreType,
};

const UPDATE_INTERVAL_MS: u64 = 2000;

/// Create and show a window for monitoring a single process
pub fn open_process_window(
    parent: &impl IsA<Window>,
    pid: u32,
    name: &str,
    monitor: Rc<RefCell<SystemMonitor>>,
) {
    let window = adw::Window::builder()
        .title(&format!("{} (PID: {}) - okular", name, pid))
        .default_width(600)
        .default_height(700)
        .transient_for(parent)
        .build();

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    // Header bar with history dropdown
    let header = adw::HeaderBar::new();

    // History duration dropdown (up to 60 min for process window)
    let history_options = StringList::new(&[
        "1 min", "2 min", "5 min", "10 min", "15 min",
        "30 min", "45 min", "60 min",
    ]);
    let history_dropdown = DropDown::new(Some(history_options), gtk4::Expression::NONE);
    history_dropdown.set_selected(2); // Default to 5 minutes

    let history_box = GtkBox::new(Orientation::Horizontal, 8);
    let history_label = Label::new(Some("History:"));
    history_box.append(&history_label);
    history_box.append(&history_dropdown);
    header.pack_end(&history_box);

    main_box.append(&header);

    // Action buttons bar
    let action_bar = GtkBox::new(Orientation::Horizontal, 8);
    action_bar.set_margin_start(12);
    action_bar.set_margin_end(12);
    action_bar.set_margin_top(8);
    action_bar.set_margin_bottom(8);
    action_bar.set_halign(gtk4::Align::Center);

    // End Process button
    let end_btn = Button::with_label("End Process");
    end_btn.add_css_class("destructive-action");
    action_bar.append(&end_btn);

    // Force Kill button
    let kill_btn = Button::with_label("Force Kill");
    kill_btn.add_css_class("destructive-action");
    action_bar.append(&kill_btn);

    // Separator
    let sep = Separator::new(Orientation::Vertical);
    sep.set_margin_start(8);
    sep.set_margin_end(8);
    action_bar.append(&sep);

    // CPU Affinity button
    let affinity_btn = Button::with_label("CPU Affinity");
    action_bar.append(&affinity_btn);

    // Priority button
    let priority_btn = Button::with_label("Set Priority");
    action_bar.append(&priority_btn);

    main_box.append(&action_bar);

    // Separator
    let sep = Separator::new(Orientation::Horizontal);
    main_box.append(&sep);

    // Detail view
    let detail_view = DetailView::new();
    main_box.append(&detail_view.widget);

    window.set_content(Some(&main_box));

    // Initial update
    {
        let mon = monitor.borrow();
        let history = mon.get_history(pid);
        let process_details = ProcessDetails::from_pid(pid);
        detail_view.update(name, pid, history, process_details.as_ref());
    }

    // Store window reference for closing
    let window_weak = window.downgrade();
    let window_weak_for_timer = window.downgrade();
    let name_owned = name.to_string();
    let detail_view = Rc::new(detail_view);

    // Connect history duration dropdown
    let monitor_clone = monitor.clone();
    history_dropdown.connect_selected_notify(move |dropdown| {
        let idx = dropdown.selected();
        // Convert to samples (at 2-second intervals)
        let max_samples = match idx {
            0 => 30,    // 1 min
            1 => 60,    // 2 min
            2 => 150,   // 5 min
            3 => 300,   // 10 min
            4 => 450,   // 15 min
            5 => 900,   // 30 min
            6 => 1350,  // 45 min
            7 => 1800,  // 60 min
            _ => 150,   // Default to 5 min
        };
        monitor_clone.borrow_mut().set_max_samples(max_samples);
    });

    // Set up periodic refresh
    let detail_view_clone = detail_view.clone();
    let monitor_clone = monitor.clone();

    let source_id = glib::timeout_add_local(Duration::from_millis(UPDATE_INTERVAL_MS), move || {
        // Check if window still exists
        let Some(win) = window_weak_for_timer.upgrade() else {
            return ControlFlow::Break;
        };

        // Check if process still exists
        if !process_actions::is_process_running(pid) {
            // Process ended - close window
            win.close();
            return ControlFlow::Break;
        }

        // Update detail view
        let mon = monitor_clone.borrow();
        let history = mon.get_history(pid);
        let process_details = ProcessDetails::from_pid(pid);
        detail_view_clone.update(&name_owned, pid, history, process_details.as_ref());

        ControlFlow::Continue
    });

    let source_id = Rc::new(RefCell::new(Some(source_id)));

    // Connect End Process button
    let window_weak_clone = window_weak.clone();
    end_btn.connect_clicked(move |_| {
        if let Err(e) = kill_process(pid, false) {
            if let Some(win) = window_weak_clone.upgrade() {
                show_error_dialog(&win, "Failed to end process", &e.to_string());
            }
        }
        // Process will end, timer will close window
    });

    // Connect Force Kill button
    let window_weak_clone = window_weak.clone();
    let source_id_clone = source_id.clone();
    kill_btn.connect_clicked(move |_| {
        if let Err(e) = kill_process(pid, true) {
            if let Some(win) = window_weak_clone.upgrade() {
                show_error_dialog(&win, "Failed to kill process", &e.to_string());
            }
        } else {
            // Process killed, close window immediately
            if let Some(win) = window_weak_clone.upgrade() {
                if let Some(id) = source_id_clone.borrow_mut().take() {
                    id.remove();
                }
                win.close();
            }
        }
    });

    // Connect CPU Affinity button
    let window_weak_clone = window_weak.clone();
    affinity_btn.connect_clicked(move |_| {
        if let Some(win) = window_weak_clone.upgrade() {
            show_affinity_dialog(&win, pid);
        }
    });

    // Connect Priority button
    let window_weak_clone = window_weak.clone();
    priority_btn.connect_clicked(move |_| {
        if let Some(win) = window_weak_clone.upgrade() {
            show_priority_dialog(&win, pid);
        }
    });

    // Clean up timer on window close
    let source_id_clone = source_id.clone();
    window.connect_close_request(move |_| {
        if let Some(id) = source_id_clone.borrow_mut().take() {
            id.remove();
        }
        glib::Propagation::Proceed
    });

    window.present();
}

/// Show CPU affinity dialog with core type information
fn show_affinity_dialog(parent: &impl IsA<Window>, pid: u32) {
    let core_info = get_cpu_core_info();
    let current_affinity = get_cpu_affinity(pid).unwrap_or_else(|_| vec![true; core_info.len()]);

    let dialog = adw::Window::builder()
        .title("Set CPU Affinity")
        .transient_for(parent)
        .modal(true)
        .default_width(350)
        .default_height(500)
        .resizable(true)
        .build();

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    // Header bar with Cancel/Apply buttons
    let header = adw::HeaderBar::new();

    let cancel_btn = Button::with_label("Cancel");
    header.pack_start(&cancel_btn);

    let apply_btn = Button::with_label("Apply");
    apply_btn.add_css_class("suggested-action");
    header.pack_end(&apply_btn);

    main_box.append(&header);

    // Content
    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let label = Label::new(Some("Select which CPU cores this process can run on:"));
    label.set_halign(gtk4::Align::Start);
    content.append(&label);

    // Check if we have any special core types
    let has_special_cores = core_info.iter().any(|c| c.core_type != CoreType::Standard);
    if has_special_cores {
        let legend = create_core_type_legend(&core_info);
        content.append(&legend);
    }

    // Create scrolled window for CPU list
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let cpu_box = GtkBox::new(Orientation::Vertical, 4);
    let checkboxes: Rc<RefCell<Vec<CheckButton>>> = Rc::new(RefCell::new(Vec::new()));

    for info in &core_info {
        let label_text = if info.core_type != CoreType::Standard {
            format!("CPU {} ({})", info.cpu_id, info.core_type.label())
        } else if let Some(die) = info.die_id {
            format!("CPU {} [CCD {}]", info.cpu_id, die)
        } else {
            format!("CPU {}", info.cpu_id)
        };

        let checkbox = CheckButton::with_label(&label_text);
        checkbox.set_active(current_affinity.get(info.cpu_id).copied().unwrap_or(true));

        // Apply CSS class based on core type
        if let Some(css_class) = info.core_type.css_class() {
            checkbox.add_css_class(css_class);
        }

        cpu_box.append(&checkbox);
        checkboxes.borrow_mut().push(checkbox);
    }

    scrolled.set_child(Some(&cpu_box));
    content.append(&scrolled);

    // Select All / Deselect All / Select by type buttons
    let btn_box = GtkBox::new(Orientation::Horizontal, 8);
    btn_box.set_halign(gtk4::Align::Center);
    btn_box.set_margin_top(8);

    let select_all = Button::with_label("Select All");
    let checkboxes_clone = checkboxes.clone();
    select_all.connect_clicked(move |_| {
        for cb in checkboxes_clone.borrow().iter() {
            cb.set_active(true);
        }
    });
    btn_box.append(&select_all);

    let deselect_all = Button::with_label("Deselect All");
    let checkboxes_clone = checkboxes.clone();
    deselect_all.connect_clicked(move |_| {
        for cb in checkboxes_clone.borrow().iter() {
            cb.set_active(false);
        }
    });
    btn_box.append(&deselect_all);

    content.append(&btn_box);

    // Add core type selection buttons if we have hybrid/X3D cores
    if has_special_cores {
        let type_btn_box = GtkBox::new(Orientation::Horizontal, 8);
        type_btn_box.set_halign(gtk4::Align::Center);
        type_btn_box.set_margin_top(4);

        // Check what types we have
        let has_pcores = core_info.iter().any(|c| c.core_type == CoreType::PCore);
        let has_ecores = core_info.iter().any(|c| c.core_type == CoreType::ECore);
        let has_x3d = core_info.iter().any(|c| c.core_type == CoreType::X3D);

        if has_pcores {
            let core_info_clone = core_info.clone();
            let checkboxes_clone = checkboxes.clone();
            let pcore_btn = Button::with_label("P-Cores Only");
            pcore_btn.connect_clicked(move |_| {
                for (i, cb) in checkboxes_clone.borrow().iter().enumerate() {
                    cb.set_active(core_info_clone[i].core_type == CoreType::PCore);
                }
            });
            type_btn_box.append(&pcore_btn);
        }

        if has_ecores {
            let core_info_clone = core_info.clone();
            let checkboxes_clone = checkboxes.clone();
            let ecore_btn = Button::with_label("E-Cores Only");
            ecore_btn.connect_clicked(move |_| {
                for (i, cb) in checkboxes_clone.borrow().iter().enumerate() {
                    cb.set_active(core_info_clone[i].core_type == CoreType::ECore);
                }
            });
            type_btn_box.append(&ecore_btn);
        }

        if has_x3d {
            let core_info_clone = core_info.clone();
            let checkboxes_clone = checkboxes.clone();
            let x3d_btn = Button::with_label("X3D Only");
            x3d_btn.connect_clicked(move |_| {
                for (i, cb) in checkboxes_clone.borrow().iter().enumerate() {
                    cb.set_active(core_info_clone[i].core_type == CoreType::X3D);
                }
            });
            type_btn_box.append(&x3d_btn);

            let core_info_clone = core_info.clone();
            let checkboxes_clone = checkboxes.clone();
            let non_x3d_btn = Button::with_label("Non-X3D Only");
            non_x3d_btn.connect_clicked(move |_| {
                for (i, cb) in checkboxes_clone.borrow().iter().enumerate() {
                    cb.set_active(core_info_clone[i].core_type != CoreType::X3D);
                }
            });
            type_btn_box.append(&non_x3d_btn);
        }

        content.append(&type_btn_box);
    }

    main_box.append(&content);
    dialog.set_content(Some(&main_box));

    // Cancel button closes dialog
    let dialog_weak = dialog.downgrade();
    cancel_btn.connect_clicked(move |_| {
        if let Some(d) = dialog_weak.upgrade() {
            d.close();
        }
    });

    // Apply button
    let checkboxes_clone = checkboxes.clone();
    let parent_weak = parent.downgrade();
    let dialog_weak = dialog.downgrade();
    apply_btn.connect_clicked(move |_| {
        let selected_cpus: Vec<usize> = checkboxes_clone
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, cb)| cb.is_active())
            .map(|(i, _)| i)
            .collect();

        if selected_cpus.is_empty() {
            if let Some(parent) = parent_weak.upgrade() {
                show_error_dialog(&parent, "Invalid Selection", "You must select at least one CPU.");
            }
        } else if let Err(e) = set_cpu_affinity(pid, &selected_cpus) {
            if let Some(parent) = parent_weak.upgrade() {
                show_error_dialog(&parent, "Failed to set CPU affinity", &e.to_string());
            }
        }

        if let Some(d) = dialog_weak.upgrade() {
            d.close();
        }
    });

    dialog.present();
}

/// Create a legend showing core type colors
fn create_core_type_legend(core_info: &[crate::process_actions::CpuCoreInfo]) -> GtkBox {
    let legend = GtkBox::new(Orientation::Horizontal, 16);
    legend.set_halign(gtk4::Align::Center);
    legend.set_margin_top(4);
    legend.set_margin_bottom(4);

    let has_pcores = core_info.iter().any(|c| c.core_type == CoreType::PCore);
    let has_ecores = core_info.iter().any(|c| c.core_type == CoreType::ECore);
    let has_x3d = core_info.iter().any(|c| c.core_type == CoreType::X3D);

    if has_pcores {
        let label = Label::new(Some("● P-Core"));
        label.add_css_class("accent");
        legend.append(&label);
    }

    if has_ecores {
        let label = Label::new(Some("● E-Core"));
        label.add_css_class("dim-label");
        legend.append(&label);
    }

    if has_x3d {
        let label = Label::new(Some("● X3D"));
        label.add_css_class("success");
        legend.append(&label);
    }

    legend
}

/// Show priority dialog using adw::Window
fn show_priority_dialog(parent: &impl IsA<Window>, pid: u32) {
    let current_priority = process_actions::get_priority(pid).unwrap_or(0);

    let dialog = adw::Window::builder()
        .title("Set Process Priority")
        .transient_for(parent)
        .modal(true)
        .default_width(300)
        .default_height(350)
        .build();

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    // Header bar with Cancel/Apply buttons
    let header = adw::HeaderBar::new();

    let cancel_btn = Button::with_label("Cancel");
    header.pack_start(&cancel_btn);

    let apply_btn = Button::with_label("Apply");
    apply_btn.add_css_class("suggested-action");
    header.pack_end(&apply_btn);

    main_box.append(&header);

    // Content
    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let label = Label::new(Some(&format!(
        "Current priority (nice value): {}\n\nSelect new priority:",
        current_priority
    )));
    label.set_halign(gtk4::Align::Start);
    content.append(&label);

    let priority_box = GtkBox::new(Orientation::Vertical, 4);
    let mut first_button: Option<CheckButton> = None;
    let buttons: Rc<RefCell<Vec<(CheckButton, Priority)>>> = Rc::new(RefCell::new(Vec::new()));

    for priority in Priority::all() {
        let radio = CheckButton::with_label(priority.as_str());

        if let Some(ref first) = first_button {
            radio.set_group(Some(first));
        } else {
            first_button = Some(radio.clone());
        }

        // Select current priority
        if priority.nice_value() == current_priority {
            radio.set_active(true);
        }

        priority_box.append(&radio);
        buttons.borrow_mut().push((radio, *priority));
    }

    content.append(&priority_box);

    let note = Label::new(Some(
        "Note: Setting higher priority (lower nice value) may require root privileges.",
    ));
    note.add_css_class("dim-label");
    note.set_halign(gtk4::Align::Start);
    note.set_wrap(true);
    content.append(&note);

    main_box.append(&content);
    dialog.set_content(Some(&main_box));

    // Cancel button closes dialog
    let dialog_weak = dialog.downgrade();
    cancel_btn.connect_clicked(move |_| {
        if let Some(d) = dialog_weak.upgrade() {
            d.close();
        }
    });

    // Apply button
    let buttons_clone = buttons.clone();
    let parent_weak = parent.downgrade();
    let dialog_weak = dialog.downgrade();
    apply_btn.connect_clicked(move |_| {
        for (radio, priority) in buttons_clone.borrow().iter() {
            if radio.is_active() {
                if let Err(e) = set_priority(pid, *priority) {
                    if let Some(parent) = parent_weak.upgrade() {
                        show_error_dialog(&parent, "Failed to set priority", &e.to_string());
                    }
                }
                break;
            }
        }

        if let Some(d) = dialog_weak.upgrade() {
            d.close();
        }
    });

    dialog.present();
}

/// Show a simple error dialog
fn show_error_dialog(parent: &impl IsA<Window>, title: &str, message: &str) {
    let dialog = adw::MessageDialog::builder()
        .transient_for(parent)
        .heading(title)
        .body(message)
        .build();

    dialog.add_response("ok", "OK");
    dialog.set_default_response(Some("ok"));
    dialog.present();
}
