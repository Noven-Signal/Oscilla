use std::{
    convert::TryFrom, fmt::{self, Debug, Display}, panic, pin::Pin, sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicPtr, AtomicU8, Ordering},
    }, time::Duration,
};

use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::JoinError,
};
use tokio_util::sync::CancellationToken;

use crate::{
    app::{PlayerToUISingnal, PlayerToUISingnalVeEnabled, SeekCompleteSignal, TrackInfo},
    audio_decoder::decode_loop,
    audio_output,
    decoder_wrapper::DecoderWrapper,
    utils::array_init,
    visual_effects::oscilloscope::{VeControlSignalInner, ve_loop},
};

pub enum DecoderToRendererSyncSignal {
    Sync,
    EndOfStream(EndOfStreamSignal),
}
pub struct EndOfStreamSignal {
    pub last_block: usize,
    pub filled_len: usize,
}
pub struct RendererToDecoderSsynSignal();

pub struct DecorderToVeSyncSignal();
pub struct VeToDecoderSyncSignal();

pub struct AudioDeviceInfo {
    pub sample_rate: usize,
}
#[derive(Debug)]
pub struct SeekTimingSyncObj {
    pub mutex: Mutex<AtomicU8>,
    pub condvar: Condvar,
}
impl SeekTimingSyncObj {
    pub fn new() -> Self {
        SeekTimingSyncObj {
            mutex: Mutex::new(AtomicU8::new(0)),
            condvar: Condvar::new(),
        }
    }

    pub fn wait(&self, increment: u8) {
        let SeekTimingSyncObj { mutex, condvar } = self;
        let mutex_guard = mutex.lock().unwrap();
        mutex_guard.fetch_add(increment, Ordering::SeqCst);
        drop(mutex_guard);
        let _mutex_guard = condvar
            .wait_while(mutex.lock().unwrap(), |state| {
                //info!("{}", state.load(Ordering::SeqCst));
                state.load(Ordering::SeqCst) < 3
            })
            .unwrap();
        condvar.notify_all();
    }
}
#[derive(Debug)]
pub struct SeekTimingSyncState {
    pub init_sync: SeekTimingSyncObj,
    pub complete_sync: SeekTimingSyncObj,
}
#[derive(Debug)]
pub struct SeekSignalForDecoderVeChannels {
    pub decoder_to_ve_sync_signal_sender: UnboundedSender<DecorderToVeSyncSignal>,
    pub ve_to_decoder_sync_signal_recv: UnboundedReceiver<VeToDecoderSyncSignal>,
}
#[derive(Debug)]
pub struct SeekSignalForDecoder {
    pub sync_obj: Arc<SeekTimingSyncState>,
    pub target_duration: Duration,
    pub decoder_to_renderer_sync_signal_sender: UnboundedSender<DecoderToRendererSyncSignal>,
    pub renderer_to_decoder_sync_signal_recv: UnboundedReceiver<RendererToDecoderSsynSignal>,
    pub seek_signal_for_decoder_ve_channels: Option<SeekSignalForDecoderVeChannels>,
    pub seek_no: u64,
}
#[derive(Debug)]
pub struct SeekSignalForRenderer {
    pub sync_obj: Arc<SeekTimingSyncState>,
    pub renderer_to_decoder_sync_signal_sender: UnboundedSender<RendererToDecoderSsynSignal>,
    pub decoder_to_renderer_sync_signal_recv: UnboundedReceiver<DecoderToRendererSyncSignal>,
    pub seek_no: u64,
}

#[derive(Debug)]
pub struct SeekSignalForVe {
    pub sync_obj: Arc<SeekTimingSyncState>,
    pub ve_to_decoder_sync_signal_sender: UnboundedSender<VeToDecoderSyncSignal>,
    pub decoder_to_ve_sync_signal_recv: UnboundedReceiver<DecorderToVeSyncSignal>,
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
    pub fn raw_value(&self) -> usize {
        *self as usize
    }
}

pub struct SeekCompleteFromVeSignal {
    pub ui_to_ve_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    pub ve_to_ui_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
}

