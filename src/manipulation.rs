use std::{
    fmt::Debug,
    panic,
    sync::{
        Arc,
        atomic::{AtomicPtr, Ordering},
    }, time::Duration
};

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
};

// pub fn play(app_state_container: &AppStateContainer) {
//     let Some(selected) = app_state_container.play_list_selected.selected() else {
//         return;
//     };
//     let playback_file_path_ptr = app_state_container.play_list[selected].as_str();
//     let playback_file_path = String::from(playback_file_path_ptr);

//     tokio::spawn(async move {
//         play_executor(&playback_file_path).await;
//     });
// }

pub struct DecoderRendererSyncSignal();

#[derive(Debug)]
pub enum RendererControlSignal {
    SetVol(u16),
    Pause,
    Resume,
}
#[derive(Debug)]
pub enum PlayerControlSignal {
    SetVol(u16),
    Pause,
    Resume,
}
pub type ChannelData = [Vec<f32>; 8];

pub struct SharedBuffer {
    pub channel_left_data: ChannelData,
    pub channel_right_data: ChannelData,
    pub renderer_to_decoder_sender: UnboundedSender<DecoderRendererSyncSignal>,
    pub renderer_to_docoder_reciever: UnboundedReceiver<DecoderRendererSyncSignal>,
    pub decoder_to_renderer_sender: UnboundedSender<DecoderRendererSyncSignal>,
    pub decoder_to_renderer_reciever: UnboundedReceiver<DecoderRendererSyncSignal>,
}
unsafe impl Send for SharedBuffer {}

pub const BLOCK_SIZE: usize = 40 * 1024;
pub const CHANNEL: usize = 2;

pub async fn play_executor(
    playback_file_path: &str,
    player_control_signal_recv: &mut UnboundedReceiver<PlayerControlSignal>,
) {
    let probe = get_probe();
    use std::fs::File;
    let Ok(file) = File::open(playback_file_path) else {
        return;
    };
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("mp3");
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

    let (renderer_to_decoder_sender, renderer_to_docoder_reciever) = mpsc::unbounded_channel();
    let (decoder_to_renderer_sender, decoder_to_renderer_reciever) = mpsc::unbounded_channel();

    fn array_init<T: Sized + Debug, const N: usize>(f: impl Fn() -> T) -> [T; N] {
        (0..N).map(|_| f()).collect::<Vec<T>>().try_into().unwrap()
    }

    let create_vec = || vec![0f32; BLOCK_SIZE];
    let mut shared_buffer = SharedBuffer {
        channel_left_data: array_init(create_vec),
        channel_right_data: array_init(create_vec),
        renderer_to_decoder_sender,
        renderer_to_docoder_reciever,
        decoder_to_renderer_sender,
        decoder_to_renderer_reciever,
    };

    for _ in 0..6 {
        shared_buffer
            .renderer_to_decoder_sender
            .send(DecoderRendererSyncSignal());
    }

    let shared_buffer_for_decoder = AtomicPtr::new(&raw mut shared_buffer);
    let shared_buffer_for_renderer = AtomicPtr::new(&raw mut shared_buffer);

    let retrieve_ref = |atomic_ptr: AtomicPtr<SharedBuffer>| unsafe {
        atomic_ptr.load(Ordering::Acquire).as_mut().unwrap()
    };

    let decoder_handle = tokio::spawn(async move {
        let shared_buffer = retrieve_ref(shared_buffer_for_decoder);
        append_decode_buffer(&mut decoder_wrapper, shared_buffer).await;
    });

    let (renderer_control_signal_sender, renderer_control_signal_recv) =
        unbounded_channel::<RendererControlSignal>();

    let renderer_handle = tokio::spawn(async move {
        let shared_buffer = retrieve_ref(shared_buffer_for_renderer);

        AudioOutput::main(shared_buffer, renderer_control_signal_recv).await.unwrap();
        //let res = res.ok();
    });

    'l1: loop {
        let map_signal = match player_control_signal_recv.recv().await {
            Some(PlayerControlSignal::SetVol(vol)) => RendererControlSignal::SetVol(vol),
            Some(PlayerControlSignal::Pause) => RendererControlSignal::Pause,
            Some(PlayerControlSignal::Resume) => RendererControlSignal::Resume,
            
            None => break 'l1,
        };
        renderer_control_signal_sender.send(map_signal);
    }

    decoder_handle.await;
    renderer_handle.await;

    tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
}

async fn append_decode_buffer(
    decoder_wrapper: &mut DecoderWrapper,
    shared_buffer: &mut SharedBuffer,
) {
    let next_block = |write_end| match write_end {
        7 => 0,
        write_end => write_end + 1,
    };

    let mut singnal_to_thread = async || {
        let reciever = &mut shared_buffer.renderer_to_docoder_reciever;
        reciever.recv().await;

        let sender = &mut shared_buffer.decoder_to_renderer_sender;
        sender.send(DecoderRendererSyncSignal());
    };

    let mut write_exclusive: usize = 1;

    let mut head: usize = 0;
    'l1: loop {
        let decoded = decoder_wrapper.decode();

        use symphonia::core::audio::AudioBufferRef::*;
        match decoded {
            DecodeResult::Buf(audio_buffer_ref) => match audio_buffer_ref {
                F32(cow) => {
                    macro_rules! exclusize_buf_ref_mut {
                        ($channel: tt,$write_exclusive:expr) => {
                            &mut shared_buffer.$channel[write_exclusive]
                        };
                    }

                    let view_0 = cow.chan(0);
                    let view_1 = cow.chan(1);

                    let mut fill_buff_within_block = || {
                        let copy_buff = |exclusive_buf: &mut [f32], view: &[f32]| {
                            let target_slice = &mut exclusive_buf[head..head + view.len()];
                            target_slice.copy_from_slice(view);
                        };

                        let exclusive_buf_left =
                            exclusize_buf_ref_mut!(channel_left_data, write_exclusive);
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
                            singnal_to_thread().await;

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

                                let (current_view, spill_over_view) =
                                    view.split_at(BLOCK_SIZE - head);
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

                            singnal_to_thread().await;
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
                }
                _ => {}
            },
            DecodeResult::Err(error) => {
                return;
                //println!("error: {error}")
            }
            DecodeResult::EndOfStream => break 'l1,
            DecodeResult::None => continue,
        }
    }

    println!("end of func");
}
