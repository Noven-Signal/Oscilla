// src/main.rs
// Minimal WASAPI playback example using windows-rs
// Plays a 440Hz sine wave for 5 seconds.

use core::result::Result::Ok;
use core::slice;
use imp::CreateEventW;
use std::alloc::GlobalAlloc;
use std::ops::Range;
use std::ptr::null;
use std::thread::sleep;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use windows::Win32::Foundation::HANDLE;
use windows::{
    Win32::{Foundation::BOOL, Media::Audio::*, System::Com::*},
    core::*,
};

use crate::manipulation::{BlockRange, RingBufferInfo, SharedBuffer};

pub async fn main(
    shared_buffer: &mut SharedBuffer,
    cancellation_token: CancellationToken,
) -> Result<()> {
    unsafe {
        let mut audio_output = AudioOutput::new(shared_buffer, cancellation_token)?;
        audio_output.start().await?;
    }
    Ok(())
}

struct AudioOutput<'a> {
    audio_client: IAudioClient,
    render_client: IAudioRenderClient,
    buffer_frame_count: u32,
    cancellation_token: CancellationToken,
    shared_buffer: &'a mut SharedBuffer,
}

impl<'a> AudioOutput<'a> {
    pub unsafe fn new(
        shared_buffer: &'a mut SharedBuffer,
        cancellation_token: CancellationToken,
    ) -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        let audio_client = unsafe { Self::setup_audio_client() }?;
        let buffer_frame_count = unsafe { audio_client.GetBufferSize() }?;
        let render_client = unsafe { audio_client.GetService()? };

