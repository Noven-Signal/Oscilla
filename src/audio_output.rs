use core::result::Result::Ok;
use imp::CreateEventW;
use std::cmp;
use std::ptr::null;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::debug;

use windows::Win32::Foundation::{HANDLE, WAIT_EVENT};
use windows::Win32::System::Threading::{INFINITE, WaitForSingleObject};
use windows::{
    Win32::{Media::Audio::*, System::Com::*},
    core::*,
};

use crate::app::{PlayedFrames, PlayerToUISingnal};
use crate::manipulation::{
    AUDIO_OUTPUT_BUFFER_DURATION, AudioDeviceInfo, CHANNEL, DecoderToRendererSyncSignal,
    EndOfStreamSignal, NUM_OF_BLOCK, RendererControlSignal, RendererToDecoderSsynSignal,
    SeekSignalForRenderer, SharedBuffer, WorkerToPlayerNotification,
};

pub fn main(
    shared_buffer: &SharedBuffer,
    renderer_to_decoder_singal_sender: UnboundedSender<RendererToDecoderSsynSignal>,
    decoder_to_renderer_singal_recv: UnboundedReceiver<DecoderToRendererSyncSignal>,
    control_signal_receiver: UnboundedReceiver<RendererControlSignal>,
    player_to_ui_singnal_sender: UnboundedSender<PlayerToUISingnal>,
    worker_to_player_notofication_signal_sender: UnboundedSender<WorkerToPlayerNotification>,
    init_vol: u16,
) -> Result<()> {
    unsafe {
        let mut audio_output = AudioOutput::new(
            shared_buffer,
            renderer_to_decoder_singal_sender,
            decoder_to_renderer_singal_recv,
            control_signal_receiver,
            player_to_ui_singnal_sender,
            init_vol,
        )?;

        let wave_format = get_waveformat_ex(&audio_output.audio_client)?;
        _ = worker_to_player_notofication_signal_sender.send(
            WorkerToPlayerNotification::NoticeAudioDeviceInfo(AudioDeviceInfo {
                sample_rate: wave_format.nSamplesPerSec as usize,
            }),
        );

        audio_output.start()?;
    }
    Ok(())
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn get_waveformat_ex(audio_client: &IAudioClient) -> Result<WAVEFORMATEX> {
    let wave_format_ptr = audio_client.GetMixFormat()?;

    Ok(*wave_format_ptr)
}

enum RenderLoopEndReason {
    Stop,
    Seek,
}

struct AudioOutput<'a> {
    audio_client: IAudioClient,
    render_client: IAudioRenderClient,
    buffer_frame_count: u32,
    shared_buffer: &'a SharedBuffer,
    wasapi_event_hanle: HANDLE,
    renderer_to_decoder_singal_sender: UnboundedSender<RendererToDecoderSsynSignal>,
    decoder_to_renderer_singal_recv: UnboundedReceiver<DecoderToRendererSyncSignal>,
    control_signal_receiver: UnboundedReceiver<RendererControlSignal>,
    player_to_ui_singnal_sender: UnboundedSender<PlayerToUISingnal>,
    current_vol: f32,
    current_seek_no: u64,
}

impl<'a> AudioOutput<'a> {
    pub unsafe fn new(
        shared_buffer: &'a SharedBuffer,
        renderer_to_decoder_singal_sender: UnboundedSender<RendererToDecoderSsynSignal>,
        decoder_to_renderer_singal_recv: UnboundedReceiver<DecoderToRendererSyncSignal>,
        control_signal_receiver: UnboundedReceiver<RendererControlSignal>,
        player_to_ui_singnal_sender: UnboundedSender<PlayerToUISingnal>,
        init_vol: u16,
    ) -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        let handle = {
            let handle = unsafe { CreateEventW(null(), 0, 0, null()) };
            HANDLE(handle)
        };
        let audio_client = unsafe { Self::setup_audio_client(handle) }?;
        let buffer_frame_count = unsafe { audio_client.GetBufferSize() }?;
        let render_client = unsafe { audio_client.GetService()? };

