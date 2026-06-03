mod app;
mod config;
mod win_state;

fn main() -> gtk::glib::ExitCode {
    let application = app::SettingsApp::new();
    application.run()
}
