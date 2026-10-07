use clap::Parser;
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
mod key_guide;
mod logging;
mod manipulation;
#[cfg(feature = "mcp")]
mod mcp;
mod my_def_macro;
mod resampler_wrapper;
mod rgb_color;
mod shared;
mod tui;
mod utils;
mod visual_effects;
mod widgets;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long, default_value_t = false)]
    mcp_enabled: bool,

    files: Option<Vec<String>>,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;

    let args = Args::parse();

    let filtered_args: Vec<_> = args
        .files
        .unwrap_or(Vec::new())
        .into_iter()
        .filter(filter_valid_extension)
        .collect();

    let (app_control_signal_sender, app_control_signal_recv) =
        unbounded_channel::<AppContorlSignal>();
    let (popup_queue_signal_sender, popup_queue_signal_recv) = unbounded_channel::<PopupObject>();

    let app_state_container = AppStateContainer::new(
        app_control_signal_sender,
        popup_queue_signal_sender,
        filtered_args,
        args.mcp_enabled,
    )
    .await;

    let mut app = App::new(
        AppRoot::default(),
        app_state_container,
        app_control_signal_recv,
        popup_queue_signal_recv,
    )?;
    app.run().await?;

    Ok(())
}
