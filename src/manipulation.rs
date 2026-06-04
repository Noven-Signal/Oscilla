use std::{
    cmp, fmt::Debug, ops::{Add, AddAssign}, panic, path::Path, sync::{
        Arc,
        atomic::{AtomicPtr, Ordering},
    }, time::Duration
};

use futures::{future::join, join};
use rand::rand_core::block;
use symphonia::{
    core::{audio::Signal, io::MediaSourceStream, probe::Hint},
    default::get_probe,
};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::spawn_blocking,
};
use tokio_util::sync::CancellationToken;
use tracing::info;
use urlencoding::decode;

use crate::{
    AppState::AppState::AppStateContainer,
    AudioOutput,
    DecoderWrapper::{DecodeResult, DecoderWrapper},
    app::{PlayedFrames, PlayerToUISingnal, PlayerToUISingnalVeEnabled, TrackInfo},
    utils::array_init,
    visual_effects::Oscilloscope::ve_loop,
};

pub enum DecoderToRendererSyncSignal {
    Sync,
    EndOfStream(EndOfStreamSignal),
}
pub struct EndOfStreamSignal {
    pub last_block: usize,
}
pub struct RendererToDecoderSsynSignal();

pub struct DecorderToVeSyncSignal();
pub struct VeToDecoderSyncSignal();

pub enum WorkerToPlayerNotification {
    VeEnabledInfo(VeEnabledInfoFromDecoder),
    VeDisabledSync,
}
pub struct VeEnabledInfoFromDecoder {
    pub sample_rate: usize,
    pub ve_start_read_exclusize: usize,
    pub decorder_to_ve_signal_recv: UnboundedReceiver<DecorderToVeSyncSignal>,
    pub ve_to_decoder_signal_sender: UnboundedSender<VeToDecoderSyncSignal>,
    pub ve_buffer_duration_offset_sec: f64,
    pub ve_shared_buffer: AtomicPtr<VESharedBuffer>,
}

pub struct VeEnabledInfoFromPlayerToVe {
    pub sample_rate: usize,
    pub renderer_read_exclusize: usize,
    pub decorder_to_ve_signal_recv: UnboundedReceiver<DecorderToVeSyncSignal>,
    pub ve_to_decoder_signal_sender: UnboundedSender<VeToDecoderSyncSignal>,

    pub ve_to_ui_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    pub ui_to_ve_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
    pub ve_shared_buffer: AtomicPtr<VESharedBuffer>,
    pub cancellation_token: CancellationToken,
}

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
    VeEnabled(AtomicPtr<VESharedBuffer>),
    VeDisabled,
}
pub enum DecoderControlSignal {
    VeEnabled(VeEnabledSignalFromPlayerToDecoder),
    VeDisabled,
    Stop,
}

pub struct VeEnabledSignalFromPlayerToDecoder {
    sample_rate: usize,
    pub decoder_to_ve_signal_sender: UnboundedSender<DecorderToVeSyncSignal>,
    pub decorder_to_ve_signal_recv: UnboundedReceiver<DecorderToVeSyncSignal>,
    pub ve_to_decoder_signal_sender: UnboundedSender<VeToDecoderSyncSignal>,
    pub ve_to_decoder_signal_recv: UnboundedReceiver<VeToDecoderSyncSignal>,
    pub ve_shared_buffer: AtomicPtr<VESharedBuffer>,
}

pub enum VeControlSignal {
    VeEnabled(VeControlSignalVeEnabled),
    VeDisabled,
    PlayStop,
}
pub struct VeControlSignalVeEnabled {
    pub ve_enabled_info: VeEnabledInfoFromDecoder,
    pub ve_to_ui_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    pub ui_to_ve_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
}

// pub struct VeStartInfo {
//     pub sample_rate: usize,
//     pub start_block: usize,
// }

pub type SharedBuffer = [[Vec<f32>; CHANNEL]; NUM_OF_BLOCK];

#[derive(Debug)]
pub struct OscilloscopeData(pub Vec<(f64, f64)>);
pub type VESharedBuffer = [[OscilloscopeData; CHANNEL]; NUM_OF_BLOCK_VE];

