pub mod artwork;
pub mod config;
pub mod constants;
pub mod discord;
pub mod media;
pub mod tray;

/// Commands sent from the main (winit) thread to the Tokio pipeline.
#[derive(Debug)]
pub enum AppCommand {
    SetEnabled(bool),
    Quit,
}
