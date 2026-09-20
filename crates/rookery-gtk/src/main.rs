use gtk4 as gtk;
use libadwaita as adw;
use gtk::prelude::*;
use adw::prelude::*;
use adw::{Application, ApplicationWindow};

use rookery_core::AnkiSession;

fn main() {
    let app = Application::builder()
        .application_id("io.github.ritesh.Rookery")
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Rookery M0")
        .default_width(800)
        .default_height(600)
        .build();

    let mut data_dir = gtk::glib::user_data_dir();
    data_dir.push("rookery");
    let _ = std::fs::create_dir_all(&data_dir);
    
    let mut profile_path = data_dir.clone();
    profile_path.push("default.anki2");

    let status = match AnkiSession::open(profile_path) {
        Ok(mut session) => {
            match session.list_decks() {
                Ok(decks) => format!("Loaded {} decks", decks.len()),
                Err(e) => format!("Error loading decks: {}", e),
            }
        },
        Err(e) => format!("Failed to open collection: {}", e),
    };

    let label = gtk::Label::new(Some(&status));
    window.set_content(Some(&label));
    window.present();
}
