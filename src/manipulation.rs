use std::{
    fmt::Debug,
    panic,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicPtr, Ordering},
    },
    time::Duration,
};

use futures::{future::join, join};
use symphonia::{
    core::{audio::Signal, io::MediaSourceStream, probe::Hint},
    default::get_probe,
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;
use urlencoding::decode;

use crate::{
    AppState::AppState::AppStateContainer,
    AudioOutput,
    DecoderWrapper::{DecodeResult, DecoderWrapper},
    app::{PlayerToUISingnal, TrackInfo},
};

pub struct DecoderRendererSyncSignal();

#[derive(Debug)]
pub enum RendererControlSignal {
    SetVol(u16),
    Pause,
    Resume,
    Stop,
}
#[derive(Debug)]
pub enum PlayerControlSignal {
    SetVol(u16),
    Pause,
    Resume,
    Stop,
}
pub enum DecoderControlSignal {
    Stop,
}
pub type ChannelData = [Vec<f32>; 8];

pub struct SharedBuffer {
    pub channel_left_data: ChannelData,
    pub channel_right_data: ChannelData,
}

pub const BLOCK_SIZE: usize = 512 * 1024;
pub const CHANNEL: usize = 2;

fn array_init<T: Sized + Debug, const N: usize>(f: impl Fn() -> T) -> [T; N] {
    (0..N).map(|_| f()).collect::<Vec<T>>().try_into().unwrap()
}

pub async fn play_executor(
    playback_file_path: &str,
    player_control_signal_recv: &mut UnboundedReceiver<PlayerControlSignal>,
    player_to_ui_singnal_sender: UnboundedSender<PlayerToUISingnal>,
) {
    let probe = get_probe();
    use std::fs::File;
    let Ok(file) = File::open(playback_file_path) else {
        return;
    };
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    let extension = match Path::extension(Path::new(playback_file_path)) {
        Some(os_str) => match os_str.to_str() {
            Some(str) => str,
            None => "",
        },
        None => "",
    };
    hint.with_extension(extension);
    let probe_result = probe.format(&hint, mss, &Default::default(), &Default::default());
    let format_reader = match probe_result {
        Ok(res) => res.format,
        Err(_) => todo!(),
    };

    let decoder_wrapper = DecoderWrapper::new(format_reader);
    let mut decoder_wrapper = match decoder_wrapper {
        Ok(d) => d,
        Err(_) => todo!(),
    };

    if let (Some(track_duration), Some(sample_rate)) = (
        decoder_wrapper.get_duration(),
        decoder_wrapper.get_sample_rate(),
    ) {
        let track_info = TrackInfo {
            sample_rate,
            track_duration,
        };
        player_to_ui_singnal_sender.send(PlayerToUISingnal::NoticeTrackInfo(track_info));
    }

    let (renderer_to_decoder_sender, renderer_to_docoder_reciever) = mpsc::unbounded_channel();
    let (decoder_to_renderer_sender, decoder_to_renderer_reciever) = mpsc::unbounded_channel();

    let create_vec = || vec![0f32; BLOCK_SIZE];
    let mut shared_buffer = SharedBuffer {
        channel_left_data: array_init(create_vec),
        channel_right_data: array_init(create_vec),
    };

    for _ in 0..6 {
        renderer_to_decoder_sender.send(DecoderRendererSyncSignal());
    }

    let shared_buffer_for_decoder = AtomicPtr::new(&raw mut shared_buffer);
    let shared_buffer_for_renderer = AtomicPtr::new(&raw mut shared_buffer);

    let retrieve_ref = |atomic_ptr: AtomicPtr<SharedBuffer>| unsafe {
        atomic_ptr.load(Ordering::Acquire).as_mut().unwrap()
    };

    let (decoder_control_signal_sender, decoder_control_signal_recv) = unbounded_channel();

    let decoder_handle = tokio::task::spawn_blocking(move || {
        let shared_buffer = retrieve_ref(shared_buffer_for_decoder);
        append_decode_buffer(
            &mut decoder_wrapper,
            shared_buffer,
            decoder_to_renderer_sender,
            renderer_to_docoder_reciever,
            decoder_control_signal_recv,
        );
    });

    let (renderer_control_signal_sender, renderer_control_signal_recv) = unbounded_channel();

    let player_to_ui_singnal = player_to_ui_singnal_sender.clone();
    let renderer_handle = tokio::task::spawn_blocking(move || {
        let shared_buffer = retrieve_ref(shared_buffer_for_renderer);

        AudioOutput::main(
            shared_buffer,
            renderer_to_decoder_sender,
            decoder_to_renderer_reciever,
            renderer_control_signal_recv,
            player_to_ui_singnal,
        )
        .unwrap();
        //let res = res.ok();
    });

    'l1: loop {
        match player_control_signal_recv.recv().await {
            Some(PlayerControlSignal::SetVol(vol)) => {
                renderer_control_signal_sender.send(RendererControlSignal::SetVol(vol));
            }
            Some(PlayerControlSignal::Pause) => {
                renderer_control_signal_sender.send(RendererControlSignal::Pause);
            }
            Some(PlayerControlSignal::Resume) => {
                renderer_control_signal_sender.send(RendererControlSignal::Resume);
            }
            Some(PlayerControlSignal::Stop) => {
                renderer_control_signal_sender.send(RendererControlSignal::Stop);
                decoder_control_signal_sender.send(DecoderControlSignal::Stop);
                break 'l1;
            }
            None => break 'l1,
        };
    }

    join!(renderer_handle, decoder_handle);
}

