use std::{
    cell::UnsafeCell,
    fmt::Debug,
    sync::{
        Arc,
        atomic::{AtomicPtr, AtomicU8},
    },
    usize,
};

use futures::{future::Shared, lock::Mutex};
use symphonia::{
    core::{audio::Signal, io::MediaSourceStream, probe::Hint},
    default::{get_codecs, get_probe},
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;

use crate::{
    AppState::{self, AppState::AppStateContainer},
    AudioOutput,
    DecoderWrapper::{DecodeResult, DecoderWrapper},
};

pub fn play(app_satte_container: &AppStateContainer) {
    let Some(selected) = app_satte_container.play_list_selected.selected() else {
        return;
    };
    let playback_file_path_ptr = app_satte_container.play_list[selected].as_str();
    let playback_file_path = String::from(playback_file_path_ptr);

    tokio::spawn(async move {
        play_executor(&playback_file_path).await;
    });
}

#[derive(Clone)]
pub struct BlockRange {
    pub start: u8,
    pub end: u8,
}
pub struct ReleasedBlock(u8);

#[derive(Clone)]
pub struct RingBufferInfo {
    pub write_buf: BlockRange,
    pub read_exclusive: usize,
}
impl RingBufferInfo {
    pub const fn get_write_exlusive(&self) -> usize {
        self.write_buf.end as usize
    }
}
pub type ChannelData = [Vec<f32>; 8];
pub struct SharedBuffer {
    pub channel_left_data: ChannelData,
    pub channel_right_data: ChannelData,
    pub ring_buffer_info: Mutex<RingBufferInfo>,
    pub renderer_to_decoder_sender: UnboundedSender<ReleasedBlock>,
    pub renderer_to_docoder_reciever: UnboundedReceiver<ReleasedBlock>,
    pub decoder_to_renderer_sender: UnboundedSender<ReleasedBlock>,
    pub decoder_to_renderer_reciever: UnboundedReceiver<ReleasedBlock>,
}

const COUNT_1M: usize = 1 * 1024 * 1024;
const COUNT_CHUNK: usize = COUNT_1M;

async fn play_executor(playback_file_path: &str) {
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

    fn array_init<T: Sized + Debug, const N: usize>(f: impl Fn() -> T, count: usize) -> [T; N] {
        (0..count - 1)
            .map(|_| f())
            .collect::<Vec<T>>()
            .try_into()
            .unwrap()
    }
    let create_vec = || vec![0f32; COUNT_1M];
    let mut shared_buffer = SharedBuffer {
        channel_left_data: array_init(create_vec, 8),
        channel_right_data: array_init(create_vec, 8),
        ring_buffer_info: Mutex::new(RingBufferInfo {
            write_buf: BlockRange { start: 1, end: 1 },
            read_exclusive: 0,
        }),
        renderer_to_decoder_sender,
        renderer_to_docoder_reciever,
        decoder_to_renderer_sender,
        decoder_to_renderer_reciever,
    };

    let shared_buffer_for_decoder = AtomicPtr::new(&raw mut shared_buffer);
    let shared_buffer_for_renderer = AtomicPtr::new(&raw mut shared_buffer);
    tokio::spawn(async move {
        let shared_buffer = unsafe { shared_buffer_for_decoder.into_inner().as_mut().unwrap() };
        //let p = ss.as_mut().unwrap();
        append_decode_buffer(&mut decoder_wrapper, shared_buffer).await;
    });

    tokio::spawn(async move {
        let shared_buffer = unsafe { shared_buffer_for_renderer.into_inner().as_mut().unwrap() };
        let cancellation_token = CancellationToken::new();
        let res = AudioOutput::main(shared_buffer, cancellation_token);
    });
}

async fn append_decode_buffer(
    decoder_wrapper: &mut DecoderWrapper,
    shared_buffer: &mut SharedBuffer,
) {
    let next_block = |write_end| match write_end {
        7 => 0,
        write_end => write_end + 1,
    };

    let mut move_to_next_block = async || {
        let mut mutex = shared_buffer.ring_buffer_info.lock().await;

        if next_block(mutex.read_exclusive) == mutex.write_buf.end as usize {
            let reciever = &mut shared_buffer.renderer_to_docoder_reciever;
            reciever.recv().await;
        }
        let next = next_block(mutex.read_exclusive);
        mutex.read_exclusive = next;
        next
    };

    let next = move_to_next_block().await;
    let mut write_end = next;

    let mut head: usize = 0;
    'l1: loop {
        let decoded = decoder_wrapper.decode();
        use symphonia::core::audio::AudioBufferRef::*;
        match decoded {
            DecodeResult::Buf(audio_buffer_ref) => match audio_buffer_ref {
                F32(cow) => {
                    // let RingBufferInfo {
                    //     read_exclusive,
                    //     write_buf:
                    //         BlockRange {
                    //             end: ref mut write_end,
                    //             ..
                    //         },
                    //     ..
                    // } = *shared_buffer.ring_buffer_info.lock().await;

                    let view_0 = cow.chan(0);

                    let target_len = head + view_0.len();

                    use std::cmp::Ordering::*;
                    match Ord::cmp(&target_len, &COUNT_1M) {
                        Less => {
                            let exclusive_buf =
                                &mut shared_buffer.channel_left_data[write_end as usize];
                            let target_slice = &mut exclusive_buf[head..head + view_0.len()];

                            target_slice.copy_from_slice(view_0);

                            head = head + view_0.len();
                        }
                        Equal => {
                            let exclusive_buf =
                                &mut shared_buffer.channel_left_data[write_end as usize];
                            let target_slice = &mut exclusive_buf[head..];

                            target_slice.copy_from_slice(view_0);

                            let next = move_to_next_block().await;
                            write_end = next;
                            head = 0;
                        }
                        Greater => {
                            let exclusive_buf_left =
                                &mut shared_buffer.channel_left_data[write_end as usize];
                            let target_slice_spill_over = &mut exclusive_buf_left[head..];

                            let block_len = COUNT_1M;
                            let this_iter_fill = block_len - head;

                            let (current_view0, spill_over_view0) = view_0.split_at(this_iter_fill);
                            target_slice_spill_over.copy_from_slice(current_view0);

                            let next = move_to_next_block().await;
                            write_end = next;
                            let exclusive_buf_spill_over =
                                &mut shared_buffer.channel_left_data[write_end as usize];

                            let target_slice_spill_over =
                                &mut exclusive_buf_spill_over[0..spill_over_view0.len()];

                            target_slice_spill_over.copy_from_slice(spill_over_view0);

                            head = spill_over_view0.len();
                        }
                    }
                }
                _ => {}
            },
            DecodeResult::Err(error) => todo!(),
            DecodeResult::EndOfStream => break 'l1,
            DecodeResult::None => continue,
        }
    }
}
