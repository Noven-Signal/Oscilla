use std::path::Path;

use tokio::sync::mpsc::unbounded_channel;

use crate::{
    AppState::AppState::{AppStateContainer, PlayState},
    app::App,
    manipulation::PlayerControlSignal,
    widgets::AppRoot::*,
};

mod AppState;
mod AudioOutput;
mod DecoderWrapper;
mod MyDefMacro;
mod action;
mod app;
mod cli;
mod config;
mod errors;
mod extensions;
mod logging;
mod manipulation;
mod tui;
mod widgets;
mod visual_effects;
mod utils;
mod event_handler;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;

    let filter = |arg: &String| match Path::extension(Path::new(arg)) {
        Some(os_str) => {
            if let Some(ext_str) = os_str.to_str() {
                match ext_str {
                    "wav" | "mp3" | "ogg" | "m4a" => true,
                    _ => false,
                }
            } else {
                false
            }
        }
        None => false,
    };

    let current_exe = std::env::current_exe().expect("fail to retreive executable path");
    let current_exe_path = current_exe
        .to_str()
        .expect("fail to parse current executable path");
    let filtered_args = std::env::args()
        .filter(filter)
        .filter(|arg| arg != current_exe_path)
        .collect::<Vec<String>>();


    let app_state_container = AppStateContainer::new(filtered_args);

    let mut app = App::new(AppRoot::default(), app_state_container)?;
    app.run().await?;

    Ok(())
}
