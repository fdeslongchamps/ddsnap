// The UI lives outside the library so that the CLI and `core` never link GTK.
#[path = "../ui/mod.rs"]
mod ui;

fn main() -> gtk::glib::ExitCode {
    ui::run()
}
