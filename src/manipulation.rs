use std::{
    convert::TryFrom,
    fmt::Debug,
    ops::AddAssign,
    panic,
    path::Path,
    sync::atomic::{AtomicPtr, Ordering},
    time::Duration,
};

use symphonia::{
    core::{io::MediaSourceStream, probe::Hint},
    default::get_probe,
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    AudioDecoder::decode_loop,
    AudioOutput,
    DecoderWrapper::DecoderWrapper,
    app::{PlayerToUISingnal, PlayerToUISingnalVeEnabled, TrackInfo},
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

pub struct AudioDeviceInfo {
    pub sample_rate: usize,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum SampleRate {
    R44100 = 44100,
    R88200 = 88200,
    R176400 = 176400,
    R48000 = 48000,
    R96000 = 96000,
    R192000 = 192000,
}

impl TryFrom<usize> for SampleRate {
    type Error = usize;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        use SampleRate::*;
        match value {
            44100 => Ok(R44100),
            88200 => Ok(R88200),
            176400 => Ok(R176400),
            48000 => Ok(R48000),
            96000 => Ok(R96000),
            192000 => Ok(R192000),
            other => Err(other),
        }
    }
}
impl SampleRate {
    fn rawValue(&self) -> usize {
        *self as usize
    }
}

pub enum WorkerToPlayerNotification {
    VeEnabledInfo(VeEnabledInfoFromDecoder),
    VeDisabledSync,
    NoticeAudioDeviceInfo(AudioDeviceInfo),
}
pub struct VeEnabledInfoFromDecoder {
    pub audio_device_sample_rate: usize,
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

struct DecoderInitSignal {
    renderer_sample_rate: usize,
}

pub struct RendererInitSignal {
    pub block_size: usize,
}

pub type SharedBuffer = [[Vec<f32>; CHANNEL]; NUM_OF_BLOCK];

#[derive(Debug)]
pub struct OscilloscopeData(pub Vec<(f64, f64)>);
pub type VESharedBuffer = [[OscilloscopeData; CHANNEL]; NUM_OF_BLOCK_VE];

pub const BLOCK_SIZE: usize = 147 * 160 * 4;
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

    let file_sample_rate = {
        let Some(file_sample_rate) = decoder_wrapper.get_sample_rate() else {
            return;
        };
        file_sample_rate as usize
    };

    let Some(track_duration) = decoder_wrapper.get_duration() else {
        return;
    };

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

    let (
        worker_to_player_notofication_signal_sender,
        mut worker_to_player_notofication_signal_recv,
    ) = unbounded_channel();
    let worker_to_player_notofication_signal_sender_for_renderer =
        worker_to_player_notofication_signal_sender.clone();

    let (decoder_init_signal_sender, mut decoder_init_signal_recv) =
        unbounded_channel::<DecoderInitSignal>();

    let worker_to_player_notofication_signal_sender_for_ve =
        worker_to_player_notofication_signal_sender.clone();
    let decoder_handle = tokio::task::spawn_blocking(move || {
        let shared_buffer = unsafe { retrieve_ref(&shared_buffer_for_decoder) };

        let Some(DecoderInitSignal {
            renderer_sample_rate,
        }) = decoder_init_signal_recv.blocking_recv()
        else {
            return;
        };

        decode_loop(
            &mut decoder_wrapper,
            shared_buffer,
            renderer_sample_rate,
            decoder_to_renderer_sender,
            renderer_to_docoder_reciever,
            decoder_control_signal_recv,
            worker_to_player_notofication_signal_sender,
        );
    });

    for _ in 0..NUM_OF_BLOCK - 2 - BACK_ROOM {
        renderer_to_decoder_sender.send(RendererToDecoderSsynSignal());
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
                            sample_rate: ve_enabled_info.audio_device_sample_rate,
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
            worker_to_player_notofication_signal_sender_for_renderer
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
                Some(WorkerToPlayerNotification::NoticeAudioDeviceInfo(AudioDeviceInfo {
                    sample_rate: audio_device_sample_rate,
                })) => {
                    let (file_sample_rate, audio_device_sample_rate) = (
                        SampleRate::try_from(file_sample_rate).expect("unsupported_sample_rate"),
                        SampleRate::try_from(audio_device_sample_rate)
                            .expect("unsupported_sample_rate"),
                    );
                   

                    decoder_init_signal_sender.send(DecoderInitSignal {
                        renderer_sample_rate: audio_device_sample_rate.rawValue(),
                    });

                    player_to_ui_singnal_sender.send(PlayerToUISingnal::NoticeTrackInfo(
                        TrackInfo {
                            file_sample_rate: file_sample_rate.rawValue(),
                            audio_device_sample_rate: audio_device_sample_rate.rawValue(),
                            track_duration
                        },
                    ));
                }
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
