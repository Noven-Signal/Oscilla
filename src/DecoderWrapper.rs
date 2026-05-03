use std::error::Error;
use std::fmt::{self, Display};
use std::time::Duration;

use symphonia::core::audio::{AsAudioBufferRef, AudioBufferRef, Signal};
use symphonia::core::codecs::{CodecParameters, Decoder, DecoderOptions};

use symphonia::core::formats::{FormatReader, Packet};
use tracing::info;

/// Options for the decode command.
#[derive(Copy, Clone)]
pub struct DecoderOptionsAndTrackNum {
    pub dec_opts: DecoderOptions,
    pub track_num: Option<usize>,
}

#[derive(Debug)] // Required for Debug trait
pub enum DecodeInitError {
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
    reader: Box<dyn FormatReader>,
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
    pub fn new(reader: Box<dyn FormatReader>) -> Result<Self, DecodeInitError> {
        let track = reader
            .default_track()
            .ok_or_else(|| DecodeInitError::NoTrackFound)?;

        let decoder: Box<dyn Decoder> = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: true })
            .unwrap();

        let track_id = track.id;
        Ok(Self {
            reader,
            decoder,
            track_id,
        })
    }

    pub fn decode(&'_ mut self) -> DecodeResult<'_> {
        let packet = match self.reader.next_packet() {
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
                .reader
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
}
