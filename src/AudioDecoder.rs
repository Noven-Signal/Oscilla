use std::ops::AddAssign;

use symphonia::core::audio::Signal;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::info;

use crate::{
    DecoderWrapper::{DecodeResult, DecoderWrapper},
    manipulation::{
        BLOCK_SIZE, CHANNEL, DecoderControlSignal, DecoderToRendererSyncSignal,
        DecorderToVeSyncSignal, EndOfStreamSignal, NUM_OF_BLOCK, RendererToDecoderSsynSignal,
        SharedBuffer, VeEnabledInfoFromDecoder, VeEnabledSignalFromPlayerToDecoder,
        VeToDecoderSyncSignal, WorkerToPlayerNotification,
    },
    utils::array_init,
};

pub fn decode_loop(
    decoder_wrapper: &mut DecoderWrapper,
    shared_buffer: &mut SharedBuffer,
    decoder_to_renderer_sender: UnboundedSender<DecoderToRendererSyncSignal>,
    mut renderer_to_decoder_singal_recv: UnboundedReceiver<RendererToDecoderSsynSignal>,
    // decoder_to_ve_signal_sender: UnboundedSender<DecorderToVeSyncSignal>,
    // mut ve_to_decoder_signal_recv: UnboundedReceiver<VeToDecoderSyncSignal>,
    mut decoder_control_signal: UnboundedReceiver<DecoderControlSignal>,
    decoder_to_player_notification_signal: UnboundedSender<WorkerToPlayerNotification>,
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
