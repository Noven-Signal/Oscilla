use tokio::sync::mpsc::unbounded_channel;

use crate::{
    app::{App, AppContorlSignal, PopupObject},
    app_state::app_state::AppStateContainer,
    shared::filter_valid_extension,
    widgets::app_root::*,
};

mod app;
mod app_state;
mod audio_decoder;
mod audio_file_info;
mod audio_output;
mod decoder_wrapper;
mod errors;
mod extensions;
mod logging;
mod manipulation;
mod my_def_macro;
mod resampler_wrapper;
mod shared;
mod tui;
mod utils;
mod visual_effects;
mod widgets;
mod key_guide;
mod rgb_color;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;

    let current_exe = std::env::current_exe().expect("fail to retreive executable path");
    let current_exe_path = current_exe
        .to_str()
        .expect("fail to parse current executable path");
    let filtered_args = std::env::args()
        .filter(filter_valid_extension)
        .filter(|arg| arg != current_exe_path)
        .collect::<Vec<String>>();

    let (app_control_signal_sender, app_control_signal_recv) =
        unbounded_channel::<AppContorlSignal>();
    let (popup_queue_signal_sender, popup_queue_signal_recv) = unbounded_channel::<PopupObject>();

    let app_state_container = AppStateContainer::new(
        app_control_signal_sender,
        popup_queue_signal_sender,
        filtered_args,
    );

    let mut app = App::new(
        AppRoot::default(),
        app_state_container,
        app_control_signal_recv,
        popup_queue_signal_recv,
    )?;
    app.run().await?;

    Ok(())
}
