mod monitor;
mod process_list;
mod detail_view;
mod window;

use gtk4::prelude::*;
use libadwaita as adw;

const APP_ID: &str = "org.okular.ProcessMonitor";

fn main() -> glib::ExitCode {
    // Initialize GTK
    gtk4::init().expect("Failed to initialize GTK4");

    // Create the application
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(|app| {
        let window = window::OcularWindow::new(app);
        window.present();
    });

    app.run()
}
