use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{
    ColumnView, ColumnViewColumn, GestureClick, PopoverMenu, ScrolledWindow,
    SignalListItemFactory, ListItem, Label, SortListModel, CustomSorter, CustomFilter,
    FilterListModel, SingleSelection, Ordering as GtkOrdering, SortType,
};
use glib::Object;
use std::cell::RefCell;
use std::rc::Rc;

use crate::context_menu;
use crate::monitor::{ProcessInfo, format_bytes};

// GObject subclass to hold process data
mod imp {
    use super::*;
    use glib::subclass::prelude::*;
    use std::cell::Cell;

    #[derive(Default)]
    pub struct ProcessObject {
        pub pid: Cell<u32>,
        pub name: RefCell<String>,
        pub cpu_percent: Cell<f32>,
        pub memory_bytes: Cell<u64>,
        pub disk_read_bytes: Cell<u64>,
        pub disk_write_bytes: Cell<u64>,
        pub gpu_percent: Cell<f32>, // -1.0 means N/A
        pub child_count: Cell<usize>,
        pub is_group: Cell<bool>,
        pub children: RefCell<Vec<ProcessInfo>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProcessObject {
        const NAME: &'static str = "OcularProcessObject";
        type Type = super::ProcessObject;
    }

    impl ObjectImpl for ProcessObject {}
}

glib::wrapper! {
    pub struct ProcessObject(ObjectSubclass<imp::ProcessObject>);
}

impl ProcessObject {
    pub fn new(info: &ProcessInfo) -> Self {
        let obj: Self = Object::builder().build();
        obj.set_from_info(info);
        obj
    }

    pub fn set_from_info(&self, info: &ProcessInfo) {
        let imp = self.imp();
        imp.pid.set(info.pid);
        imp.name.replace(info.name.clone());
        // For groups, show total; for individuals, show own value
        imp.cpu_percent.set(info.total_cpu());
        imp.memory_bytes.set(info.total_memory());
        imp.disk_read_bytes.set(info.disk_read_bytes);
        imp.disk_write_bytes.set(info.disk_write_bytes);
        imp.gpu_percent.set(info.gpu_percent.unwrap_or(-1.0));
        imp.child_count.set(info.children.len());
        imp.is_group.set(info.is_group);
        imp.children.replace(info.children.clone());
    }

    pub fn pid(&self) -> u32 {
        self.imp().pid.get()
    }

    pub fn name(&self) -> String {
        self.imp().name.borrow().clone()
    }

    pub fn cpu_percent(&self) -> f32 {
        self.imp().cpu_percent.get()
    }

    pub fn memory_bytes(&self) -> u64 {
        self.imp().memory_bytes.get()
    }

    pub fn disk_read_bytes(&self) -> u64 {
        self.imp().disk_read_bytes.get()
    }

    pub fn disk_write_bytes(&self) -> u64 {
        self.imp().disk_write_bytes.get()
    }

    pub fn gpu_percent(&self) -> f32 {
        self.imp().gpu_percent.get()
    }

    pub fn child_count(&self) -> usize {
        self.imp().child_count.get()
    }

    pub fn is_group(&self) -> bool {
        self.imp().is_group.get()
    }