        Ok(Self {
            audio_client,
            buffer_frame_count,
            render_client,
            shared_buffer,
            renderer_to_decoder_singal_sender,
            decoder_to_renderer_singal_recv,
            wasapi_event_hanle: handle,
            control_signal_receiver,
            player_to_ui_singnal_sender,
            current_vol: init_vol as f32 / 100f32,
            current_seek_no: 0,
        })
    }
    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn start(&mut self) -> Result<()> {
        self.audio_client.Start()?;
        let mut paused = false;
        'seek_loop: loop {
            match self.render_loop(&mut paused)? {
                RenderLoopEndReason::Stop => break 'seek_loop,
                RenderLoopEndReason::Seek => continue 'seek_loop,
            }
        }

        Ok(())
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn wait_for_wasapi_event(wasapi_event_hanle: HANDLE) -> Result<()> {
        match WaitForSingleObject(wasapi_event_hanle, INFINITE) {
            windows::Win32::Foundation::WAIT_OBJECT_0 => {}
            WAIT_EVENT(val) => {
                return Err(Error::new(HRESULT(val as i32), "wating event error"));
            }
        };

        Ok(())
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn setup_audio_client(event_handle: HANDLE) -> Result<IAudioClient> {
        // Create device enumerator
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        // Default playback device
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;

        // let device_list = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE);
        // let prop = device_list
        //     .unwrap()
        //     .Item(0)
        //     .unwrap()
        //     .OpenPropertyStore(STGM_READ);

        // use windows::Win32::Devices::FunctionDiscovery::*;
        // let mut ptr = PKEY_Device_FriendlyName;
        // let pro_varant = prop.unwrap().GetValue(&ptr);
        //println!("{}", pro_varant.unwrap());
        // let v = prop.unwrap().GetValue(*PROPERTYKEY::QUERY);
        // Activate IAudioClient
        let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

        let wave_format_ptr = audio_client.GetMixFormat()?;

        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            AUDIO_OUTPUT_BUFFER_DURATION.as_nanos() as i64 / 100, // 100-nanosecond units
            0,
            wave_format_ptr,
            Some(null()),
        )?;
        CoTaskMemFree(Some(wave_format_ptr as _));

        audio_client.SetEventHandle(event_handle)?;
        Ok(audio_client)
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn render_loop(&mut self, paused: &mut bool) -> Result<RenderLoopEndReason> {
        const NUM_OF_BLOCK_LAST_INDEX: usize = NUM_OF_BLOCK - 1;
        let next_block = |read_exclusive| match read_exclusive {
            NUM_OF_BLOCK_LAST_INDEX => 0,
            read_exclusive => read_exclusive + 1,
        };

        #[derive(Clone, Copy)]
        struct EndOfStreamBlockInfo {
            block_idx: usize,
            filled_len: usize,
        }

        let mut end_of_stream_block: Option<EndOfStreamBlockInfo> = None;
        let mut head: usize = 0;
        let mut read_exclusive: usize = 0;

        let signal_to_thread =
            |decoder_to_renderer_singal_recv: &mut UnboundedReceiver<
                DecoderToRendererSyncSignal,
            >,
             renderer_to_decoder_singal_sender: &mut UnboundedSender<
                RendererToDecoderSsynSignal,
            >,
             end_of_stream_block: &mut Option<EndOfStreamBlockInfo>| {
                //info!("singnal_to_thread_rebderer_a");
                match decoder_to_renderer_singal_recv.blocking_recv() {
                    Some(DecoderToRendererSyncSignal::Sync) => {}
                    Some(DecoderToRendererSyncSignal::EndOfStream(EndOfStreamSignal {
                        last_block,
                        filled_len,
                    })) => {
                        *end_of_stream_block = Some(EndOfStreamBlockInfo {
                            block_idx: last_block,
                            filled_len,
                        });
                    }
                    None => {}
                }
                //info!("singnal_to_thread_rebderer_b");

                _ = renderer_to_decoder_singal_sender.send(RendererToDecoderSsynSignal());
            };

        _ = signal_to_thread(
            &mut self.decoder_to_renderer_singal_recv,
            &mut self.renderer_to_decoder_singal_sender,
            &mut end_of_stream_block,
        );
        'l1: loop {
            let get_block_len = |read_exclusive: usize| self.shared_buffer[read_exclusive][0].len();

            macro_rules! control_loop_proc {
                () => {
                    'control_singal_loop: loop {
                        if self.control_signal_receiver.is_empty() && !*paused {
                            break 'control_singal_loop;
                        }

                        match self.control_signal_receiver.blocking_recv() {
                            Some(RendererControlSignal::SetVol(set_vol)) => {
                                self.current_vol = set_vol as f32 / 100f32;
                                continue 'control_singal_loop;
                            }
                            Some(RendererControlSignal::Pause) => {
                                self.audio_client.Stop()?;
                                *paused = true;
                                continue 'control_singal_loop;
                            }
                            Some(RendererControlSignal::Resume) => {
                                self.audio_client.Start()?;
                                *paused = false;
                                continue 'control_singal_loop;
                            }
                            Some(RendererControlSignal::Stop) => {
                                _ = self.audio_client.Stop();
                                _ = signal_to_thread(
                                    &mut self.decoder_to_renderer_singal_recv,
                                    &mut self.renderer_to_decoder_singal_sender,
                                    &mut end_of_stream_block,
                                );
                                break 'l1;
                            }
                            Some(RendererControlSignal::Seek(SeekSignalForRenderer {
                                sync_obj,
                                renderer_to_decoder_sync_signal_sender,
                                decoder_to_renderer_sync_signal_recv,
                                seek_no,
                                ..
                            })) => {
                                self.audio_client.Stop()?;
                                self.audio_client.Reset()?;
                                if !*paused {
                                    self.audio_client.Start()?;
                                }

                                self.renderer_to_decoder_singal_sender =
                                    renderer_to_decoder_sync_signal_sender;
                                self.decoder_to_renderer_singal_recv =
                                    decoder_to_renderer_sync_signal_recv;

                                self.current_seek_no = seek_no;

                                sync_obj.wait(1);
                                //info!("renderer_seek_completed");
                                return Ok(RenderLoopEndReason::Seek);
                            }
                            _ => break 'l1,
                        }
                    }
                };
            }

            control_loop_proc!();

            Self::wait_for_wasapi_event(self.wasapi_event_hanle)?;
            //tokio::time::sleep(Duration::from_millis(10)).await;

            let padding = self.audio_client.GetCurrentPadding()?;
            let os_available = (self.buffer_frame_count - padding) as usize;
            if os_available == 0 {
                continue 'l1;
            }
            let available = cmp::min(os_available, get_block_len(read_exclusive));
            let available = match end_of_stream_block {
                Some(EndOfStreamBlockInfo {
                    block_idx,
                    filled_len,
                }) if block_idx == read_exclusive => {
                    debug!("filled_len: {filled_len}, head: {head}, available: {available}");
                    cmp::min(filled_len - head, available)
                }
                _ => available,
            };
            if available == 0 {
                break 'l1;
            }

            let output_buffer = {
                let ptr = self.render_client.GetBuffer(available as u32)?;
                unsafe { std::slice::from_raw_parts_mut(ptr as *mut f32, available * CHANNEL) }
            };

            let mut fill_buff_within_block = || {
                let get_src_slice =
                    |ch: usize| &self.shared_buffer[read_exclusive][ch][head..head + available];
                let src_slice_ch_0 = get_src_slice(0);
                let src_slice_ch_1 = get_src_slice(1);

                for i in 0..src_slice_ch_0.len() {
                    output_buffer[i * CHANNEL] = src_slice_ch_0[i] * self.current_vol;
                    output_buffer[i * CHANNEL + 1] = src_slice_ch_1[i] * self.current_vol;
                }
            };

            let target_len = head + available;

            use std::cmp::Ordering::*;
            match Ord::cmp(&target_len, &get_block_len(read_exclusive)) {
                Less => {
                    fill_buff_within_block();
                    head = head + available;
                }
                Equal => 'arm1: {
                    fill_buff_within_block();

                    if let Some(EndOfStreamBlockInfo {
                        block_idx,
                        filled_len,
                    }) = end_of_stream_block
                        && block_idx == read_exclusive
                    {
                        head = filled_len;
                        break 'arm1;
                    }

                    signal_to_thread(
                        &mut self.decoder_to_renderer_singal_recv,
                        &mut self.renderer_to_decoder_singal_sender,
                        &mut end_of_stream_block,
                    );

                    read_exclusive = next_block(read_exclusive);
                    head = 0;
                }
                Greater => {
                    {
                        let get_src_slice =
                            |ch: usize| &self.shared_buffer[read_exclusive][ch][head..];
                        let src_slice_current_ch_0 = &get_src_slice(0);
                        let src_slice_current_ch_1 = &get_src_slice(1);

                        for i in 0..src_slice_current_ch_0.len() {
                            output_buffer[i * CHANNEL] =
                                src_slice_current_ch_0[i] * self.current_vol;
                            output_buffer[i * CHANNEL + 1] =
                                src_slice_current_ch_1[i] * self.current_vol;
                        }
                    }

                    signal_to_thread(
                        &mut self.decoder_to_renderer_singal_recv,
                        &mut self.renderer_to_decoder_singal_sender,
                        &mut end_of_stream_block,
                    );

                    read_exclusive = next_block(read_exclusive);

                    let spill_over_len = head + available - get_block_len(read_exclusive);
                    {
                        assert!(spill_over_len < get_block_len(read_exclusive));

                        let get_src_slice =
                            |ch: usize| &self.shared_buffer[read_exclusive][ch][0..spill_over_len];

                        let src_slice_spill_over_ch_0 = get_src_slice(0);
                        let src_slice_spill_over_ch_1 = get_src_slice(1);

                        let split_len_channel_combined = {
                            let split_len_ch = get_block_len(read_exclusive) - head;
                            split_len_ch * CHANNEL
                        };
                        for i in 0..src_slice_spill_over_ch_0.len() {
                            output_buffer[i * CHANNEL + split_len_channel_combined] =
                                src_slice_spill_over_ch_0[i] * self.current_vol;
                            output_buffer[i * CHANNEL + 1 + split_len_channel_combined] =
                                src_slice_spill_over_ch_1[i] * self.current_vol;
                        }
                    }
                    head = spill_over_len;
                }
            };

            self.render_client.ReleaseBuffer(available as u32, 0)?;

            _ = self
                .player_to_ui_singnal_sender
                .send(PlayerToUISingnal::PlayedFrames(PlayedFrames {
                    frames: (available as u32),
                    buffered_frames: padding as u32,
                    seek_no: self.current_seek_no,
                }));

            // if let Some(EndOfStreamBlockInfo {
            //     block_idx,
            //     filled_len,
            // }) = end_of_stream_block
            //     && block_idx == read_exclusive
            // {
            //     debug!(
            //         "XXX head: {}, filled_len: {}, available: {}  XXX",
            //         head, filled_len, available
            //     );
            // }

            if let Some(EndOfStreamBlockInfo {
                block_idx,
                filled_len,
            }) = end_of_stream_block
                && block_idx == read_exclusive
                && head >= filled_len
            {
                'l2: loop {
                    control_loop_proc!();
                    Self::wait_for_wasapi_event(self.wasapi_event_hanle)?;
                    let padding = self.audio_client.GetCurrentPadding()?;

                    _ = self
                        .player_to_ui_singnal_sender
                        .send(PlayerToUISingnal::PlayedFrames(PlayedFrames {
                            frames: (0u32),
                            buffered_frames: padding as u32,
                            seek_no: self.current_seek_no,
                        }));

                    if padding == 0 {
                        break 'l2;
                    }
                }
                break 'l1;
            }
        }

        _ = self.audio_client.Stop();

        _ = self
            .player_to_ui_singnal_sender
            .send(PlayerToUISingnal::RendererPlayCompleted);

        self.control_signal_receiver.close();

        //std::thread::sleep(Duration::from_millis(3000));

        'l3: loop {
            match self.control_signal_receiver.blocking_recv() {
                Some(RendererControlSignal::Seek(signal)) => {
                    self.renderer_to_decoder_singal_sender =
                        signal.renderer_to_decoder_sync_signal_sender;
                    self.decoder_to_renderer_singal_recv.close();
                    signal.sync_obj.wait(1);
                }
                Some(_) => {}
                None => break 'l3,
            };
        }

        debug!("renderer_ exit");

        Ok(RenderLoopEndReason::Stop)
    }
}
impl<'a> Drop for AudioOutput<'a> {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
