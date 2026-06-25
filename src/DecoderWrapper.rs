use std::error::Error;
use std::fmt::{self, Display};
use std::path::Path;
use std::time::Duration;

use symphonia::core::audio::{AsAudioBufferRef, AudioBufferRef, Signal};
use symphonia::core::codecs::{CodecParameters, Decoder, DecoderOptions};

use symphonia::core::formats::{FormatReader, Packet};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{Metadata, Tag};
use symphonia::core::probe::{Hint, ProbeResult};
use symphonia::default::get_probe;
use tracing::info;

/// Options for the decode command.
#[derive(Copy, Clone)]
pub struct DecoderOptionsAndTrackNum {
    pub dec_opts: DecoderOptions,
    pub track_num: Option<usize>,
}

#[derive(Debug)] // Required for Debug trait
pub enum DecodeInitError {
    FileOpenFailed,
    NoTrackFound,
}

// 1. Implement Display for user-friendly messages
impl Display for DecodeInitError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            _ => write!(f, "No Default Track is found "),
        }
    }
}

// 2. Implement the Error trait
impl Error for DecodeInitError {}

pub struct DecoderWrapper {
    probe_result: ProbeResult,
    decoder: Box<dyn Decoder>,
    track_id: u32,
}
pub enum DecodeResult<'a> {
    Buf(AudioBufferRef<'a>),
    Err(symphonia::core::errors::Error),
    EndOfStream,
    None,
}

impl DecoderWrapper {
    fn open_file(path: &str) -> Result<ProbeResult, DecodeInitError> {
        let probe = get_probe();
        use std::fs::File;
        let Ok(file) = File::open(path) else {
            return Err(DecodeInitError::FileOpenFailed);
        };
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        let extension = match Path::extension(Path::new(path)) {
            Some(os_str) => match os_str.to_str() {
                Some(str) => str,
                None => "",
            },
            None => "",
        };
        hint.with_extension(extension);

        let probe_result = probe
            .format(&hint, mss, &Default::default(), &Default::default())
            .map_err(|_| DecodeInitError::FileOpenFailed)?;

        Ok(probe_result)
    }

    pub fn new(path: &str) -> Result<Self, DecodeInitError> {
        let probe_result = Self::open_file(path)?;

        let reader = &probe_result.format;

        let track = reader
            .default_track()
            .ok_or_else(|| DecodeInitError::NoTrackFound)?;

        let decoder: Box<dyn Decoder> = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: true })
            .unwrap();

        let track_id = track.id;
        Ok(Self {
            probe_result,
            decoder,
            track_id,
        })
    }

    pub fn decode(&'_ mut self) -> DecodeResult<'_> {
        let packet = match self.probe_result.format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(err)) => {
                //temp EndOfStream
                return DecodeResult::EndOfStream;
            }
            Err(err) => return DecodeResult::Err(err),
        };

        if packet.track_id() != self.track_id {
            return DecodeResult::None;
        }
        match self.decoder.decode(&packet) {
            Ok(buf) => DecodeResult::Buf(buf),
            Err(err) => DecodeResult::Err(err),
        }
        //   do_verification(decoder.finalize())
    }

    fn get_codec_params(&self) -> Option<&CodecParameters> {
        Some(
            &self
                .probe_result
                .format
                .tracks()
                .iter()
                .filter(|track| track.id == self.track_id)
                .next()?
                .codec_params,
        )
    }

    pub fn get_duration(&self) -> Option<Duration> {
        let params = self.get_codec_params()?;

        let (Some(n_frames), Some(sample_rate)) = (params.n_frames, params.sample_rate) else {
            return None;
        };

        Some(Duration::from_secs_f64(
            n_frames as f64 / sample_rate as f64,
        ))
    }

    pub fn get_sample_rate(&self) -> Option<u32> {
        let params = self.get_codec_params()?;
        params.sample_rate
    }

    pub fn extract_file_info_map_into<T>(&mut self, f: impl FnOnce(&[Tag]) -> T) -> T {
        let mut meta_data = self.probe_result.metadata.get();

        let tags_container = match meta_data {
            Some(ref mut meta_data) => Self::get_tags_from_meta_data(meta_data),
            None => &[],
        };

        let mut meta_data = self.probe_result.format.metadata();
        let tags_format = Self::get_tags_from_meta_data(&mut meta_data);

        let tags_both_from_container_format = [tags_container, tags_format].concat();

        f(tags_both_from_container_format.as_slice())
    }

    fn get_tags_from_meta_data<'a>(meta_data: &'a mut Metadata<'a>) -> &'a [Tag] {
        match meta_data.current() {
            Some(current) => current.tags(),
            None => &[],
        }
    }
}
