//! Context menu for process list right-click actions

use gtk4::prelude::*;
use gtk4::gdk::Display;
use gtk4::{
    gio, CheckButton, Dialog, Label, Orientation, ResponseType,
    ScrolledWindow, Box as GtkBox,
};
use libadwaita as adw;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::monitor::SystemMonitor;
use crate::process_actions::{
    self, get_cpu_affinity, get_cpu_count, kill_process, set_cpu_affinity,
    set_priority, Priority,
};
use crate::process_window;

/// Create the context menu for a process
pub fn create_process_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    // Open in Window
    menu.append(Some("Open in Window"), Some("process.open-window"));

    // Separator
    menu.append(None, None);

    // End Process submenu
    let end_menu = gio::Menu::new();
    end_menu.append(Some("End Process (SIGTERM)"), Some("process.end"));
    end_menu.append(Some("Force Kill (SIGKILL)"), Some("process.kill"));
    end_menu.append(Some("Pause (SIGSTOP)"), Some("process.stop"));
    end_menu.append(Some("Resume (SIGCONT)"), Some("process.cont"));
    menu.append_submenu(Some("Send Signal"), &end_menu);

    // Separator
    menu.append(None, None);

    // CPU Affinity
    menu.append(Some("Set CPU Affinity..."), Some("process.affinity"));

    // Priority
    menu.append(Some("Set Priority..."), Some("process.priority"));

    // Separator
    menu.append(None, None);

    // Copy options
    menu.append(Some("Copy PID"), Some("process.copy-pid"));
    menu.append(Some("Copy Command"), Some("process.copy-command"));

    menu
}

/// Set up actions for the process context menu
pub fn setup_process_actions(
    widget: &impl IsA<gtk4::Widget>,
    get_selected: impl Fn() -> Option<(u32, String)> + 'static,
    get_window: impl Fn() -> Option<gtk4::Window> + 'static,
    monitor: Rc<RefCell<SystemMonitor>>,
) {
    let action_group = gio::SimpleActionGroup::new();

    // Open in Window action
    let get_selected_clone = Rc::new(get_selected);
    let get_window_clone = Rc::new(get_window);
    let monitor_clone = monitor.clone();

    let get_sel = get_selected_clone.clone();
    let get_win = get_window_clone.clone();
    let mon = monitor_clone.clone();
    let open_action = gio::SimpleAction::new("open-window", None);
    open_action.connect_activate(move |_, _| {
        if let (Some((pid, name)), Some(window)) = (get_sel(), get_win()) {
            process_window::open_process_window(&window, pid, &name, mon.clone());
        }
    });
    action_group.add_action(&open_action);

    // End Process action (SIGTERM)
    let get_sel = get_selected_clone.clone();
    let get_win = get_window_clone.clone();
    let end_action = gio::SimpleAction::new("end", None);
    end_action.connect_activate(move |_, _| {
        if let Some((pid, _)) = get_sel() {
            if let Err(e) = kill_process(pid, false) {
                if let Some(win) = get_win() {
                    show_error(&win, "Failed to end process", &e.to_string());
                }
            }
        }
    });
    action_group.add_action(&end_action);

    // Kill action (SIGKILL)
    let get_sel = get_selected_clone.clone();
    let get_win = get_window_clone.clone();
    let kill_action = gio::SimpleAction::new("kill", None);
    kill_action.connect_activate(move |_, _| {
        if let Some((pid, _)) = get_sel() {
            if let Err(e) = kill_process(pid, true) {
                if let Some(win) = get_win() {
                    show_error(&win, "Failed to kill process", &e.to_string());
                }
            }
        }
    });
    action_group.add_action(&kill_action);

    // Stop action (SIGSTOP)
    let get_sel = get_selected_clone.clone();
    let get_win = get_window_clone.clone();
    let stop_action = gio::SimpleAction::new("stop", None);
    stop_action.connect_activate(move |_, _| {
        if let Some((pid, _)) = get_sel() {
            if let Err(e) = process_actions::send_signal(pid, process_actions::Signal::Stop) {
                if let Some(win) = get_win() {
                    show_error(&win, "Failed to pause process", &e.to_string());
                }
            }
        }
    });
    action_group.add_action(&stop_action);

    // Continue action (SIGCONT)
    let get_sel = get_selected_clone.clone();
    let get_win = get_window_clone.clone();
    let cont_action = gio::SimpleAction::new("cont", None);
    cont_action.connect_activate(move |_, _| {
        if let Some((pid, _)) = get_sel() {
            if let Err(e) = process_actions::send_signal(pid, process_actions::Signal::Cont) {
                if let Some(win) = get_win() {
                    show_error(&win, "Failed to resume process", &e.to_string());
                }
            }
        }
    });
    action_group.add_action(&cont_action);

    // CPU Affinity action
    let get_sel = get_selected_clone.clone();
    let get_win = get_window_clone.clone();
    let affinity_action = gio::SimpleAction::new("affinity", None);
    affinity_action.connect_activate(move |_, _| {
        if let (Some((pid, _)), Some(win)) = (get_sel(), get_win()) {
            show_affinity_dialog(&win, pid);
        }
    });
    action_group.add_action(&affinity_action);

    // Priority action
    let get_sel = get_selected_clone.clone();
    let get_win = get_window_clone.clone();
    let priority_action = gio::SimpleAction::new("priority", None);
    priority_action.connect_activate(move |_, _| {
        if let (Some((pid, _)), Some(win)) = (get_sel(), get_win()) {
            show_priority_dialog(&win, pid);
        }
    });
    action_group.add_action(&priority_action);

    // Copy PID action
    let get_sel = get_selected_clone.clone();
    let copy_pid_action = gio::SimpleAction::new("copy-pid", None);
    copy_pid_action.connect_activate(move |_, _| {
        if let Some((pid, _)) = get_sel() {
            if let Some(display) = Display::default() {
                let clipboard = display.clipboard();
                clipboard.set_text(&pid.to_string());
            }
        }
    });
    action_group.add_action(&copy_pid_action);

    // Copy Command action
    let get_sel = get_selected_clone.clone();
    let copy_cmd_action = gio::SimpleAction::new("copy-command", None);
    copy_cmd_action.connect_activate(move |_, _| {
        if let Some((pid, _)) = get_sel() {
            if let Some(cmd) = process_actions::get_command_line(pid) {
                if let Some(display) = Display::default() {
                    let clipboard = display.clipboard();
                    clipboard.set_text(&cmd);
                }
            }
        }
    });
    action_group.add_action(&copy_cmd_action);

    widget.insert_action_group("process", Some(&action_group));
}