pub struct SeekCompleteFromDecoderSignal {
    pub actual_seek_duration_sec: f64,
    pub seek_no: u64,
    pub ve_enabled: bool,
}

pub enum WorkerToPlayerNotification {
    VeEnabledInfo(VeEnabledInfoFromDecoder),
    VeDisabledSync,
    NoticeAudioDeviceInfo(AudioDeviceInfo),
    SeekCompleteFromVe(SeekCompleteFromVeSignal),
    SeekCompleteFromDecoder(SeekCompleteFromDecoderSignal),
    Error,
}
pub struct VeEnabledInfoFromDecoder {
    pub audio_device_sample_rate: usize,
    pub ve_start_read_exclusize: usize,
    pub decorder_to_ve_signal_recv: UnboundedReceiver<DecorderToVeSyncSignal>,
    pub ve_to_decoder_signal_sender: UnboundedSender<VeToDecoderSyncSignal>,
    pub ve_buffer_duration_offset_sec: f64,
    pub ve_shared_buffer: AtomicPtr<Pin<Box<VESharedBuffer>>>,
}

pub struct VeEnabledInfoFromPlayerToVe {
    pub sample_rate: usize,
    pub renderer_read_exclusize: usize,
    pub decorder_to_ve_signal_recv: UnboundedReceiver<DecorderToVeSyncSignal>,
    pub ve_to_decoder_signal_sender: UnboundedSender<VeToDecoderSyncSignal>,

    pub ve_to_ui_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    pub ui_to_ve_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
    pub ve_shared_buffer: AtomicPtr<Pin<Box<VESharedBuffer>>>,
    pub ve_control_signal_inner_recv: UnboundedReceiver<VeControlSignalInner>,
}

#[derive(Debug)]
pub struct UiToPlayerSeekSignal {
    pub target_duration: Duration,
    pub seek_no: u64,
}

pub struct UiVEThreadSyncSignal();

#[derive(Debug)]
pub enum RendererControlSignal {
    SetVol(u16),
    Pause,
    Resume,
    Stop,
    Seek(SeekSignalForRenderer),
}
#[derive(Debug)]
pub enum PlayerControlSignal {
    SetVol(u16),
    Pause,
    Resume,
    Stop,
    VeEnabled(AtomicPtr<Pin<Box<VESharedBuffer>>>),
    VeDisabled,
    Seek(UiToPlayerSeekSignal),
}
pub enum DecoderControlSignal {
    VeEnabled(VeEnabledSignalFromPlayerToDecoder),
    VeDisabled,
    Stop,
    Seek(SeekSignalForDecoder),
}

pub struct VeEnabledSignalFromPlayerToDecoder {
    pub decoder_to_ve_signal_sender: UnboundedSender<DecorderToVeSyncSignal>,
    pub decorder_to_ve_signal_recv: UnboundedReceiver<DecorderToVeSyncSignal>,
    pub ve_to_decoder_signal_sender: UnboundedSender<VeToDecoderSyncSignal>,
    pub ve_to_decoder_signal_recv: UnboundedReceiver<VeToDecoderSyncSignal>,
    pub ve_shared_buffer: AtomicPtr<Pin<Box<VESharedBuffer>>>,
}

pub enum VeControlSignal {
    VeEnabled(VeControlSignalVeEnabled),
    VeDisabled,
    Stop,
    Seek(SeekSignalForVe),
}
pub struct VeControlSignalVeEnabled {
    pub ve_enabled_info: VeEnabledInfoFromDecoder,
    pub ve_to_ui_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    pub ui_to_ve_signal_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
}

struct DecoderInitSignal {
    renderer_sample_rate: SampleRate,
}

pub type SharedBuffer = [[Vec<f32>; CHANNEL]; NUM_OF_BLOCK];

#[derive(Debug)]
pub struct OscilloscopeData(pub Vec<(f64, f64)>);
pub type VESharedBuffer = [[OscilloscopeData; CHANNEL]; NUM_OF_BLOCK_VE];