        Ok(Self {
            audio_client: audio_client,
            buffer_frame_count,
            render_client,
            cancellation_token,
            shared_buffer,
        })
    }
    #[allow(unsafe_op_in_unsafe_fn)]
    async unsafe fn start(&mut self) -> Result<()> {
        self.audio_client.Start()?;
        self.render_loop().await?;
        Ok(())
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn setup_audio_client() -> Result<IAudioClient> {
        // Create device enumerator
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        // Default playback device
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;

        let device_list = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE);
        let prop = device_list
            .unwrap()
            .Item(0)
            .unwrap()
            .OpenPropertyStore(STGM_READ);
        // let count = device_list.unwrap().GetCount();

        use windows::Win32::Devices::FunctionDiscovery::*;
        let mut ptr = PKEY_Device_FriendlyName;
        let pro_varant = prop.unwrap().GetValue(&ptr);
        println!("{}", pro_varant.unwrap());
        // let v = prop.unwrap().GetValue(*PROPERTYKEY::QUERY);
        // Activate IAudioClient
        let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

        let wave_format_ptr = audio_client.GetMixFormat()?;

        let wave_format: WAVEFORMATEX = *wave_format_ptr;

        let sample_rate = wave_format.nSamplesPerSec as f64;
        let channels = wave_format.nChannels as usize;
        let bits = wave_format.wBitsPerSample;
        let format_tag = wave_format.wFormatTag;

        println!(
            "sample_rate: {sample_rate}, channels:{channels}, bits:{bits}, format_tag:{format_tag}"
        );
        println!("format_tag={:#x}", format_tag);
        // 1 second buffer duration (100-nanosecond units)
        let duration: i64 = 10_000_000;
        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            duration,
            0,
            wave_format_ptr,
            Some(null()),
        )?;
        CoTaskMemFree(Some(wave_format_ptr as _));

        let h_handle = CreateEventW(null(), 0, 0, null());
        audio_client.SetEventHandle(HANDLE(h_handle));

        Ok(audio_client)
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    async unsafe fn render_loop(&mut self) -> Result<()> {
        // let ring_buffer_info_mutex = self.shared_buffer.ring_buffer_info.lock().await;
        // let RingBufferInfo {
        //     mut read_exclusive,
        //     write_buf: BlockRange { end: write_end, .. },
        // } = *ring_buffer_info_mutex;
        // drop(ring_buffer_info_mutex);
        let next_block = |read_exclusive| match read_exclusive {
            7 => 0,
            read_exclusive => read_exclusive + 1,
        };
        let mut move_to_next_block = async || {
            let mut mutex = self.shared_buffer.ring_buffer_info.lock().await;

            if next_block(mutex.read_exclusive) == mutex.write_buf.end as usize {
                let reciever = &mut self.shared_buffer.decoder_to_renderer_reciever;
                reciever.recv().await;
            }
            let next = next_block(mutex.read_exclusive);
            mutex.read_exclusive = next;
            next
        };

        let next = move_to_next_block().await;
        let mut read_exclusive = next;
        let mut channel_combined_buf: Vec<f32> = vec![0f32; self.buffer_frame_count as usize * 2];
        let mut head: usize = 0;
        'l1: loop {
            if self.cancellation_token.is_cancelled() {
                break 'l1;
            }
            sleep(Duration::from_millis(10));

            let target_ptr = self.render_client.GetBuffer(self.buffer_frame_count)?;

            let get_block =|read_exclusive| &self.shared_buffer.channel_left_data[read_exclusive as usize];

            let mut fill_buff = || {
                let out = unsafe {
                    std::slice::from_raw_parts_mut(
                        target_ptr as *mut f32,
                        self.buffer_frame_count as usize * 2,
                    )
                };
                let src_slice_ch_0 = &get_block(read_exclusive)[head..head + self.buffer_frame_count as usize];

                for i in 0..src_slice_ch_0.len() {
                    channel_combined_buf[i * 2] = src_slice_ch_0[i];
                    channel_combined_buf[i * 2 + 1] = src_slice_ch_0[i];
                }
                out.copy_from_slice(&channel_combined_buf);
            };
            let target_len = head + self.buffer_frame_count as usize;
            use std::cmp::Ordering::*;
            match Ord::cmp(&target_len, &get_block(read_exclusive).len()) {
                Less => {
                    fill_buff();
                    head = head + self.buffer_frame_count as usize;
                }
                Equal => {
                    fill_buff();
                    let next = move_to_next_block().await;
                    read_exclusive = next;

                    head = 0;
                }
                Greater => {
                    let out = unsafe {
                        std::slice::from_raw_parts_mut(
                            target_ptr as *mut f32,
                            self.buffer_frame_count as usize * 2,
                        )
                    };
                    let src_slice_current_ch_0 = &get_block(read_exclusive)[head..];

                    for i in 0..src_slice_current_ch_0.len() {
                        channel_combined_buf[i * 2] = src_slice_current_ch_0[i];
                        channel_combined_buf[i * 2 + 1] = src_slice_current_ch_0[i];
                    }
                    out[0..src_slice_current_ch_0.len() * 2].copy_from_slice(
                        &channel_combined_buf[0..src_slice_current_ch_0.len() * 2],
                    );

                    let next = move_to_next_block().await;
                    read_exclusive = next;

                    let src_slice_spill_over_ch_0 =
                        &get_block(read_exclusive)[0..head + self.buffer_frame_count as usize - get_block(read_exclusive).len()];

                    assert!(src_slice_spill_over_ch_0.len() < get_block(read_exclusive).len());

                    for i in 0..src_slice_spill_over_ch_0.len() {
                        channel_combined_buf[i * 2 + src_slice_current_ch_0.len()] =
                            src_slice_spill_over_ch_0[i];
                        channel_combined_buf[i * 2 + 1 + src_slice_current_ch_0.len()] =
                            src_slice_spill_over_ch_0[i];
                    }

                    out.copy_from_slice(&channel_combined_buf);
                }
            };

            self.render_client
                .ReleaseBuffer(self.buffer_frame_count, 0)?;
        }

        //sleep(Duration::from_millis(500));
        self.audio_client.Stop()?;

        Ok(())
    }
}
impl<'a> Drop for AudioOutput<'a> {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

unsafe fn Fill_buff(
    target_ptr: *mut u8,
    ptr_len: usize,
    left_ch_data: Vec<f32>,
    range: Range<usize>,
) {
}

unsafe fn fill_buffer_f32(
    data: *mut u8,
    frames: usize,
    channels: usize,
    phase: &mut f64,
    phase_step: f64,
) {
    let samples = frames * channels;
    let out = unsafe { std::slice::from_raw_parts_mut(data as *mut f32, samples) };

    for frame in 0..frames {
        for ch in 0..channels {
            let sample = (*phase).sin();
            out[frame * channels + ch] = sample as f32;
        }

        *phase += phase_step;
    }
}