/// Show CPU affinity dialog
fn show_affinity_dialog(parent: &gtk4::Window, pid: u32) {
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

    let label = Label::new(Some(&format!(
        "Select which CPU cores process {} can run on:",
        pid
    )));
    label.set_halign(gtk4::Align::Start);
    content.append(&label);

    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .min_content_height(150)
        .max_content_height(300)
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

    let btn_box = GtkBox::new(Orientation::Horizontal, 8);
    btn_box.set_halign(gtk4::Align::Center);

    let select_all = gtk4::Button::with_label("Select All");
    let checkboxes_clone = checkboxes.clone();
    select_all.connect_clicked(move |_| {
        for cb in checkboxes_clone.borrow().iter() {
            cb.set_active(true);
        }
    });
    btn_box.append(&select_all);

    let deselect_all = gtk4::Button::with_label("Deselect All");
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
                    show_error(&parent, "Invalid Selection", "You must select at least one CPU.");
                }
            } else if let Err(e) = set_cpu_affinity(pid, &selected_cpus) {
                if let Some(parent) = parent_weak.upgrade() {
                    show_error(&parent, "Failed to set CPU affinity", &e.to_string());
                }
            }
        }
        dialog.close();
    });

    dialog.present();
}

/// Show priority dialog
fn show_priority_dialog(parent: &gtk4::Window, pid: u32) {
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

        if priority.nice_value() == current_priority {
            radio.set_active(true);
        }

        priority_box.append(&radio);
        buttons.borrow_mut().push((radio, *priority));
    }

    content.append(&priority_box);

    let note = Label::new(Some(
        "Note: Higher priority (lower nice value) may require root privileges.",
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
                            show_error(&parent, "Failed to set priority", &e.to_string());
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

/// Show error dialog
fn show_error(parent: &gtk4::Window, title: &str, message: &str) {
    let dialog = adw::MessageDialog::builder()
        .transient_for(parent)
        .heading(title)
        .body(message)
        .build();

    dialog.add_response("ok", "OK");
    dialog.set_default_response(Some("ok"));
    dialog.present();
}
