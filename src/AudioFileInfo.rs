use std::path::Path;

use symphonia::core::{meta::Tag, *};
use tracing::info;

use crate::DecoderWrapper::DecoderWrapper;

pub struct AudioFileInfo {
    pub file_path: String,
    pub title: Option<String>,
    pub album_name: Option<String>,
    pub artist_name: Option<String>,
    disp_name: String,
}

impl AudioFileInfo {
    pub fn new(path: &str) -> Self {
        let mut decoder_wrapper = DecoderWrapper::new(path).expect("decoder init error");

        let mapper = |tags: &[Tag]| {
            let mut audio_file_info = AudioFileInfo {
                file_path: path.to_string(),
                title: None,
                album_name: None,
                artist_name: None,
                disp_name: path.to_string(),
            };
            let audio_file_info_ref = &mut audio_file_info;
            for tag in tags {
                use symphonia::core::meta::StandardTagKey::*;
                let mapped_ref = match tag.std_key {
                    Some(key) => match key {
                        Album => &mut audio_file_info_ref.album_name,
                        Artist => &mut audio_file_info_ref.artist_name,
                        TrackTitle => &mut audio_file_info_ref.title,
                        _ => continue,
                    },
                    None => continue,
                };

                // if same key exists, overriten by later visit value
                *mapped_ref = match &tag.value {
                    meta::Value::String(str) => Some(str.clone()),
                    _ => None,
                }
            }
            let file_name = {
                let file_name = Path::new(path)
                    .file_name()
                    .and_then(|os_str| os_str.to_str());
                if let Some(file_name) = file_name {
                    file_name
                } else {
                    ""
                }
            };
            audio_file_info.disp_name = match (&audio_file_info.title, &audio_file_info.artist_name)
            {
                (None, None) => file_name.to_string(),
                (None, Some(_)) => file_name.to_string(),
                (Some(title), None) => title.clone(),
                (Some(title), Some(artist_name)) => format!("{title} [{artist_name}]"),
            };
            audio_file_info
        };
        decoder_wrapper.extract_file_info_map_into(mapper)
    }

    pub fn get_disp_name(&self) -> &String {
        &self.disp_name
    }
}