pub const BLOCK_SIZE: usize = 16 * 1024;
pub const CHANNEL: usize = 2;
pub const NUM_OF_BLOCK: usize = 16;
pub const BACK_ROOM: usize = 4;
pub const NUM_OF_BLOCK_VE: usize = 180;
pub const AUDIO_OUTPUT_BUFFER_DURATION: Duration = Duration::from_secs(1);

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

    let Some(sample_rate) = decoder_wrapper.get_sample_rate() else {
        return;
    };

    if let Some(track_duration) = decoder_wrapper.get_duration() {
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

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn retrieve_ref<'a, T>(atomic_ptr: &AtomicPtr<T>) -> &'a mut T {
        atomic_ptr.load(Ordering::Acquire).as_mut().unwrap()
    }

    let (decoder_control_signal_sender, decoder_control_signal_recv) = unbounded_channel();
    // let (decoder_to_ve_signal_sender, decoder_to_ve_signal_recv) = unbounded_channel();

    // let (ve_to_decoder_signal_sender, ve_to_decoder_signal_recv) = unbounded_channel();
    let (
        worker_to_player_notofication_signal_sender,
        mut worker_to_player_notofication_signal_recv,
    ) = unbounded_channel();
    let worker_to_player_notofication_signal_sender_for_ve =
        worker_to_player_notofication_signal_sender.clone();
    let decoder_handle = tokio::task::spawn_blocking(move || {
        let shared_buffer = unsafe { retrieve_ref(&shared_buffer_for_decoder) };
        append_decode_buffer(
            &mut decoder_wrapper,
            shared_buffer,
            decoder_to_renderer_sender,
            renderer_to_docoder_reciever,
            // decoder_to_ve_signal_sender,
            // ve_to_decoder_signal_recv,
            decoder_control_signal_recv,
            worker_to_player_notofication_signal_sender,
        );
    });

    for _ in 0..NUM_OF_BLOCK - 2 - BACK_ROOM {
        renderer_to_decoder_sender.send(RendererToDecoderSsynSignal());
        //ve_to_decoder_signal_sender.send(VeToDecoderSyncSignal());
    }

    let (ve_control_signal_sender, ve_control_signal_recv) = unbounded_channel();

    let player_to_ui_singnal_sender_for_ve = player_to_ui_singnal_sender.clone();
    let visual_effect_handle = tokio::task::spawn(async move {
        let shared_buffer = unsafe { retrieve_ref(&shared_buffer_for_ve) };

        let mut ve_control_signal_recv = ve_control_signal_recv;

        let (sender, mut recv) = unbounded_channel::<VeEnabledInfoFromPlayerToVe>();

        let handle = tokio::task::spawn_blocking(move || {
            let mut disable_call_count_ve = 0;
            'l1: loop {
                match recv.blocking_recv() {
                    Some(mut signal) => {
                        let ve_shared_buffer = unsafe { retrieve_ref(&signal.ve_shared_buffer) };

                        player_to_ui_singnal_sender_for_ve.send(PlayerToUISingnal::VeEnabledSync);

                        ve_loop(
                            shared_buffer,
                            ve_shared_buffer,
                            signal.ve_to_decoder_signal_sender,
                            signal.decorder_to_ve_signal_recv,
                            &signal.ve_to_ui_signal_sender,
                            &mut signal.ui_to_ve_signal_recv,
                            signal.cancellation_token,
                            signal.sample_rate,
                            signal.renderer_read_exclusize,
                        );
                        worker_to_player_notofication_signal_sender_for_ve
                            .send(WorkerToPlayerNotification::VeDisabledSync);
                        disable_call_count_ve.add_assign(1);
                        info!("disable_call_count_ve: {disable_call_count_ve}");

                        //player_to_ui_singnal_sender_for_ve.send(PlayerToUISingnal::VeDisabled);
                    }
                    None => break 'l1,
                }
            }
        });

        let event_block = async move {
            let mut cancellation_token: Option<CancellationToken> = None;
            'l1: loop {
                match ve_control_signal_recv.recv().await {
                    Some(VeControlSignal::VeEnabled(VeControlSignalVeEnabled {
                        ve_enabled_info,
                        ve_to_ui_signal_sender,
                        ui_to_ve_signal_recv,
                    })) => {
                        let cancellation_token = {
                            let token = CancellationToken::new();
                            cancellation_token = Some(token.clone());
                            token
                        };

                        _ = sender.send(VeEnabledInfoFromPlayerToVe {
                            sample_rate: ve_enabled_info.sample_rate,
                            renderer_read_exclusize: ve_enabled_info.ve_start_read_exclusize,
                            decorder_to_ve_signal_recv: ve_enabled_info.decorder_to_ve_signal_recv,
                            ve_to_decoder_signal_sender: ve_enabled_info
                                .ve_to_decoder_signal_sender,
                            ve_to_ui_signal_sender,
                            ui_to_ve_signal_recv,
                            ve_shared_buffer: ve_enabled_info.ve_shared_buffer,
                            cancellation_token,
                        })
                    }
                    Some(VeControlSignal::PlayStop) => {
                        if let Some(token) = &cancellation_token {
                            token.cancel();
                        }
                        break 'l1;
                    }
                    Some(VeControlSignal::VeDisabled) => {
                        if let Some(token) = &cancellation_token {
                            token.cancel();
                        }
                    }
                    None => break 'l1,
                };
            }
        };

        _ = tokio::join!(handle, event_block);
    });

    let (renderer_control_signal_sender, renderer_control_signal_recv) = unbounded_channel();

    let player_to_ui_singnal = player_to_ui_singnal_sender.clone();
    let renderer_handle = tokio::task::spawn_blocking(move || {
        let shared_buffer = unsafe { retrieve_ref(&shared_buffer_for_renderer) };

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

    let player_control_signal_loop_task = async {
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
                    ve_control_signal_sender.send(VeControlSignal::PlayStop);
                    break 'l1;
                }
                PlayerControlSignal::VeEnabled(ve_shared_buffer) => {
                    let (decoder_to_ve_signal_sender, decorder_to_ve_signal_recv) =
                        unbounded_channel();
                    let (ve_to_decoder_signal_sender, ve_to_decoder_signal_recv) =
                        unbounded_channel();
                    let ve_enabled_signal = VeEnabledSignalFromPlayerToDecoder {
                        sample_rate: sample_rate as usize,
                        decoder_to_ve_signal_sender,
                        decorder_to_ve_signal_recv,
                        ve_to_decoder_signal_sender,
                        ve_to_decoder_signal_recv,
                        ve_shared_buffer,
                    };

                    decoder_control_signal_sender
                        .send(DecoderControlSignal::VeEnabled(ve_enabled_signal));
                }
                PlayerControlSignal::VeDisabled => {
                    decoder_control_signal_sender.send(DecoderControlSignal::VeDisabled);
                    ve_control_signal_sender.send(VeControlSignal::VeDisabled);
                }
            };
        }
    };

    let player_notification_loop_task = async {
        let mut disabled_count = 0;
        'l2: loop {
            match worker_to_player_notofication_signal_recv.recv().await {
                Some(WorkerToPlayerNotification::VeEnabledInfo(ve_enabled_info)) => {
                    let (ve_to_ui_signal_sender, ve_to_ui_signal_recv) = unbounded_channel();
                    let (ui_to_ve_signal_sender, ui_to_ve_signal_recv) = unbounded_channel();

                    for _ in 0..NUM_OF_BLOCK_VE - 2 {
                        ui_to_ve_signal_sender.send(UiVEThreadSyncSignal());
                    }

                    let ve_buffer_duration_offset_sec =
                        ve_enabled_info.ve_buffer_duration_offset_sec;
                    let send_signal = VeControlSignalVeEnabled {
                        ve_enabled_info,
                        ve_to_ui_signal_sender,
                        ui_to_ve_signal_recv,
                    };
                    ve_control_signal_sender.send(VeControlSignal::VeEnabled(send_signal));
                    player_to_ui_singnal_sender.send(PlayerToUISingnal::VeEnabled(
                        PlayerToUISingnalVeEnabled {
                            ui_to_ve_signal_sender,
                            ve_to_ui_signal_recv,
                            ve_buffer_duration_offset_sec,
                        },
                    ));
                }
                Some(WorkerToPlayerNotification::VeDisabledSync) => match disabled_count {
                    0 => {
                        disabled_count = disabled_count + 1;
                    }
                    1 => {
                        _ = player_to_ui_singnal_sender.send(PlayerToUISingnal::VeDisabled);
                        disabled_count = 0;
                    }
                    _ => panic!(),
                },
                None => break 'l2,
            };
        }
    };

    tokio::join!(
        player_control_signal_loop_task,
        player_notification_loop_task,
        renderer_handle,
        decoder_handle,
        visual_effect_handle
    );
}

