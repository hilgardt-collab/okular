//! Isolated process window for detailed monitoring of a single process

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, Dialog, Label, Orientation, ResponseType,
    ScrolledWindow, Separator, Window,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::prelude::MessageDialogExt;
use glib::ControlFlow;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::detail_view::{DetailView, ProcessDetails};
use crate::monitor::SystemMonitor;
use crate::process_actions::{
    self, get_cpu_affinity, get_cpu_count, kill_process, set_cpu_affinity,
    set_priority, Priority,
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
        .title(&format!("{} (PID: {}) - Ocular", name, pid))
        .default_width(600)
        .default_height(700)
        .transient_for(parent)
        .build();

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    // Header bar
    let header = adw::HeaderBar::new();
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
        } else {
            // Process will end, timer will close window
        }
    });

    // Connect Force Kill button
    let window_weak_clone = window_weak.clone();
    let source_id_clone2 = source_id.clone();
    kill_btn.connect_clicked(move |_| {
        if let Err(e) = kill_process(pid, true) {
            if let Some(win) = window_weak_clone.upgrade() {
                show_error_dialog(&win, "Failed to kill process", &e.to_string());
            }
        } else {
            // Process killed, close window immediately
            if let Some(win) = window_weak_clone.upgrade() {
                if let Some(id) = source_id_clone2.borrow_mut().take() {
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

/// Show CPU affinity dialog
fn show_affinity_dialog(parent: &impl IsA<Window>, pid: u32) {
    let cpu_count = get_cpu_count();
    let current_affinity = get_cpu_affinity(pid).unwrap_or_else(|_| vec![true; cpu_count]);

    let dialog = Dialog::builder()
        .title("Set CPU Affinity")
        .transient_for(parent)
        .modal(true)
        .build();

    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Apply", ResponseType::Apply);

    let content = dialog.content_area();
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_spacing(8);

    let label = Label::new(Some("Select which CPU cores this process can run on:"));
    label.set_halign(gtk4::Align::Start);
    content.append(&label);

    // Create scrolled window for many CPUs
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .min_content_height(200)
        .max_content_height(400)
        .build();

    let cpu_box = GtkBox::new(Orientation::Vertical, 4);
    let checkboxes: Rc<RefCell<Vec<CheckButton>>> = Rc::new(RefCell::new(Vec::new()));

    for i in 0..cpu_count {
        let checkbox = CheckButton::with_label(&format!("CPU {}", i));
        checkbox.set_active(current_affinity.get(i).copied().unwrap_or(true));
        cpu_box.append(&checkbox);
        checkboxes.borrow_mut().push(checkbox);
    }

    scrolled.set_child(Some(&cpu_box));
    content.append(&scrolled);

    // Select All / Deselect All buttons
    let btn_box = GtkBox::new(Orientation::Horizontal, 8);
    btn_box.set_halign(gtk4::Align::Center);

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

    let checkboxes_clone = checkboxes.clone();
    let parent_weak = parent.downgrade();
    dialog.connect_response(move |dialog: &Dialog, response| {
        if response == ResponseType::Apply {
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
        }
        dialog.close();
    });

    dialog.present();
}

/// Show priority dialog
fn show_priority_dialog(parent: &impl IsA<Window>, pid: u32) {
    let current_priority = process_actions::get_priority(pid).unwrap_or(0);

    let dialog = Dialog::builder()
        .title("Set Process Priority")
        .transient_for(parent)
        .modal(true)
        .build();

    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Apply", ResponseType::Apply);

    let content = dialog.content_area();
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_spacing(8);

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

    let buttons_clone = buttons.clone();
    let parent_weak = parent.downgrade();
    dialog.connect_response(move |dialog: &Dialog, response| {
        if response == ResponseType::Apply {
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
        }
        dialog.close();
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
