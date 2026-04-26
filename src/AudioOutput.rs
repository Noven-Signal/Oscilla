use core::panic;
use core::result::Result::Ok;

use futures::future::ok;
use imp::CreateEventW;
use std::ops::Range;
use std::ptr::null;
use std::thread::sleep;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use windows::Win32::Foundation::{HANDLE, WAIT_EVENT};
use windows::Win32::System::Threading::{INFINITE, ResetEvent, WaitForSingleObject};
use windows::{
    Win32::{Media::Audio::*, System::Com::*},
    core::*,
};

use crate::manipulation::{BLOCK_SIZE, SendSignal, SharedBuffer};

pub async fn main(
    shared_buffer: &mut SharedBuffer,
    cancellation_token: CancellationToken,
) -> Result<()> {
    unsafe {
        let mut audio_output = AudioOutput::new(shared_buffer, cancellation_token)?;
        audio_output.start().await;
    }
    Ok(())
}

struct AudioOutput<'a> {
    audio_client: IAudioClient,
    render_client: IAudioRenderClient,
    buffer_frame_count: u32,
    cancellation_token: CancellationToken,
    shared_buffer: &'a mut SharedBuffer,
    wasapi_event_hanle: HANDLE,
}
unsafe impl<'a> Send for AudioOutput<'a> {}

impl<'a> AudioOutput<'a> {
    pub unsafe fn new(
        shared_buffer: &'a mut SharedBuffer,
        cancellation_token: CancellationToken,
    ) -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        let handle = {
            let handle = CreateEventW(null(), 0, 0, null());
            HANDLE(handle)
        };
        let audio_client = unsafe { Self::setup_audio_client(handle) }?;
        let buffer_frame_count = unsafe { audio_client.GetBufferSize() }?;
        let render_client = unsafe { audio_client.GetService()? };

        Ok(Self {
            audio_client: audio_client,
            buffer_frame_count,
            render_client,
            cancellation_token,
            shared_buffer,
            wasapi_event_hanle: handle,
        })
    }
    #[allow(unsafe_op_in_unsafe_fn)]
    async unsafe fn start(&mut self) -> Result<()> {
        self.audio_client.Start()?;
        self.render_loop().await?;
        Ok(())
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn setup_audio_client(event_handle: HANDLE) -> Result<IAudioClient> {
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
            0,
            //AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            duration,
            0,
            wave_format_ptr,
            Some(null()),
        )?;
        CoTaskMemFree(Some(wave_format_ptr as _));

       // audio_client.SetEventHandle(event_handle);
        Ok(audio_client)
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    async unsafe fn render_loop(&mut self) -> Result<()> {
        let next_block = |read_exclusive| match read_exclusive {
            7 => 0,
            read_exclusive => read_exclusive + 1,
        };

        let mut singnal_to_thread = async || {
            // let mut write_end = &write_end;
            let reciever = &mut self.shared_buffer.decoder_to_renderer_reciever;
            println!("xxx");
            reciever.recv().await;
            println!("yyy");
            let sender = &mut self.shared_buffer.renderer_to_decoder_sender;
            sender.send(SendSignal());
        };

        let mut channel_combined_buf: Vec<f32> = vec![0f32; self.buffer_frame_count as usize * 2];
        let mut head: usize = 0;
        let mut read_exclusive = 0;
        let mut count = 0;

        'l1: loop {
            if self.cancellation_token.is_cancelled() {
                break 'l1;
            }
            count = count + 1;
            println!("count: {count}");

            // match WaitForSingleObject(self.wasapi_event_hanle, INFINITE) {
            //     windows::Win32::Foundation::WAIT_OBJECT_0 => {}
            //     WAIT_EVENT(val) => {
            //         return Err(Error::new(HRESULT(val as i32), "wating event error"));
            //     }
            // };
            tokio::time::sleep(Duration::from_millis(10)).await;

            let padding = self.audio_client.GetCurrentPadding()?;
            let available = self.buffer_frame_count - padding;

            if available == 0 {
                continue 'l1;
            }

            let output_buffer = {
                println!("before get ptr");
                let ptr = self.render_client.GetBuffer(available);
                let ptr = match ptr {
                    Err(e) => {
                        dbg!(e);
                        panic!();
                    }
                    x => x.unwrap(),
                };
                println!("after get ptr");
                unsafe { std::slice::from_raw_parts_mut(ptr as *mut f32, available as usize * 2) }
            };
            println!("after output buffer");
            let get_block =
                |read_exclusive| &self.shared_buffer.channel_left_data[read_exclusive as usize];

            let mut fill_buff_within_block = || {
                let src_slice_ch_0 = &get_block(read_exclusive)[head..head + available as usize];

                let channel_combined_view = &mut channel_combined_buf[0..available as usize * 2];

                for i in 0..src_slice_ch_0.len() {
                    channel_combined_view[i * 2] = src_slice_ch_0[i];
                    channel_combined_view[i * 2 + 1] = src_slice_ch_0[i];
                }
                println!("avaliable: {}", available);
                println!("output_buffer.len(): {}", output_buffer.len());
                println!(
                    "channel_combined_buf.len(): {}",
                    channel_combined_view.len()
                );
                output_buffer.copy_from_slice(&channel_combined_view);
            };
            let target_len = head + available as usize;

            use std::cmp::Ordering::*;
            match Ord::cmp(&target_len, &get_block(read_exclusive).len()) {
                Less => {
                    fill_buff_within_block();
                    head = head + available as usize;
                    dbg!(&output_buffer[0..10]);
                    println!("less");
                }
                Equal => {
                    fill_buff_within_block();
                    singnal_to_thread().await;

                    read_exclusive = next_block(read_exclusive);
                    head = 0;
                    println!("equal");
                }
                Greater => {
                    println!("Grater");
                    

                    let src_slice_current_ch_0 = &get_block(read_exclusive)[head..];
                    let channel_combined_view =
                        &mut channel_combined_buf[0..available as usize * 2];

                    for i in 0..src_slice_current_ch_0.len() {
                        channel_combined_view[i * 2] = src_slice_current_ch_0[i];
                        channel_combined_view[i * 2 + 1] = src_slice_current_ch_0[i];
                    }

                    let output_buff_spilt_len = src_slice_current_ch_0.len() * 2;

                    output_buffer[0..output_buff_spilt_len].copy_from_slice(
                        &channel_combined_view[0..output_buff_spilt_len],
                    );
                    singnal_to_thread().await;
                    read_exclusive = next_block(read_exclusive);

                    let src_slice_spill_over_ch_0 = &get_block(read_exclusive)
                        [0..head + available as usize - get_block(read_exclusive).len()];

                    assert!(src_slice_spill_over_ch_0.len() < get_block(read_exclusive).len());

                    for i in 0..src_slice_spill_over_ch_0.len() {
                        channel_combined_view[i * 2 + src_slice_current_ch_0.len()] =
                            src_slice_spill_over_ch_0[i];
                        channel_combined_view[i * 2 + 1 + src_slice_current_ch_0.len()] =
                            src_slice_spill_over_ch_0[i];
                    }

                    output_buffer[output_buff_spilt_len..].copy_from_slice(&channel_combined_view[output_buff_spilt_len..]);

                    head = src_slice_spill_over_ch_0.len();
                }
            };
            println!("match end");
            self.render_client.ReleaseBuffer(available, 0)?;
            println!("buffer released ");
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
