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
    utils::array_init,
    visual_effects::oscilloscope::ve_loop,
};

pub struct DecoderRendererSyncSignal();
pub struct DecoderVisualEffectThreadSyncSignal();
pub struct UiVEThreadSyncSignal();

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
    NoticeSampleRate(usize),
}
pub enum DecoderControlSignal {
    Stop,
}

pub enum VeControlSignal {
    NoticeSampleRate(usize),
}

pub type SharedBuffer = [[Vec<f32>; CHANNEL]; NUM_OF_BLOCK];

#[derive(Debug)]
pub struct OscilloscopeData(pub Vec<(f64,f64)>);
pub type VESharedBuffer = [[OscilloscopeData; CHANNEL]; NUM_OF_BLOCK_VE];

pub const BLOCK_SIZE: usize = 16 * 1024;
pub const CHANNEL: usize = 2;
pub const NUM_OF_BLOCK: usize = 8;
pub const NUM_OF_BLOCK_VE: usize = 8;

pub async fn play_executor(
    playback_file_path: &str,
    player_control_signal_recv: &mut UnboundedReceiver<PlayerControlSignal>,
    player_to_ui_singnal_sender: UnboundedSender<PlayerToUISingnal>,
    ve_to_ui_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    ui_to_ve_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
    ve_shared_buffer: AtomicPtr<Option<VESharedBuffer>>,
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

    let mut shared_buffer = array_init(|| array_init(|| vec![0f32; BLOCK_SIZE]));

    let shared_buffer_for_decoder = AtomicPtr::new(&raw mut shared_buffer);
    let shared_buffer_for_renderer = AtomicPtr::new(&raw mut shared_buffer);
    let shared_buffer_for_ve = AtomicPtr::new(&raw mut shared_buffer);

    let retrieve_ref = |atomic_ptr: AtomicPtr<SharedBuffer>| unsafe {
        atomic_ptr.load(Ordering::Acquire).as_mut().unwrap()
    };

    let (decoder_control_signal_sender, decoder_control_signal_recv) = unbounded_channel();
    let (decoder_to_ve_signal_sender, decoder_to_ve_signal_recv) = unbounded_channel();

    let (ve_to_decoder_signal_sender, ve_to_decoder_signal_recv) = unbounded_channel();
    let decoder_handle = tokio::task::spawn_blocking(move || {
        let shared_buffer = retrieve_ref(shared_buffer_for_decoder);
        append_decode_buffer(
            &mut decoder_wrapper,
            shared_buffer,
            decoder_to_renderer_sender,
            renderer_to_docoder_reciever,
            decoder_to_ve_signal_sender,
            ve_to_decoder_signal_recv,
            decoder_control_signal_recv,
        );
    });

    // let mut ve_shared_buffer: VESharedBuffer =
    //     array_init(|| array_init(|| OscilloscopeData(vec![0f32; BLOCK_SIZE])));

    // let (ve_to_ui_signal_sender, ve_to_ui_signal_recv) = unbounded_channel();
    // let (ui_to_ve_signal_sender, ui_to_ve_signal_recv) = unbounded_channel();

    for _ in 0..NUM_OF_BLOCK - 2 {
        renderer_to_decoder_sender.send(DecoderRendererSyncSignal());
        ve_to_decoder_signal_sender.send(DecoderVisualEffectThreadSyncSignal());
    }

    let (ve_control_signal_sender, ve_control_signal_recv) = unbounded_channel();

    let visual_effect_handle = tokio::task::spawn_blocking(move || {
        let shared_buffer = retrieve_ref(shared_buffer_for_ve);
        let ve_shared_buffer =
            unsafe { ve_shared_buffer.load(Ordering::Acquire).as_mut().unwrap() };
        ve_loop(
            shared_buffer,
            ve_shared_buffer,
            ve_to_decoder_signal_sender,
            decoder_to_ve_signal_recv,
            ve_to_ui_signal_sender,
            ui_to_ve_signal_recv,
            ve_control_signal_recv,
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
        let message = match player_control_signal_recv.recv().await {
            Some(msg) => msg,
            None => break 'l1,
        };
        match message {
            PlayerControlSignal::SetVol(vol) => {
                renderer_control_signal_sender.send(RendererControlSignal::SetVol(vol));
            }
            PlayerControlSignal::Pause => {
                renderer_control_signal_sender.send(RendererControlSignal::Pause);
            }
            PlayerControlSignal::Resume => {
                renderer_control_signal_sender.send(RendererControlSignal::Resume);
            }
            PlayerControlSignal::Stop => {
                renderer_control_signal_sender.send(RendererControlSignal::Stop);
                decoder_control_signal_sender.send(DecoderControlSignal::Stop);
                break 'l1;
            }
            PlayerControlSignal::NoticeSampleRate(sample_rate) => {
                ve_control_signal_sender.send(VeControlSignal::NoticeSampleRate(sample_rate));
            }
        };
    }

    join!(renderer_handle, decoder_handle, visual_effect_handle);
}

fn append_decode_buffer(
    decoder_wrapper: &mut DecoderWrapper,
    shared_buffer: &mut SharedBuffer,
    decoder_to_renderer_sender: UnboundedSender<DecoderRendererSyncSignal>,
    mut renderer_to_decoder_singal_recv: UnboundedReceiver<DecoderRendererSyncSignal>,
    decoder_to_ve_signal_sender: UnboundedSender<DecoderVisualEffectThreadSyncSignal>,
    mut ve_to_decoder_signal_recv: UnboundedReceiver<DecoderVisualEffectThreadSyncSignal>,
    mut decoder_control_signal: UnboundedReceiver<DecoderControlSignal>,
) {
    let next_block = |write_end| match write_end {
        7 => 0,
        write_end => write_end + 1,
    };

    let mut singnal_to_thread = || {
        renderer_to_decoder_singal_recv.blocking_recv();
        ve_to_decoder_signal_recv.blocking_recv();
        decoder_to_ve_signal_sender.send(DecoderVisualEffectThreadSyncSignal());
        decoder_to_renderer_sender.send(DecoderRendererSyncSignal())
    };

    let mut write_exclusive: usize = 0;
    let mut count = 0;
    let mut head: usize = 0;
    let mut type_conversion_buff: [Vec<f32>; CHANNEL] = array_init(|| vec![0f32; BLOCK_SIZE]);
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

        // macro_rules! exclusize_buf_ref_mut {
        //     ($channel: tt,$write_exclusive:expr) => {
        //         &mut shared_buffer.$channel[write_exclusive]
        //     };
        // }

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

                let exclusive_buff = &mut shared_buffer[write_exclusive];

                for ch in 0..CHANNEL {
                    copy_buff(exclusive_buff[ch].as_mut_slice(), view[ch]);
                }
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

                    let spill_over = [0, 1].map(|ch| {
                        fill_current_block_buff(
                            head,
                            shared_buffer[write_exclusive][ch].as_mut_slice(),
                            view[ch],
                        )
                    });

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

                    for ch in 0..CHANNEL {
                        fill_spill_over_block_buff(
                            shared_buffer[write_exclusive][ch].as_mut_slice(),
                            spill_over[ch],
                        );
                    }

                    head = spill_over[0].len();
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
                for channel_data_ref in &mut shared_buffer[write_exclusive] {
                    channel_data_ref[head..].fill(0f32);
                }

                singnal_to_thread();
                break 'l1;
            }
            DecodeResult::None => continue,
        }
    }
}
