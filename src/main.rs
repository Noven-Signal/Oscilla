use clap::Parser;
use cli::Cli;

use crate::{app::App, widgets::AppRoot};

mod action;
mod app;
mod cli;
mod components;
mod config;
mod errors;
mod logging;
mod tui;
mod widgets;
mod AppState;
mod extensions;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;
    

    AppState::AppState::init();

    let args = Cli::parse();
    let mut app = App::new(AppRoot::AppRoot::default())?;
    app.run().await?;
    Ok(())
}
