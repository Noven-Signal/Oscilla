use std::error::Error;
use std::fmt::{self, Display};

use symphonia::core::audio::{AsAudioBufferRef, AudioBufferRef, Signal};
use symphonia::core::codecs::{Decoder, DecoderOptions};

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

    pub fn decode(&mut self) -> DecodeResult {
        let packet = match self.reader.next_packet() {
            Ok(p) => p,
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
}
