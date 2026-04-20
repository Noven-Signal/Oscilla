use std::path::Path;

use crate::{AppState::AppState::AppStateContainer, app::App, extensions::OnceLock::OnceLock_ext, widgets::AppRoot::*};

mod AppState;
mod action;
mod app;
mod cli;
mod components;
mod config;
mod errors;
mod extensions;
mod logging;
mod tui;
mod widgets;

use std::path::*;


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
        .filter(|arg| arg == current_exe_path);

    // AppState::AppState::init(filtered_args.collect());

    let mut app = App::new(AppRoot::default(), AppStateContainer::new(filtered_args.collect()))?;
    app.run().await?;
    Ok(())
}
