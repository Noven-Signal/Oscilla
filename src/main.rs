use std::{path::Path, time::Duration};

use tokio::sync::mpsc::unbounded_channel;

use crate::{
    AppState::AppState::{AppStateContainer, PlayState},
    app::{App, AppContorlSignal},
    manipulation::PlayerControlSignal,
    widgets::AppRoot::*,
};

mod AppState;
mod AudioDecoder;
mod AudioOutput;
mod ResamplerWrapper;
mod DecoderWrapper;
mod MyDefMacro;
mod action;
mod app;
mod cli;
mod config;
mod errors;
mod event_handler;
mod extensions;
mod logging;
mod manipulation;
mod tui;
mod utils;
mod visual_effects;
mod widgets;
mod AudioFileInfo;

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

    let (app_control_signal_sender,app_control_signal_recv) = unbounded_channel::<AppContorlSignal>();

    let app_state_container = AppStateContainer::new(app_control_signal_sender,filtered_args);

    let mut app = App::new(AppRoot::default(), app_state_container,app_control_signal_recv)?;
    app.run().await?;

    Ok(())
}