//pub const BLOCK_SIZE: usize = 1024 * 16; //147 * 160 * 4;
pub const CHANNEL: usize = 2;
pub const NUM_OF_BLOCK: usize = 16;
pub const BACK_ROOM: usize = 4;
pub const NUM_OF_BLOCK_VE: usize = 180;
pub const AUDIO_OUTPUT_BUFFER_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct PlayerExecutorError {
    pub error_message: String,
}
impl PlayerExecutorError {
    fn new(error_message: &str) -> Self {
        Self {
            error_message: error_message.to_string(),
        }
    }
}

impl Display for PlayerExecutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error_message)
    }
}

impl std::error::Error for PlayerExecutorError {}

pub async fn play_executor(
    playback_file_path: &str,
    init_vol: u16,
    player_control_signal_recv: &mut UnboundedReceiver<PlayerControlSignal>,
    player_to_ui_singnal_sender: UnboundedSender<PlayerToUISingnal>,
) -> Result<(), PlayerExecutorError> {
    let mut decoder_wrapper = match DecoderWrapper::new(playback_file_path) {
        Ok(d) => d,
        Err(err) => {
            return Err(PlayerExecutorError::new(&err.to_string()));
        }
    };

    let file_sample_rate = {
        let Some(file_sample_rate) = decoder_wrapper.get_sample_rate() else {
            return Err(PlayerExecutorError::new("sample_rate get failed"));
        };
        file_sample_rate as usize
    };

    let Some(track_duration) = decoder_wrapper.get_duration() else {
        return Err(PlayerExecutorError::new("get duration failed"));
    };

    let (renderer_to_decoder_sender, renderer_to_docoder_reciever) = mpsc::unbounded_channel();
    let (decoder_to_renderer_sender, decoder_to_renderer_reciever) = mpsc::unbounded_channel();

    let mut shared_buffer = Box::pin(array_init(|| array_init(|| vec![0f32; 0])));

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

    let worker_to_player_notification_signal_sender_for_ve =
        worker_to_player_notofication_signal_sender.clone();
    let player_to_ui_singnal_for_decoder = player_to_ui_singnal_sender.clone();
    let decoder_handle = tokio::task::spawn_blocking(move || {
        let shared_buffer = unsafe { retrieve_ref(&shared_buffer_for_decoder) };

        let Some(DecoderInitSignal {
            renderer_sample_rate,
        }) = decoder_init_signal_recv.blocking_recv()
        else {
            return Ok(()); // abnormal case, but assuming other thread returns error,this thrad returns Ok.
        };

        let worker_to_player_notofication_signal_sender_clone =
            worker_to_player_notofication_signal_sender.clone();

        if let Err(err) = decode_loop(
            &mut decoder_wrapper,
            shared_buffer,
            renderer_sample_rate,
            decoder_to_renderer_sender,
            renderer_to_docoder_reciever,
            decoder_control_signal_recv,
            worker_to_player_notofication_signal_sender,
            player_to_ui_singnal_for_decoder,
        ) {
            _ = worker_to_player_notofication_signal_sender_clone
                .send(WorkerToPlayerNotification::Error);
            //info!("decode_loop error: {err}");
            return Err(PlayerExecutorError {
                error_message: err.to_string(),
            });
        }

        Ok(())
    });

    for _ in 0..NUM_OF_BLOCK - 2 - BACK_ROOM {
        _ = renderer_to_decoder_sender.send(RendererToDecoderSsynSignal());
    }

    let (ve_control_signal_sender, ve_control_signal_recv) = unbounded_channel();

    let player_to_ui_singnal_sender_for_ve = player_to_ui_singnal_sender.clone();
    let visual_effect_handle = tokio::task::spawn(async move {
        let shared_buffer = unsafe { retrieve_ref(&shared_buffer_for_ve) };

        let mut ve_control_signal_recv = ve_control_signal_recv;

        let (sender, mut recv) = unbounded_channel::<VeEnabledInfoFromPlayerToVe>();

        let handle = tokio::task::spawn_blocking(move || {
            'l1: loop {
                match recv.blocking_recv() {
                    Some(VeEnabledInfoFromPlayerToVe {
                        sample_rate,
                        renderer_read_exclusize,
                        decorder_to_ve_signal_recv,
                        ve_to_decoder_signal_sender,
                        ve_to_ui_signal_sender,
                        ui_to_ve_signal_recv,
                        ve_shared_buffer,
                        ve_control_signal_inner_recv,
                    }) => {
                        let ve_shared_buffer = unsafe { retrieve_ref(&ve_shared_buffer) };

                        _ = player_to_ui_singnal_sender_for_ve
                            .send(PlayerToUISingnal::VeEnabledSync);

                        ve_loop(
                            shared_buffer,
                            ve_shared_buffer,
                            ve_to_decoder_signal_sender,
                            decorder_to_ve_signal_recv,
                            ve_to_ui_signal_sender,
                            ui_to_ve_signal_recv,
                            ve_control_signal_inner_recv,
                            &worker_to_player_notification_signal_sender_for_ve,
                            sample_rate,
                            renderer_read_exclusize,
                        );
                        _ = worker_to_player_notification_signal_sender_for_ve
                            .send(WorkerToPlayerNotification::VeDisabledSync);
                    }
                    None => break 'l1,
                }
            }
        });

        let event_block = async move {
            let mut ve_control_signal_inner_sender: Option<UnboundedSender<VeControlSignalInner>> =
                None;
            'l1: loop {
                match ve_control_signal_recv.recv().await {
                    Some(VeControlSignal::VeEnabled(VeControlSignalVeEnabled {
                        ve_enabled_info,
                        ve_to_ui_signal_sender,
                        ui_to_ve_signal_recv,
                    })) => {
                        let (ve_control_signal_inner_sender_temp, ve_control_signal_inner_recv) =
                            unbounded_channel();
                        ve_control_signal_inner_sender = Some(ve_control_signal_inner_sender_temp);

                        _ = sender.send(VeEnabledInfoFromPlayerToVe {
                            sample_rate: ve_enabled_info.audio_device_sample_rate,
                            renderer_read_exclusize: ve_enabled_info.ve_start_read_exclusize,
                            decorder_to_ve_signal_recv: ve_enabled_info.decorder_to_ve_signal_recv,
                            ve_to_decoder_signal_sender: ve_enabled_info
                                .ve_to_decoder_signal_sender,
                            ve_to_ui_signal_sender,
                            ui_to_ve_signal_recv,
                            ve_shared_buffer: ve_enabled_info.ve_shared_buffer,
                            ve_control_signal_inner_recv,
                        });
                    }
                    Some(VeControlSignal::Stop) => {
                        if let Some(sender) = &ve_control_signal_inner_sender {
                            _ = sender.send(VeControlSignalInner::Stop);
                        }
                        break 'l1;
                    }
                    Some(VeControlSignal::VeDisabled) => {
                        if let Some(sender) = &ve_control_signal_inner_sender {
                            _ = sender.send(VeControlSignalInner::Stop);
                        }
                    }
                    Some(VeControlSignal::Seek(signal)) => {
                        if let Some(sender) = &ve_control_signal_inner_sender {
                            _ = sender.send(VeControlSignalInner::Seek(signal));
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

        let worker_to_player_notofication_signal_sender_for_renderer_clone =
            worker_to_player_notofication_signal_sender_for_renderer.clone();

        let res = audio_output::main(
            shared_buffer,
            renderer_to_decoder_sender,
            decoder_to_renderer_reciever,
            renderer_control_signal_recv,
            player_to_ui_singnal,
            worker_to_player_notofication_signal_sender_for_renderer,
            init_vol,
        );

        if let Err(err) = res {
            _ = worker_to_player_notofication_signal_sender_for_renderer_clone
                .send(WorkerToPlayerNotification::Error);
            return Err(PlayerExecutorError {
                error_message: err.message(),
            });
        }
        Ok(())
        //let res = res.ok();
    });

    let player_control_signal_loop_task_cancellation_token = CancellationToken::new();
    let player_control_signal_loop_task_cancellation_token_for_worker_notification =
        player_control_signal_loop_task_cancellation_token.clone();

    let player_control_signal_loop_task = async {
        enum VeEnabledRquestRecivedState {
            VeEnabled,
            VeDisabled,
        }
        let mut ve_enabled_request_recvied_state = VeEnabledRquestRecivedState::VeDisabled;
        let cancellation_token =
            player_control_signal_loop_task_cancellation_token_for_worker_notification;

        'l1: loop {
            let message = tokio::select! {
                _ = cancellation_token.cancelled() => break 'l1,
                signal = player_control_signal_recv.recv() => match signal {
                    Some(msg) => msg,
                    None => break 'l1,
                }
            };

            match message {
                PlayerControlSignal::SetVol(vol) => {
                    _ = renderer_control_signal_sender.send(RendererControlSignal::SetVol(vol));
                }
                PlayerControlSignal::Pause => {
                    _ = renderer_control_signal_sender.send(RendererControlSignal::Pause);
                }
                PlayerControlSignal::Resume => {
                    _ = renderer_control_signal_sender.send(RendererControlSignal::Resume);
                }
                PlayerControlSignal::Stop => {
                    _ = renderer_control_signal_sender.send(RendererControlSignal::Stop);
                    _ = decoder_control_signal_sender.send(DecoderControlSignal::Stop);
                    _ = ve_control_signal_sender.send(VeControlSignal::Stop);
                    break 'l1;
                }
                PlayerControlSignal::VeEnabled(ve_shared_buffer) => {
                    ve_enabled_request_recvied_state = VeEnabledRquestRecivedState::VeEnabled;
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

                    _ = decoder_control_signal_sender
                        .send(DecoderControlSignal::VeEnabled(ve_enabled_signal));
                }
                PlayerControlSignal::VeDisabled => {
                    ve_enabled_request_recvied_state = VeEnabledRquestRecivedState::VeDisabled;
                    _ = decoder_control_signal_sender.send(DecoderControlSignal::VeDisabled);
                    _ = ve_control_signal_sender.send(VeControlSignal::VeDisabled);
                }
                PlayerControlSignal::Seek(signal) => {
                    struct SeekSignalForVeChannels {
                        pub decoder_to_ve_sync_signal_recv:
                            UnboundedReceiver<DecorderToVeSyncSignal>,
                        pub ve_to_decoder_sync_signal_sender:
                            UnboundedSender<VeToDecoderSyncSignal>,
                    }
                    let target_duration = signal.target_duration;
                    let sync_obj = SeekTimingSyncState {
                        init_sync: SeekTimingSyncObj::new(),
                        complete_sync: SeekTimingSyncObj::new(),
                    };
                    let sync_obj = Arc::new(sync_obj);

                    let (
                        renderer_to_decoder_sync_signal_sender,
                        renderer_to_decoder_sync_signal_recv,
                    ) = unbounded_channel();
                    let (
                        decoder_to_renderer_sync_signal_sender,
                        decoder_to_renderer_sync_signal_recv,
                    ) = unbounded_channel();

                    let (seek_signal_for_decoder_ve_channels, seek_singal_for_ve_channels) =
                        if let VeEnabledRquestRecivedState::VeEnabled =
                            ve_enabled_request_recvied_state
                        {
                            let (decoder_to_ve_sync_signal_sender, decoder_to_ve_sync_signal_recv) =
                                unbounded_channel();
                            let (ve_to_decoder_sync_signal_sender, ve_to_decoder_sync_signal_recv) =
                                unbounded_channel();

                            for _ in 0..NUM_OF_BLOCK - 2 {
                                _ = ve_to_decoder_sync_signal_sender.send(VeToDecoderSyncSignal());
                            }

                            let seek_signal_for_decoder_ve_channels =
                                SeekSignalForDecoderVeChannels {
                                    decoder_to_ve_sync_signal_sender,
                                    ve_to_decoder_sync_signal_recv,
                                };
                            let seek_singal_for_ve_channels = SeekSignalForVeChannels {
                                decoder_to_ve_sync_signal_recv,
                                ve_to_decoder_sync_signal_sender,
                            };

                            (
                                Some(seek_signal_for_decoder_ve_channels),
                                Some(seek_singal_for_ve_channels),
                            )
                        } else {
                            (None, None)
                        };

                    for _ in 0..NUM_OF_BLOCK - 2 - BACK_ROOM {
                        _ = renderer_to_decoder_sync_signal_sender
                            .send(RendererToDecoderSsynSignal());
                    }

                    let seek_signal_for_renderer = SeekSignalForRenderer {
                        sync_obj: sync_obj.clone(),
                        renderer_to_decoder_sync_signal_sender,
                        decoder_to_renderer_sync_signal_recv,
                        seek_no: signal.seek_no,
                    };

                    let seek_signal_for_decoder = SeekSignalForDecoder {
                        sync_obj: sync_obj.clone(),
                        target_duration,
                        decoder_to_renderer_sync_signal_sender,
                        renderer_to_decoder_sync_signal_recv,
                        seek_signal_for_decoder_ve_channels,
                        seek_no: signal.seek_no,
                    };

                    match seek_singal_for_ve_channels {
                        Some(SeekSignalForVeChannels {
                            decoder_to_ve_sync_signal_recv,
                            ve_to_decoder_sync_signal_sender,
                        }) => {
                            let seek_signal_for_ve = SeekSignalForVe {
                                sync_obj,
                                ve_to_decoder_sync_signal_sender,
                                decoder_to_ve_sync_signal_recv,
                            };

                            _ = ve_control_signal_sender
                                .send(VeControlSignal::Seek(seek_signal_for_ve));
                        }
                        None => {}
                    };

                    _ = decoder_control_signal_sender
                        .send(DecoderControlSignal::Seek(seek_signal_for_decoder));
                    _ = renderer_control_signal_sender
                        .send(RendererControlSignal::Seek(seek_signal_for_renderer));
                }
            };
        }
        //info!("exit_player_control_signal_loop_task");
    };

    let renderer_control_signal_sender = &renderer_control_signal_sender;
    let decoder_control_signal_sender = &decoder_control_signal_sender;
    let ve_control_signal_sender = &ve_control_signal_sender;

    let player_notification_loop_task = async move {
        let mut disabled_count = 0;
        let mut seek_ve_completed_signal: Option<SeekCompleteFromVeSignal> = None;
        let mut seek_decoder_completed_signal: Option<SeekCompleteFromDecoderSignal> = None;

        let handle_error_variant = || {
            _ = renderer_control_signal_sender.send(RendererControlSignal::Stop);
            _ = decoder_control_signal_sender.send(DecoderControlSignal::Stop);
            _ = ve_control_signal_sender.send(VeControlSignal::Stop);

            //info!("WorkerToPlayerNotification::Error");
            player_control_signal_loop_task_cancellation_token.cancel();
        };

        'l2: loop {
            match worker_to_player_notofication_signal_recv.recv().await {
                Some(WorkerToPlayerNotification::VeEnabledInfo(ve_enabled_info)) => {
                    let (ve_to_ui_signal_sender, ve_to_ui_signal_recv) = unbounded_channel();
                    let (ui_to_ve_signal_sender, ui_to_ve_signal_recv) = unbounded_channel();

                    for _ in 0..NUM_OF_BLOCK_VE - 2 {
                        _ = ui_to_ve_signal_sender.send(UiVEThreadSyncSignal());
                    }

                    let ve_buffer_duration_offset_sec =
                        ve_enabled_info.ve_buffer_duration_offset_sec;
                    let send_signal = VeControlSignalVeEnabled {
                        ve_enabled_info,
                        ve_to_ui_signal_sender,
                        ui_to_ve_signal_recv,
                    };
                    _ = ve_control_signal_sender.send(VeControlSignal::VeEnabled(send_signal));
                    _ = player_to_ui_singnal_sender.send(PlayerToUISingnal::VeEnabled(
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
                        match SampleRate::try_from(file_sample_rate) {
                            Ok(value) => value,
                            Err(rate) => {
                                handle_error_variant();
                                return Err(PlayerExecutorError::new(
                                    &format!("unsupported file sample rate {rate}")
                                        .into_boxed_str(),
                                ));
                            }
                        },
                        match SampleRate::try_from(audio_device_sample_rate) {
                            Ok(value) => value,
                            Err(rate) => {
                                handle_error_variant();
                                return Err(PlayerExecutorError::new(
                                    &format!("unsupported audio_device sample rate {rate}")
                                        .into_boxed_str(),
                                ));
                            }
                        },
                    );

                    _ = decoder_init_signal_sender.send(DecoderInitSignal {
                        renderer_sample_rate: audio_device_sample_rate,
                    });

                    _ = player_to_ui_singnal_sender.send(PlayerToUISingnal::NoticeTrackInfo(
                        TrackInfo {
                            file_sample_rate: file_sample_rate.raw_value(),
                            audio_device_sample_rate: audio_device_sample_rate.raw_value(),
                            track_duration,
                        },
                    ));
                }
                Some(WorkerToPlayerNotification::SeekCompleteFromVe(signal)) => 'b1: {
                    if let Some(decoder_signal) = seek_decoder_completed_signal.take() {
                        notify_to_ui_seek_complete(
                            Some(signal),
                            decoder_signal,
                            &player_to_ui_singnal_sender,
                        );
                        break 'b1;
                    }
                    seek_ve_completed_signal = Some(signal);
                }
                Some(WorkerToPlayerNotification::SeekCompleteFromDecoder(signal)) => 'b1: {
                    if !signal.ve_enabled {
                        notify_to_ui_seek_complete(None, signal, &player_to_ui_singnal_sender);
                        break 'b1;
                    }
                    if let Some(ve_signal) = seek_ve_completed_signal.take() {
                        notify_to_ui_seek_complete(
                            Some(ve_signal),
                            signal,
                            &player_to_ui_singnal_sender,
                        );
                        break 'b1;
                    }
                    seek_decoder_completed_signal = Some(signal);
                }
                Some(WorkerToPlayerNotification::Error) => {
                    handle_error_variant();

                    break 'l2;
                }
                None => break 'l2,
            };

            fn notify_to_ui_seek_complete(
                seek_ve_completed_signal: Option<SeekCompleteFromVeSignal>,
                seek_decoder_completed_signal: SeekCompleteFromDecoderSignal,
                player_to_ui_singnal_sender: &UnboundedSender<PlayerToUISingnal>,
            ) {
                let signal = SeekCompleteSignal {
                    seek_ve_completed_signal,
                    actual_seek_duration_sec: seek_decoder_completed_signal
                        .actual_seek_duration_sec,
                    seek_no: seek_decoder_completed_signal.seek_no,
                };
                _ = player_to_ui_singnal_sender.send(PlayerToUISingnal::SeekComplete(signal));
            }
        }

        //info!("player_notification_loop_task");

        Ok(())
    };

    let res = tokio::join!(
        player_control_signal_loop_task,
        player_notification_loop_task,
        renderer_handle,
        decoder_handle,
        visual_effect_handle
    );

    let get_thread_error = |_: JoinError| {
        Err(PlayerExecutorError {
            error_message: "thread_error".to_string(),
        })
    };
    let thread_ressults: [Result<(), PlayerExecutorError>; _] = [
        res.1,
        res.2.map_or_else(get_thread_error, |x| x),
        res.3.map_or_else(get_thread_error, |x| x),
    ];

    match thread_ressults.iter().find(|x| x.is_err()) {
        Some(result) => {
            //info!("EXIT_FINAL_1");
            result.clone()
        }
        None => {
            //info!("EXIT_FINAL_2");
            Ok(())
        }
    }
}