    pub fn children(&self) -> Vec<ProcessInfo> {
        self.imp().children.borrow().clone()
    }
}

/// Process list widget
pub struct ProcessListView {
    pub widget: ScrolledWindow,
    store: gtk4::gio::ListStore,
    #[allow(dead_code)]
    sort_model: SortListModel,
    filter_model: FilterListModel,
    selection: SingleSelection,
    filter_text: Rc<RefCell<String>>,
    column_view: ColumnView,
    /// Flag to indicate we're updating programmatically (to avoid callback recursion)
    pub updating: Rc<RefCell<bool>>,
    /// Context menu popover (kept alive for right-click)
    #[allow(dead_code)]
    context_menu: PopoverMenu,
}

impl ProcessListView {
    pub fn new() -> Self {
        // Create the list store for process objects
        let store = gtk4::gio::ListStore::new::<ProcessObject>();

        // Create filter model (flat list, no tree hierarchy)
        let filter = CustomFilter::new(|_| true);
        let filter_model = FilterListModel::new(Some(store.clone()), Some(filter.clone()));

        // Create sort model
        let sort_model = SortListModel::new(Some(filter_model.clone()), None::<gtk4::Sorter>);

        // Create selection model
        let selection = SingleSelection::new(Some(sort_model.clone()));

        // Create column view
        let column_view = ColumnView::new(Some(selection.clone()));
        column_view.set_show_column_separators(true);
        column_view.set_show_row_separators(true);
        column_view.set_reorderable(false);

        // Enable sorting
        sort_model.set_sorter(column_view.sorter().as_ref());
        column_view.connect_sorter_notify(glib::clone!(
            #[weak] sort_model,
            move |cv| {
                sort_model.set_sorter(cv.sorter().as_ref());
            }
        ));

        let filter_text = Rc::new(RefCell::new(String::new()));

        // Create columns with sorters
        Self::create_columns(&column_view);

        // Set default sort to CPU descending
        if let Some(col) = column_view.columns().item(2) {
            let col = col.downcast::<ColumnViewColumn>()
                .expect("Column 2 should be a ColumnViewColumn");
            column_view.sort_by_column(Some(&col), SortType::Descending);
        }

        // Create context menu
        let menu = context_menu::create_process_menu();
        let context_menu = PopoverMenu::from_model(Some(&menu));
        context_menu.set_parent(&column_view);
        context_menu.set_has_arrow(false);

        // Set up right-click gesture
        let gesture = GestureClick::new();
        gesture.set_button(3); // Right click

        let context_menu_weak = context_menu.downgrade();
        gesture.connect_pressed(move |gesture, _n_press, x, y| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);

            if let Some(menu) = context_menu_weak.upgrade() {
                // Position menu at click location
                menu.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
                    x as i32,
                    y as i32,
                    1,
                    1,
                )));
                menu.popup();
            }
        });
        column_view.add_controller(gesture);

        // Scrolled window
        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .vexpand(true)
            .hexpand(true)
            .child(&column_view)
            .build();

        Self {
            widget: scrolled,
            store,
            sort_model,
            filter_model,
            selection,
            filter_text,
            column_view,
            updating: Rc::new(RefCell::new(false)),
            context_menu,
        }
    }

    fn create_columns(column_view: &ColumnView) {
        // Name column (flat list with thread count)
        let factory = SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let label = Label::new(None);
            label.set_halign(gtk4::Align::Start);
            label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            item.set_child(Some(&label));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let obj = item.item().and_downcast::<ProcessObject>()
                .expect("Item should contain a ProcessObject");
            let label = item.child().and_downcast::<Label>()
                .expect("Item child should be a Label");

            let name = obj.name();
            let child_count = obj.child_count();
            if child_count > 0 {
                // Show thread count in parentheses
                label.set_label(&format!("{} ({} threads)", name, child_count));
            } else {
                label.set_label(&name);
            }
        });
        let sorter = CustomSorter::new(|a, b| {
            let a = a.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            let b = b.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            match a.name().to_lowercase().cmp(&b.name().to_lowercase()) {
                std::cmp::Ordering::Less => GtkOrdering::Smaller,
                std::cmp::Ordering::Equal => GtkOrdering::Equal,
                std::cmp::Ordering::Greater => GtkOrdering::Larger,
            }
        });
        let col = ColumnViewColumn::new(Some("Name"), Some(factory));
        col.set_sorter(Some(&sorter));
        col.set_resizable(true);
        col.set_expand(true);
        column_view.append_column(&col);

        // PID column
        let factory = SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let label = Label::new(None);
            label.set_halign(gtk4::Align::End);
            item.set_child(Some(&label));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let obj = item.item().and_downcast::<ProcessObject>()
                .expect("Item should contain a ProcessObject");
            let label = item.child().and_downcast::<Label>()
                .expect("Item child should be a Label");
            label.set_label(&obj.pid().to_string());
        });
        let sorter = CustomSorter::new(|a, b| {
            let a = a.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            let b = b.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            match a.pid().cmp(&b.pid()) {
                std::cmp::Ordering::Less => GtkOrdering::Smaller,
                std::cmp::Ordering::Equal => GtkOrdering::Equal,
                std::cmp::Ordering::Greater => GtkOrdering::Larger,
            }
        });
        let col = ColumnViewColumn::new(Some("PID"), Some(factory));
        col.set_sorter(Some(&sorter));
        col.set_resizable(true);
        col.set_fixed_width(80);
        column_view.append_column(&col);

        // CPU% column
        let factory = SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let label = Label::new(None);
            label.set_halign(gtk4::Align::End);
            item.set_child(Some(&label));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let obj = item.item().and_downcast::<ProcessObject>()
                .expect("Item should contain a ProcessObject");
            let label = item.child().and_downcast::<Label>()
                .expect("Item child should be a Label");
            label.set_label(&format!("{:.1}%", obj.cpu_percent()));
        });
        let sorter = CustomSorter::new(|a, b| {
            let a = a.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            let b = b.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            // Handle NaN by treating it as less than any valid number
            let a_cpu = a.cpu_percent();
            let b_cpu = b.cpu_percent();
            if a_cpu.is_nan() && b_cpu.is_nan() {
                GtkOrdering::Equal
            } else if a_cpu.is_nan() {
                GtkOrdering::Smaller
            } else if b_cpu.is_nan() {
                GtkOrdering::Larger
            } else {
                match a_cpu.partial_cmp(&b_cpu).unwrap_or(std::cmp::Ordering::Equal) {
                    std::cmp::Ordering::Less => GtkOrdering::Smaller,
                    std::cmp::Ordering::Equal => GtkOrdering::Equal,
                    std::cmp::Ordering::Greater => GtkOrdering::Larger,
                }
            }
        });
        let col = ColumnViewColumn::new(Some("CPU %"), Some(factory));
        col.set_sorter(Some(&sorter));
        col.set_resizable(true);
        col.set_fixed_width(80);
        column_view.append_column(&col);

        // Memory column
        let factory = SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let label = Label::new(None);
            label.set_halign(gtk4::Align::End);
            item.set_child(Some(&label));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let obj = item.item().and_downcast::<ProcessObject>()
                .expect("Item should contain a ProcessObject");
            let label = item.child().and_downcast::<Label>()
                .expect("Item child should be a Label");
            label.set_label(&format_bytes(obj.memory_bytes()));
        });
        let sorter = CustomSorter::new(|a, b| {
            let a = a.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            let b = b.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            match a.memory_bytes().cmp(&b.memory_bytes()) {
                std::cmp::Ordering::Less => GtkOrdering::Smaller,
                std::cmp::Ordering::Equal => GtkOrdering::Equal,
                std::cmp::Ordering::Greater => GtkOrdering::Larger,
            }
        });
        let col = ColumnViewColumn::new(Some("Memory"), Some(factory));
        col.set_sorter(Some(&sorter));
        col.set_resizable(true);
        col.set_fixed_width(100);
        column_view.append_column(&col);

        // Disk I/O column
        let factory = SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let label = Label::new(None);
            label.set_halign(gtk4::Align::End);
            item.set_child(Some(&label));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let obj = item.item().and_downcast::<ProcessObject>()
                .expect("Item should contain a ProcessObject");
            let label = item.child().and_downcast::<Label>()
                .expect("Item child should be a Label");
            let total = obj.disk_read_bytes() + obj.disk_write_bytes();
            label.set_label(&format_bytes(total));
        });
        let sorter = CustomSorter::new(|a, b| {
            let a = a.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            let b = b.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            let a_total = a.disk_read_bytes() + a.disk_write_bytes();
            let b_total = b.disk_read_bytes() + b.disk_write_bytes();
            match a_total.cmp(&b_total) {
                std::cmp::Ordering::Less => GtkOrdering::Smaller,
                std::cmp::Ordering::Equal => GtkOrdering::Equal,
                std::cmp::Ordering::Greater => GtkOrdering::Larger,
            }
        });
        let col = ColumnViewColumn::new(Some("Disk I/O"), Some(factory));
        col.set_sorter(Some(&sorter));
        col.set_resizable(true);
        col.set_fixed_width(100);
        column_view.append_column(&col);

        // GPU% column
        let factory = SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let label = Label::new(None);
            label.set_halign(gtk4::Align::End);
            item.set_child(Some(&label));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<ListItem>()
                .expect("Factory item should be a ListItem");
            let obj = item.item().and_downcast::<ProcessObject>()
                .expect("Item should contain a ProcessObject");
            let label = item.child().and_downcast::<Label>()
                .expect("Item child should be a Label");
            let gpu = obj.gpu_percent();
            if gpu < 0.0 {
                label.set_label("-");
            } else {
                label.set_label(&format!("{:.1}%", gpu));
            }
        });
        let sorter = CustomSorter::new(|a, b| {
            let a = a.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            let b = b.downcast_ref::<ProcessObject>()
                .expect("Sorter item should be a ProcessObject");
            // Handle NaN and negative values (used for N/A)
            let a_gpu = a.gpu_percent();
            let b_gpu = b.gpu_percent();
            if (a_gpu.is_nan() || a_gpu < 0.0) && (b_gpu.is_nan() || b_gpu < 0.0) {
                GtkOrdering::Equal
            } else if a_gpu.is_nan() || a_gpu < 0.0 {
                GtkOrdering::Smaller
            } else if b_gpu.is_nan() || b_gpu < 0.0 {
                GtkOrdering::Larger
            } else {
                match a_gpu.partial_cmp(&b_gpu).unwrap_or(std::cmp::Ordering::Equal) {
                    std::cmp::Ordering::Less => GtkOrdering::Smaller,
                    std::cmp::Ordering::Equal => GtkOrdering::Equal,
                    std::cmp::Ordering::Greater => GtkOrdering::Larger,
                }
            }
        });
        let col = ColumnViewColumn::new(Some("GPU %"), Some(factory));
        col.set_sorter(Some(&sorter));
        col.set_resizable(true);
        col.set_fixed_width(80);
        column_view.append_column(&col);
    }

    /// Update the process list with new data
    pub fn update(&self, processes: &[ProcessInfo]) {
        // Set updating flag to prevent selection callback from firing
        *self.updating.borrow_mut() = true;

        // Save current selection
        let selected_pid = self.selection
            .selected_item()
            .and_then(|obj| obj.downcast::<ProcessObject>().ok())
            .map(|p| p.pid());

        // Clear and repopulate
        self.store.remove_all();
        for proc in processes {
            self.store.append(&ProcessObject::new(proc));
        }

        // Restore selection if the process still exists
        if let Some(pid) = selected_pid {
            self.select_by_pid(pid);
        }

        // Clear updating flag
        *self.updating.borrow_mut() = false;
    }

    /// Select a process by PID
    pub fn select_by_pid(&self, pid: u32) {
        // Search through the model to find the item
        let Some(model) = self.selection.model() else {
            return; // No model available, nothing to select
        };
        for i in 0..model.n_items() {
            if let Some(obj) = model.item(i) {
                if let Some(proc) = obj.downcast_ref::<ProcessObject>() {
                    if proc.pid() == pid {
                        self.selection.set_selected(i);
                        return;
                    }
                }
            }
        }
        // Process not found - clear selection
        self.selection.set_selected(gtk4::INVALID_LIST_POSITION);
    }

    /// Set the filter text for searching
    pub fn set_filter(&self, text: &str) {
        *self.filter_text.borrow_mut() = text.to_lowercase();
        let filter_text = self.filter_text.clone();

        let filter = CustomFilter::new(move |obj| {
            let text = filter_text.borrow();
            if text.is_empty() {
                return true;
            }
            if let Some(proc) = obj.downcast_ref::<ProcessObject>() {
                return proc.name().to_lowercase().contains(text.as_str())
                    || proc.pid().to_string().contains(text.as_str());
            }
            true
        });
        self.filter_model.set_filter(Some(&filter));
    }

    /// Get the selection model for connecting signals
    pub fn selection_model(&self) -> &SingleSelection {
        &self.selection
    }

    /// Get the column view widget for adding action groups
    pub fn column_view(&self) -> &ColumnView {
        &self.column_view
    }

    /// Get the currently selected process (pid, name)
    pub fn get_selected_process(&self) -> Option<(u32, String)> {
        self.selection
            .selected_item()
            .and_then(|obj| obj.downcast::<ProcessObject>().ok())
            .map(|p| (p.pid(), p.name()))
    }

    /// Connect a callback for row activation (double-click or Enter key)
    pub fn connect_double_click<F>(&self, callback: F)
    where
        F: Fn(u32, String) + 'static,
    {
        let selection = self.selection.clone();
        self.column_view.connect_activate(move |_column_view, position| {
            // Get the item at the activated position from the selection model
            if let Some(obj) = selection.model().and_then(|m| m.item(position)) {
                if let Some(proc) = obj.downcast_ref::<ProcessObject>() {
                    callback(proc.pid(), proc.name());
                }
            }
        });
    }
}
