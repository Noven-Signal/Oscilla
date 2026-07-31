use color_eyre::eyre::Ok;
use rubato::{
    Fft, FixedSync, Indexing, Resampler, audioadapter_buffers::direct::SequentialSliceOfSlices,
};

use crate::manipulation::CHANNEL;

pub struct RsamplerWrapper {
    resampler: Fft<f32>,
}

impl RsamplerWrapper {
    pub fn new(
        sample_rate_input: usize,
        sample_rate_output: usize,
        chunk_size: usize,
    ) -> color_eyre::eyre::Result<Self> {
        let resampler = Fft::<f32>::new(
            sample_rate_input,
            sample_rate_output,
            chunk_size,
            2,
            CHANNEL,
            FixedSync::Both,
        )?;

        Ok(Self { resampler })
    }

    pub fn proc<'a>(
        &mut self,
        input_buffer: &[&[f32]; CHANNEL],
        output_buffer: &'a mut [&'a mut [f32]; CHANNEL],
    ) -> color_eyre::eyre::Result<()> {
        let input_adapter =
            SequentialSliceOfSlices::new(input_buffer, CHANNEL, input_buffer[0].len())?;

        let len = output_buffer[0].len();
        let mut output_adapter = SequentialSliceOfSlices::new_mut(output_buffer, CHANNEL, len)?;

        let mut indexing = Indexing {
            input_offset: 0,
            output_offset: 0,
            active_channels_mask: None,
            partial_len: None,
        };

        let mut input_frames_left = input_buffer[0].len();
        let mut input_frames_next = self.resampler.input_frames_next();

        while input_frames_left >= input_frames_next {
            let (frames_read, frames_written) = &self
                .resampler
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
