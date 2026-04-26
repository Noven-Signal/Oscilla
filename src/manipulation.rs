use std::{
    fmt::Debug,
    panic,
    sync::{Arc, atomic::{AtomicPtr, Ordering}},
    time::Duration,
};

use symphonia::{
    core::{audio::Signal, io::MediaSourceStream, probe::Hint},
    default::get_probe,
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;
use urlencoding::decode;

use crate::{
    AppState::AppState::AppStateContainer,
    AudioOutput,
    DecoderWrapper::{DecodeResult, DecoderWrapper},
};

pub fn play(app_state_container: &AppStateContainer) {
    let Some(selected) = app_state_container.play_list_selected.selected() else {
        return;
    };
    let playback_file_path_ptr = app_state_container.play_list[selected].as_str();
    let playback_file_path = String::from(playback_file_path_ptr);

    tokio::spawn(async move {
        play_executor(&playback_file_path).await;
    });
}

pub struct SendSignal();

pub type ChannelData = [Vec<f32>; 8];

pub struct SharedBuffer {
    pub channel_left_data: ChannelData,
    pub channel_right_data: ChannelData,
    pub renderer_to_decoder_sender: UnboundedSender<SendSignal>,
    pub renderer_to_docoder_reciever: UnboundedReceiver<SendSignal>,
    pub decoder_to_renderer_sender: UnboundedSender<SendSignal>,
    pub decoder_to_renderer_reciever: UnboundedReceiver<SendSignal>,
}
unsafe impl Send for SharedBuffer  {
   
}


pub const BLOCK_SIZE: usize = 100 * 1024;

pub async fn play_executor(playback_file_path: &str) {
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
        shared_buffer.renderer_to_decoder_sender.send(SendSignal());
    }

    let shared_buffer_for_decoder = AtomicPtr::new(&raw mut shared_buffer);
    let shared_buffer_for_renderer = AtomicPtr::new(&raw mut shared_buffer);

    // let shared_buffer = unsafe { shared_buffer_for_decoder.into_inner().as_mut().unwrap() };
    //let p = ss.as_mut().unwrap();

    let retrieve_ref = |atomic_ptr: AtomicPtr<SharedBuffer>| unsafe {
        atomic_ptr.load(Ordering::Acquire).as_mut().unwrap()
    };

    let decoder_handle = tokio::spawn(async move {
        let shared_buffer = retrieve_ref(shared_buffer_for_decoder);
        append_decode_buffer(&mut decoder_wrapper, shared_buffer).await;
    });

    let renderer_handle = tokio::spawn(async move {
        let shared_buffer = retrieve_ref(shared_buffer_for_renderer);
        let cancellation_token = CancellationToken::new();
        AudioOutput::main(shared_buffer, cancellation_token).await.unwrap();
        //let res = res.ok();
    });

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
        sender.send(SendSignal());
    };

    let mut write_exclusive: usize = 1;

    let mut head: usize = 0;
    'l1: loop {
        let decoded = decoder_wrapper.decode();

        use symphonia::core::audio::AudioBufferRef::*;
        match decoded {
            DecodeResult::Buf(audio_buffer_ref) => match audio_buffer_ref {
                F32(cow) => {
                    let view_0 = cow.chan(0);

                    let target_len = head + view_0.len();

                    use std::cmp::Ordering::*;
                    match Ord::cmp(&target_len, &BLOCK_SIZE) {
                        Less => {
                            let exclusive_buf =
                                &mut shared_buffer.channel_left_data[write_exclusive as usize];

                            let target_slice = &mut exclusive_buf[head..head + view_0.len()];

                            target_slice.copy_from_slice(view_0);

                            head = head + view_0.len();
                        }
                        Equal => {
                            let exclusive_buf =
                                &mut shared_buffer.channel_left_data[write_exclusive as usize];
                            let target_slice = &mut exclusive_buf[head..];

                            target_slice.copy_from_slice(view_0);

                            singnal_to_thread().await;
                            write_exclusive = next_block(write_exclusive);
                            head = 0;
                        }
                        Greater => {
                            let exclusive_buf_left =
                                &mut shared_buffer.channel_left_data[write_exclusive as usize];
                            let target_slice_spill_over = &mut exclusive_buf_left[head..];

                            let block_len = BLOCK_SIZE;
                            let this_iter_fill = block_len - head;

                            let (current_view0, spill_over_view0) = view_0.split_at(this_iter_fill);
                            target_slice_spill_over.copy_from_slice(current_view0);

                            singnal_to_thread().await;
                            write_exclusive = next_block(write_exclusive);

                            let exclusive_buf_spill_over =
                                &mut shared_buffer.channel_left_data[write_exclusive as usize];

                            let target_slice_spill_over =
                                &mut exclusive_buf_spill_over[0..spill_over_view0.len()];

                            target_slice_spill_over.copy_from_slice(spill_over_view0);
                            head = spill_over_view0.len();
                        }
                    }
                }
                _ => {}
            },
            DecodeResult::Err(error) => {
                return;
                //println!("error: {error}")
            },
            DecodeResult::EndOfStream => break 'l1,
            DecodeResult::None => continue,
        }
    }

    println!("end of func");
}
