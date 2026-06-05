pub mod domain;
pub mod application;
pub mod infrastructure;

use crate::infrastructure::tui::run_tui;

fn main() {
    if let Err(e) = run_tui() {
        eprintln!("TUI Error: {}", e);
    }
}