fn append_decode_buffer(
    decoder_wrapper: &mut DecoderWrapper,
    shared_buffer: &mut SharedBuffer,
    decoder_to_renderer_sender: UnboundedSender<DecoderRendererSyncSignal>,
    mut renderer_to_decoder_singal_recv: UnboundedReceiver<DecoderRendererSyncSignal>,
    mut decoder_control_signal: UnboundedReceiver<DecoderControlSignal>,
) {
    let next_block = |write_end| match write_end {
        7 => 0,
        write_end => write_end + 1,
    };

    let mut singnal_to_thread = || {
        renderer_to_decoder_singal_recv.blocking_recv();
        decoder_to_renderer_sender.send(DecoderRendererSyncSignal())
    };

    let mut write_exclusive: usize = 0;
    let mut count = 0;
    let mut head: usize = 0;
    let mut type_conversion_buff: [Vec<f32>; CHANNEL] =
        array_init(|| vec![0f32;BLOCK_SIZE]);
    'l1: loop {
        if !decoder_control_signal.is_empty() {
            match decoder_control_signal.blocking_recv() {
                Some(DecoderControlSignal::Stop) => {
                    _ = singnal_to_thread();
                    break 'l1;
                }
                None => break 'l1,
            }
        }

        count = count + 1;
        //tokio::time::sleep(Duration::ZERO).await;
        let decoded = decoder_wrapper.decode();

        macro_rules! exclusize_buf_ref_mut {
            ($channel: tt,$write_exclusive:expr) => {
                &mut shared_buffer.$channel[write_exclusive]
            };
        }

        enum DecodeLoopResult {
            Ok,
            Error,
        }

        let mut proc_f32 = |view: &[&[f32]]| -> DecodeLoopResult {
            let view_0 = view[0];
            let view_1 = view[1];
            let mut fill_buff_within_block = || {
                let copy_buff = |exclusive_buf: &mut [f32], view: &[f32]| {
                    let target_slice = &mut exclusive_buf[head..head + view.len()];
                    target_slice.copy_from_slice(view);
                };

                let exclusive_buf_left = exclusize_buf_ref_mut!(channel_left_data, write_exclusive);
                let exclusive_buf_right =
                    exclusize_buf_ref_mut!(channel_right_data, write_exclusive);

                copy_buff(exclusive_buf_left, view_0);
                copy_buff(exclusive_buf_right, view_1);
            };

            let target_len = head + view_0.len();

            use std::cmp::Ordering::*;
            match Ord::cmp(&target_len, &BLOCK_SIZE) {
                Less => {
                    fill_buff_within_block();
                    head = head + view_0.len();
                }
                Equal => {
                    fill_buff_within_block();
                    if let Err(_) = singnal_to_thread() {
                        return DecodeLoopResult::Error;
                    }

                    write_exclusive = next_block(write_exclusive);
                    head = 0;
                }
                Greater => {
                    fn fill_current_block_buff<'a>(
                        head: usize,
                        exclusive_buf: &mut [f32],
                        view: &'a [f32],
                    ) -> &'a [f32] {
                        let target_slice_spill_over = &mut exclusive_buf[head..];

                        let (current_view, spill_over_view) = view.split_at(BLOCK_SIZE - head);
                        target_slice_spill_over.copy_from_slice(current_view);

                        spill_over_view
                    }

                    let spill_over_ch_0 = fill_current_block_buff(
                        head,
                        exclusize_buf_ref_mut!(channel_left_data, write_exclusive),
                        view_0,
                    );
                    let spill_over_ch_1 = fill_current_block_buff(
                        head,
                        exclusize_buf_ref_mut!(channel_right_data, write_exclusive),
                        view_1,
                    );
                    if let Err(_) = singnal_to_thread() {
                        return DecodeLoopResult::Error;
                    }

                    write_exclusive = next_block(write_exclusive);

                    let fill_spill_over_block_buff =
                        |exclusive_buf_spill_over: &mut [f32], spill_over_view: &[f32]| {
                            let target_slice_spill_over =
                                &mut exclusive_buf_spill_over[0..spill_over_view.len()];

                            target_slice_spill_over.copy_from_slice(spill_over_view);
                        };

                    fill_spill_over_block_buff(
                        exclusize_buf_ref_mut!(channel_left_data, write_exclusive),
                        spill_over_ch_0,
                    );
                    fill_spill_over_block_buff(
                        exclusize_buf_ref_mut!(channel_right_data, write_exclusive),
                        spill_over_ch_1,
                    );

                    head = spill_over_ch_0.len();
                }
            }
            DecodeLoopResult::Ok
        };

        use symphonia::core::audio::AudioBufferRef::*;
        match decoded {
            DecodeResult::Buf(audio_buffer_ref) => match audio_buffer_ref {
                F32(cow) => {
                    let view = [0, 1].map(|ch| cow.chan(ch));
                    if let DecodeLoopResult::Error = proc_f32(&view) {
                        break 'l1;
                    }
                }
                S16(cow) => {
                    let f = |x| (x as f32) / (i16::MAX as f32);
                    for ch in 0..CHANNEL {
                        for i in 0..cow.chan(ch).len() {
                            type_conversion_buff[ch][i] = f(cow.chan(ch)[i])
                        }
                    }
                    let type_conversion_buff_view =
                        |i: usize| &type_conversion_buff[i][0..cow.chan(i).len()];
                    let target = [0, 1].map(|i| type_conversion_buff_view(i));
                    proc_f32(&target);
                   
                }
                _ => {}
            },
            DecodeResult::Err(error) => {
                return;
                //println!("error: {error}")
            }
            DecodeResult::EndOfStream => {
                let buf_ref_s = [
                    exclusize_buf_ref_mut!(channel_left_data, write_exclusive),
                    exclusize_buf_ref_mut!(channel_right_data, write_exclusive),
                ];
                for channel_data_ref in buf_ref_s {
                    channel_data_ref[head..].fill(0f32);
                }

                singnal_to_thread();
                break 'l1;
            }
            DecodeResult::None => continue,
        }
    }
}
