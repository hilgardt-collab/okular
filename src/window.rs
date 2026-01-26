use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Orientation, SearchEntry};
use libadwaita as adw;
use adw::prelude::*;
use glib::ControlFlow;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::context_menu;
use crate::monitor::SystemMonitor;
use crate::process_list::{ProcessListView, ProcessObject};
use crate::process_window;

const UPDATE_INTERVAL_MS: u64 = 2000; // 2 seconds

pub struct OcularWindow;

impl OcularWindow {
    /// Build and return the main application window
    pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
        // Create window
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("okular")
            .default_width(1200)
            .default_height(700)
            .build();

        // Main layout
        let main_box = GtkBox::new(Orientation::Vertical, 0);

        // Header bar with search
        let (header_bar, search_entry) = Self::create_header_bar();
        main_box.append(&header_bar);

        // Create the monitor
        let monitor = Rc::new(RefCell::new(SystemMonitor::new()));

        // Create process list view
        let process_list = Rc::new(ProcessListView::new());

        // Set up context menu actions for process list
        let process_list_clone = process_list.clone();
        let window_clone = window.clone();
        let monitor_clone = monitor.clone();
        context_menu::setup_process_actions(
            process_list.column_view(),
            move || process_list_clone.get_selected_process(),
            move || Some(window_clone.clone().upcast::<gtk4::Window>()),
            monitor_clone,
        );

        // Set up double-click to open process window
        let window_clone = window.clone();
        let monitor_clone = monitor.clone();
        process_list.connect_double_click(move |pid, name| {
            process_window::open_process_window(
                &window_clone,
                pid,
                &name,
                monitor_clone.clone(),
            );
        });

        // Add process list directly (no paned view)
        process_list.widget.set_vexpand(true);
        main_box.append(&process_list.widget);

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

        // Connect selection change to track selected PID
        let selected_pid_clone = selected_pid.clone();
        let updating_flag = process_list.updating.clone();
        process_list.selection_model().connect_selection_changed(move |selection, _, _| {
            // Skip if we're in the middle of a programmatic update
            if *updating_flag.borrow() {
                return;
            }

            if let Some(obj) = selection.selected_item() {
                if let Some(proc_obj) = obj.downcast_ref::<ProcessObject>() {
                    *selected_pid_clone.borrow_mut() = Some(proc_obj.pid());
                }
            } else {
                *selected_pid_clone.borrow_mut() = None;
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

            // Clear selected PID if process no longer exists
            let current_pid = *selected_pid_clone.borrow();
            if let Some(pid) = current_pid {
                if !processes.iter().any(|p| p.pid == pid) {
                    *selected_pid_clone.borrow_mut() = None;
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

    fn create_header_bar() -> (adw::HeaderBar, SearchEntry) {
        let header = adw::HeaderBar::new();

        // Search entry
        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search processes..."));
        search_entry.set_width_chars(30);
        header.pack_start(&search_entry);

        (header, search_entry)
    }
}