fn append_decode_buffer(
    decoder_wrapper: &mut DecoderWrapper,
    shared_buffer: &mut SharedBuffer,
    decoder_to_renderer_sender: UnboundedSender<DecoderToRendererSyncSignal>,
    mut renderer_to_decoder_singal_recv: UnboundedReceiver<RendererToDecoderSsynSignal>,
    // decoder_to_ve_signal_sender: UnboundedSender<DecorderToVeSyncSignal>,
    // mut ve_to_decoder_signal_recv: UnboundedReceiver<VeToDecoderSyncSignal>,
    mut decoder_control_signal: UnboundedReceiver<DecoderControlSignal>,
    mut decoder_to_player_notification_signal: UnboundedSender<WorkerToPlayerNotification>,
) {
    struct VeSignal {
        pub decoder_to_ve_signal_sender: UnboundedSender<DecorderToVeSyncSignal>,
        pub ve_to_decoder_signal_recv: UnboundedReceiver<VeToDecoderSyncSignal>,
    }
    let mut ve_signal: Option<VeSignal> = None;

    // let decoder_to_ve_signal_sender: Option<UnboundedSender<DecorderToVeSyncSignal>> = Some(decoder_to_ve_signal_sender);
    // let mut ve_to_decoder_signal_recv: Option<UnboundedReceiver<VeToDecoderSyncSignal>> = Some(ve_to_decoder_signal_recv);
    const NUM_OF_BLOCK_LAST_INDEX: usize = NUM_OF_BLOCK - 1;
    let next_block = |write_end| match write_end {
        NUM_OF_BLOCK_LAST_INDEX => 0,
        write_end => write_end + 1,
    };

    let mut singnal_to_thread =
        |renderer_to_decoder_singal_recv: &mut UnboundedReceiver<RendererToDecoderSsynSignal>,
         ve_signal: &mut Option<VeSignal>,
         send_signal: DecoderToRendererSyncSignal| {
            renderer_to_decoder_singal_recv.blocking_recv();
            if let Some(VeSignal {
                ve_to_decoder_signal_recv,
                decoder_to_ve_signal_sender,
            }) = ve_signal
            {
                ve_to_decoder_signal_recv.blocking_recv();
                decoder_to_ve_signal_sender.send(DecorderToVeSyncSignal());
            }
            decoder_to_renderer_sender.send(send_signal)
        };
    let mut singnal_to_thread_sync =
        |renderer_to_decoder_singal_recv: &mut UnboundedReceiver<RendererToDecoderSsynSignal>,
         ve_signal: &mut Option<VeSignal>| {
            singnal_to_thread(
                renderer_to_decoder_singal_recv,
                ve_signal,
                DecoderToRendererSyncSignal::Sync,
            )
        };

    let mut write_exclusive: usize = 0;
    let mut count = 0;
    let mut head: usize = 0;
    let mut type_conversion_buff: [Vec<f32>; CHANNEL] = array_init(|| vec![0f32; BLOCK_SIZE]);
    let mut block_count: usize = 0;
    // let mut ve_enabled = false;
    let mut disable_call_couunt_decoder = 0;

    let Some(file_sample_rate) = decoder_wrapper.get_sample_rate() else {
        return;
    };

    'l1: loop {
        if !decoder_control_signal.is_empty() {
            match decoder_control_signal.blocking_recv() {
                Some(DecoderControlSignal::Stop) => {
                    _ = singnal_to_thread_sync(
                        &mut renderer_to_decoder_singal_recv,
                        &mut ve_signal,
                    );
                    break 'l1;
                }
                Some(DecoderControlSignal::VeEnabled(VeEnabledSignalFromPlayerToDecoder {
                    decoder_to_ve_signal_sender,
                    decorder_to_ve_signal_recv,
                    ve_to_decoder_signal_sender,
                    ve_to_decoder_signal_recv,
                    sample_rate,
                    ve_shared_buffer,
                })) => {
                    let len = renderer_to_decoder_singal_recv.len();

                    let ve_start_read_exclusize = (write_exclusive + len + 1) % NUM_OF_BLOCK;
                    for _ in 0..len {
                        ve_to_decoder_signal_sender.send(VeToDecoderSyncSignal());
                    }

                    for _ in 0..NUM_OF_BLOCK - len - 2 {
                        decoder_to_ve_signal_sender.send(DecorderToVeSyncSignal());
                    }

                    let ve_buffer_duration_offset_sec = {
                        let target_block_count =
                            block_count as i32 + len as i32 + 1 - NUM_OF_BLOCK as i32;

                        let target_duration = (target_block_count as f64 * BLOCK_SIZE as f64)
                            / file_sample_rate as f64;

                        target_duration
                    };

                    decoder_to_player_notification_signal.send(
                        WorkerToPlayerNotification::VeEnabledInfo(VeEnabledInfoFromDecoder {
                            sample_rate,
                            ve_start_read_exclusize,
                            decorder_to_ve_signal_recv,
                            ve_to_decoder_signal_sender,
                            ve_buffer_duration_offset_sec,
                            ve_shared_buffer,
                        }),
                    );

                    ve_signal = Some(VeSignal {
                        decoder_to_ve_signal_sender,
                        ve_to_decoder_signal_recv,
                    })
                }
                Some(DecoderControlSignal::VeDisabled) => {
                    ve_signal = None;

                    _ = decoder_to_player_notification_signal
                        .send(WorkerToPlayerNotification::VeDisabledSync);
                    disable_call_couunt_decoder.add_assign(1);
                        info!("disable_call_couunt_decoder: {disable_call_couunt_decoder}");
                }
                None => break 'l1,
            }
        }

        let decoded = decoder_wrapper.decode();

        enum DecodeLoopResult {
            Ok,
            Error,
        }

        let mut proc_f32 = |view: &[&[f32]]| -> DecodeLoopResult {
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

            let target_len = head + view[0].len();

            use std::cmp::Ordering::*;
            match Ord::cmp(&target_len, &BLOCK_SIZE) {
                Less => {
                    fill_buff_within_block();
                    head = head + view[0].len();
                }
                Equal => {
                    fill_buff_within_block();
                    if let Err(_) =
                        singnal_to_thread_sync(&mut renderer_to_decoder_singal_recv, &mut ve_signal)
                    {
                        return DecodeLoopResult::Error;
                    }

                    write_exclusive = next_block(write_exclusive);
                    block_count = block_count + 1;
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

                    if let Err(_) =
                        singnal_to_thread_sync(&mut renderer_to_decoder_singal_recv, &mut ve_signal)
                    {
                        return DecodeLoopResult::Error;
                    }

                    write_exclusive = next_block(write_exclusive);
                    block_count = block_count + 1;

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

                _ = singnal_to_thread(
                    &mut renderer_to_decoder_singal_recv,
                    &mut ve_signal,
                    DecoderToRendererSyncSignal::EndOfStream(EndOfStreamSignal {
                        last_block: write_exclusive,
                    }),
                );
                break 'l1;
            }
            DecodeResult::None => continue,
        }

        count = count + 1;
    }
}
