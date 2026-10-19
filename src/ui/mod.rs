//! GNOME interface (GTK 4 + libadwaita), built in code: no Blueprint compiler needed to build.

mod preview;
mod window;
mod worker;

use adw::prelude::*;
use gtk::{gio, glib};

pub const APP_ID: &str = "io.github.fdeslongchamps.Ddsnap";

pub fn run() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_activate(|app| window::Window::new(app).present());
    app.connect_open(|app, files, _| {
        let window = window::Window::new(app);
        window.present();
        if let Some(path) = files.first().and_then(|f| f.path()) {
            window.open(path);
        }
    });
    app.set_accels_for_action("win.open", &["<Control>o"]);
    app.set_accels_for_action("win.export", &["<Control>e"]);
    app.set_accels_for_action("win.export-all", &["<Control><Shift>e"]);
    app.set_accels_for_action("window.close", &["<Control>w"]);
    app.run()
}
