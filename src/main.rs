use std::path::Path;

use tokio::sync::mpsc::unbounded_channel;

use crate::{
    AppState::AppState::{AppStateContainer, PlayState},
    app::App,
    manipulation::PlayerControlSignal,
    tui::Event,
    widgets::AppRoot::*,
};

mod AppState;
mod AudioOutput;
mod DecoderWrapper;
mod MyDefMacro;
mod action;
mod app;
mod cli;
mod components;
mod config;
mod errors;
mod extensions;
mod logging;
mod manipulation;
mod tui;
mod widgets;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;

    let filter = |arg: &String| match Path::extension(Path::new(arg)) {
        Some(os_str) => {
            if let Some(ext_str) = os_str.to_str() {
                match ext_str {
                    "wav" | "mp3" => true,
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

    let first_play_file_path = filtered_args[0].clone();

    let mut app_state_container = AppStateContainer::new(filtered_args);
    app_state_container.play_state = PlayState::Playing(0);
    let (player_control_signal_sender, mut player_control_signal_recv) =
        unbounded_channel::<PlayerControlSignal>();
    app_state_container.player_control_singnal_sender = Some(player_control_signal_sender);
    let initial_play_thread_handle = tokio::spawn(async move {
        manipulation::play_executor(&first_play_file_path, &mut player_control_signal_recv).await;
    });

    let mut app = App::new(AppRoot::default(), app_state_container)?;
    app.run().await?;
    initial_play_thread_handle.await?;

    Ok(())
}
