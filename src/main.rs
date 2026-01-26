mod context_menu;
mod detail_view;
mod monitor;
mod process_actions;
mod process_list;
mod process_window;
mod window;

use gtk4::prelude::*;
use libadwaita as adw;

const APP_ID: &str = "org.okular.ProcessMonitor";

fn main() -> glib::ExitCode {
    // Initialize GTK
    gtk4::init().expect("Failed to initialize GTK4");

    // Add current directory and exe directory to icon search path
    if let Some(display) = gtk4::gdk::Display::default() {
        let theme = gtk4::IconTheme::for_display(&display);
        theme.add_search_path(".");
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())) {
            theme.add_search_path(&exe_dir);
        }
    }

    // Create the application
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(|app| {
        let window = window::OcularWindow::build(app);
        window.present();
    });

    app.run()
}
