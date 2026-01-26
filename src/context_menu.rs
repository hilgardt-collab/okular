//! Context menu for process list right-click actions

use gtk4::prelude::*;
use gtk4::gdk::Display;
use gtk4::{
    gio, CheckButton, Label, Orientation,
    ScrolledWindow, Box as GtkBox, Button,
};
use libadwaita as adw;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::monitor::SystemMonitor;
use crate::process_actions::{
    self, get_cpu_affinity, get_cpu_core_info, kill_process, set_cpu_affinity,
    set_priority, Priority, CoreType,
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

/// Show CPU affinity dialog with core type information
fn show_affinity_dialog(parent: &gtk4::Window, pid: u32) {
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

    let label = Label::new(Some(&format!(
        "Select which CPU cores process {} can run on:",
        pid
    )));
    label.set_halign(gtk4::Align::Start);
    content.append(&label);

    // Check if we have any special core types
    let has_special_cores = core_info.iter().any(|c| c.core_type != CoreType::Standard);
    if has_special_cores {
        let legend = create_core_type_legend(&core_info);
        content.append(&legend);
    }

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
                show_error(&parent, "Invalid Selection", "You must select at least one CPU.");
            }
        } else if let Err(e) = set_cpu_affinity(pid, &selected_cpus) {
            if let Some(parent) = parent_weak.upgrade() {
                show_error(&parent, "Failed to set CPU affinity", &e.to_string());
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
fn show_priority_dialog(parent: &gtk4::Window, pid: u32) {
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
                        show_error(&parent, "Failed to set priority", &e.to_string());
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
