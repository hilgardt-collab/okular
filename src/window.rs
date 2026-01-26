use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Orientation, Paned, SearchEntry, DropDown, StringList, TreeListRow};
use libadwaita as adw;
use adw::prelude::*;
use glib::ControlFlow;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::monitor::SystemMonitor;
use crate::process_list::ProcessListView;
use crate::detail_view::{DetailView, ProcessDetails};

const UPDATE_INTERVAL_MS: u64 = 2000; // 2 seconds

pub struct OcularWindow;

impl OcularWindow {
    /// Build and return the main application window
    pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
        // Create window
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Ocular - Process Monitor")
            .default_width(1200)
            .default_height(700)
            .build();

        // Main layout
        let main_box = GtkBox::new(Orientation::Vertical, 0);

        // Header bar with search and history duration
        let (header_bar, search_entry, history_dropdown) = Self::create_header_bar();
        main_box.append(&header_bar);

        // Create the monitor
        let monitor = Rc::new(RefCell::new(SystemMonitor::new()));

        // Create views
        let process_list = Rc::new(ProcessListView::new());
        let detail_view = Rc::new(DetailView::new());

        // Paned view for list and detail
        let paned = Paned::new(Orientation::Horizontal);
        paned.set_start_child(Some(&process_list.widget));
        paned.set_end_child(Some(&detail_view.widget));
        paned.set_position(700);
        paned.set_shrink_start_child(false);
        paned.set_shrink_end_child(false);
        paned.set_vexpand(true);
        main_box.append(&paned);

        // Status bar
        let status_bar = GtkBox::new(Orientation::Horizontal, 8);
        status_bar.set_margin_start(8);
        status_bar.set_margin_end(8);
        status_bar.set_margin_top(4);
        status_bar.set_margin_bottom(4);
        let status_label = gtk4::Label::new(Some("Monitoring processes..."));
        status_label.set_halign(gtk4::Align::Start);
        status_bar.append(&status_label);
        main_box.append(&status_bar);

        window.set_content(Some(&main_box));

        // Track selected process
        let selected_pid: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));

        // Connect search
        let process_list_clone = process_list.clone();
        search_entry.connect_search_changed(move |entry| {
            let text = entry.text();
            process_list_clone.set_filter(&text);
        });

        // Connect history duration dropdown
        let monitor_clone = monitor.clone();
        history_dropdown.connect_selected_notify(move |dropdown| {
            let idx = dropdown.selected();
            // Convert to samples (at 2-second intervals)
            let max_samples = match idx {
                0 => 30,   // 1 min
                1 => 60,   // 2 min
                2 => 150,  // 5 min
                3 => 300,  // 10 min
                4 => 900,  // 30 min
                _ => {
                    eprintln!("Warning: Unexpected dropdown index {}, defaulting to 2 min", idx);
                    60
                }
            };
            monitor_clone.borrow_mut().set_max_samples(max_samples);
        });

        // Connect selection change for detail view
        let detail_view_clone = detail_view.clone();
        let monitor_clone = monitor.clone();
        let selected_pid_clone = selected_pid.clone();
        let updating_flag = process_list.updating.clone();
        process_list.selection_model().connect_selection_changed(move |selection, _, _| {
            // Skip if we're in the middle of a programmatic update
            if *updating_flag.borrow() {
                return;
            }

            if let Some(obj) = selection.selected_item() {
                // The item is a TreeListRow, we need to get the ProcessObject from it
                if let Some(row) = obj.downcast_ref::<TreeListRow>() {
                    if let Some(proc_obj) = row.item().and_downcast::<crate::process_list::ProcessObject>() {
                        let pid = proc_obj.pid();
                        let name = proc_obj.name();
                        *selected_pid_clone.borrow_mut() = Some(pid);
                        let monitor = monitor_clone.borrow();
                        let history = monitor.get_history(pid);
                        let process_details = ProcessDetails::from_pid(pid);
                        detail_view_clone.update(&name, pid, history, process_details.as_ref());
                    }
                }
            } else {
                *selected_pid_clone.borrow_mut() = None;
                detail_view_clone.clear();
            }
        });

        // Initial data load
        {
            let mut mon = monitor.borrow_mut();
            let processes = mon.refresh();
            process_list.update(&processes);
        }

        // Set up periodic refresh using glib::timeout_add_local
        let process_list_clone = process_list.clone();
        let detail_view_clone = detail_view.clone();
        let monitor_clone = monitor.clone();
        let selected_pid_clone = selected_pid.clone();
        let window_weak = window.downgrade();

        let source_id = glib::timeout_add_local(Duration::from_millis(UPDATE_INTERVAL_MS), move || {
            // Check if window still exists
            if window_weak.upgrade().is_none() {
                return ControlFlow::Break;
            }

            // Refresh process data
            let mut mon = monitor_clone.borrow_mut();
            let processes = mon.refresh();
            process_list_clone.update(&processes);

            // Update detail view if a process is selected
            let current_pid = *selected_pid_clone.borrow();
            if let Some(pid) = current_pid {
                if let Some(proc) = processes.iter().find(|p| p.pid == pid) {
                    let history = mon.get_history(pid);
                    let process_details = ProcessDetails::from_pid(pid);
                    detail_view_clone.update(&proc.name, pid, history, process_details.as_ref());
                } else {
                    // Process no longer exists - clear selection
                    #[cfg(debug_assertions)]
                    eprintln!("Debug: Selected process (PID {}) no longer exists", pid);
                    *selected_pid_clone.borrow_mut() = None;
                    detail_view_clone.clear();
                }
            }

            ControlFlow::Continue
        });

        // Store source ID for cleanup
        let source_id = Rc::new(RefCell::new(Some(source_id)));

        // Clean up timeout on window close
        let source_id_clone = source_id.clone();
        window.connect_close_request(move |_| {
            if let Some(id) = source_id_clone.borrow_mut().take() {
                id.remove();
            }
            glib::Propagation::Proceed
        });

        window
    }

    fn create_header_bar() -> (adw::HeaderBar, SearchEntry, DropDown) {
        let header = adw::HeaderBar::new();

        // Search entry
        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search processes..."));
        search_entry.set_width_chars(30);
        header.pack_start(&search_entry);

        // History duration dropdown
        let history_options = StringList::new(&[
            "1 min",
            "2 min",
            "5 min",
            "10 min",
            "30 min",
        ]);
        let history_dropdown = DropDown::new(Some(history_options), gtk4::Expression::NONE);
        history_dropdown.set_selected(1); // Default to 2 minutes

        let history_box = GtkBox::new(Orientation::Horizontal, 8);
        let history_label = gtk4::Label::new(Some("History:"));
        history_box.append(&history_label);
        history_box.append(&history_dropdown);
        header.pack_end(&history_box);

        (header, search_entry, history_dropdown)
    }
}
