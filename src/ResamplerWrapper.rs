use audioadapter_buffers::direct::InterleavedSlice;
use color_eyre::eyre::Ok;
use rubato::{
    Fft, FixedSync, Indexing, Resampler,
    audioadapter_buffers::{
        self,
        direct::{SequentialSliceOfSlices, SequentialSliceOfVecs},
    },
};

use crate::manipulation::CHANNEL;

pub struct RsamplerWrapper {
    resampler: Fft<f32>,
}

impl RsamplerWrapper {
    pub fn new(
        sample_rate_input: usize,
        sample_rate_output: usize,
    ) -> color_eyre::eyre::Result<Self> {
        let resampler = Fft::<f32>::new(
            sample_rate_input,
            sample_rate_output,
            1024,
            2,
            CHANNEL,
            FixedSync::Both,
        )?;

        Ok(Self {
            resampler,
        })
    }

    pub fn proc<'a>(
        &mut self,
        input_buffer: &[&[f32]; CHANNEL],
        output_buffer: &'a mut [&'a mut [f32]; CHANNEL],
    ) -> color_eyre::eyre::Result<()> {
        // create a short dummy audio clip, assuming it's stereo stored as interleaved f64 values
        //let audio_clip = vec![0.0; 2 * 10000];

        // wrap it with an InterleavedSlice Adapter
        //let nbr_input_frames = audio_clip.len() / 2;
        let input_adapter =
            SequentialSliceOfSlices::new(input_buffer, CHANNEL, input_buffer[0].len())?;
        //let input_adapter = InterleavedSlice::new(&audio_clip, 2, nbr_input_frames).unwrap();

        let len = output_buffer[0].len();
        let mut output_adapter = SequentialSliceOfSlices::new_mut(output_buffer, CHANNEL, len)?;

        // Preparations
        let mut indexing = Indexing {
            input_offset: 0,
            output_offset: 0,
            active_channels_mask: None,
            partial_len: None,
        };

        let mut input_frames_left = input_buffer[0].len();
        let mut input_frames_next = self.resampler.input_frames_next();

        // Loop over all full chunks.
        // There will be some unprocessed input frames left after the last full chunk.
        // see the `process_f64` example for how to handle those
        // using `partial_len` of the indexing struct.
        // It is also possible to use the `process_all_into_buffer` method
        // to process the entire file (including any last partial chunk) with a single call.
        while input_frames_left >= input_frames_next {
            let (frames_read, frames_written) = &self.resampler
                .process_into_buffer(&input_adapter, &mut output_adapter, Some(&indexing))
                .unwrap();

            indexing.input_offset += frames_read;
            indexing.output_offset += frames_written;
            input_frames_left -= frames_read;
            input_frames_next = self.resampler.input_frames_next();
        }

        Ok(())
    }
}
