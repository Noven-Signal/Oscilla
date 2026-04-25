use std::{fs::File, path::Path};

use symphonia::{
    core::{
        codecs::{CodecRegistry, DecoderOptions},
        io::MediaSourceStream,
        probe::{Hint, Probe},
    },
    default::{formats::WavReader, get_codecs, get_probe},
};

use crate::{
    AppState::AppState::AppStateContainer, DecoderWrapper::DecoderOptionsAndTrackNum, app::App,
    extensions::OnceLock::OnceLock_ext, widgets::AppRoot::*,
};

mod AppState;
mod MyDefMacro;
mod DecoderWrapper;
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
mod AudioOutput;
mod manipulation;

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
        .filter(|arg| arg != current_exe_path)
        .collect::<Vec<String>>();

    //AudioOutput::main()?;
    return Ok(());
    

    // let codecs = get_codecs();
    // let probe = get_probe();
    // use std::fs::File;
    // let file = File::open(filtered_args[0].as_str()).unwrap();
    // let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // // _hint: &Hint,
    // // mut mss: MediaSourceStream,
    // // format_opts: &FormatOptions,
    // // metadata_opts: &MetadataOptions,
    // let mut hint = Hint::new();
    // hint.with_extension("mp3");
    // let probe_result = probe.format(&hint, mss, &Default::default(), &Default::default());
    // let format_reader = match probe_result {
    //     Ok(res) => res.format,
    //     Err(_) => todo!(),
    // };
    // let options = DecoderOptionsAndTrackNum {
    //     dec_opts: DecoderOptions { verify: true },
    //     track_num: Some(0),
    // };
    // let _ = DecoderWrapper::DecoderWrapper ::doecode(format_reader, options);

    let mut app = App::new(AppRoot::default(), AppStateContainer::new(filtered_args))?;
    app.run().await?;
    Ok(())
}
