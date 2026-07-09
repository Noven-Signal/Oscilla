use std::{
    ops::AddAssign,
    sync::atomic::Ordering,
    time::{self, Duration},
};

use symphonia::core::{audio::Signal, formats::SeekedTo};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::info;

use crate::{
    DecoderWrapper::{DecodeResult, DecoderWrapper},
    ResamplerWrapper::{self, RsamplerWrapper},
    app::PlayerToUISingnal,
    manipulation::{
        CHANNEL, DecoderControlSignal, DecoderToRendererSyncSignal, DecorderToVeSyncSignal,
        EndOfStreamSignal, NUM_OF_BLOCK, RendererToDecoderSsynSignal, SampleRate,
        SeekCompleteFromDecoderSignal, SeekSignalForDecoder, SeekSignalForDecoderVeChannels,
        SeekTimingSyncObj, SharedBuffer, VeEnabledInfoFromDecoder,
        VeEnabledSignalFromPlayerToDecoder, VeToDecoderSyncSignal, WorkerToPlayerNotification,
    },
    utils::array_init,
};

pub const BLOCK_SIZE_DEFAULT: usize = 1024 * 16;

pub fn decode_loop(
    decoder_wrapper: &mut DecoderWrapper,
    shared_buffer: &mut SharedBuffer,
    audio_device_sample_rate: SampleRate,
    mut decoder_to_renderer_sender: UnboundedSender<DecoderToRendererSyncSignal>,
    mut renderer_to_decoder_singal_recv: UnboundedReceiver<RendererToDecoderSsynSignal>,
    mut decoder_control_signal: UnboundedReceiver<DecoderControlSignal>,
    decoder_to_player_notification_signal: UnboundedSender<WorkerToPlayerNotification>,
    player_to_ui_singnal_sender: UnboundedSender<PlayerToUISingnal>,
) {
    struct VeSignal {
        pub decoder_to_ve_signal_sender: UnboundedSender<DecorderToVeSyncSignal>,
        pub ve_to_decoder_signal_recv: UnboundedReceiver<VeToDecoderSyncSignal>,
    }
    let mut ve_signal: Option<VeSignal> = None;

    const NUM_OF_BLOCK_LAST_INDEX: usize = NUM_OF_BLOCK - 1;
    let next_block = |write_end| match write_end {
        NUM_OF_BLOCK_LAST_INDEX => 0,
        write_end => write_end + 1,
    };

    let singnal_to_thread =
        |renderer_to_decoder_singal_recv: &mut UnboundedReceiver<RendererToDecoderSsynSignal>,
         decoder_to_renderer_sender: &UnboundedSender<DecoderToRendererSyncSignal>,
         ve_signal: &mut Option<VeSignal>,
         send_signal: DecoderToRendererSyncSignal| {
            //info!("singnal_to_threal_decoer_a");
            renderer_to_decoder_singal_recv.blocking_recv();
            if let Some(VeSignal {
                ve_to_decoder_signal_recv,
                decoder_to_ve_signal_sender,
            }) = ve_signal
            {
                ve_to_decoder_signal_recv.blocking_recv();
                decoder_to_ve_signal_sender.send(DecorderToVeSyncSignal());
            }
            //info!("singnal_to_threal_decoer_b");
            let ret = decoder_to_renderer_sender.send(send_signal);
            //info!("singnal_to_threal_decoer_c");
            ret
        };
    let singnal_to_thread_sync =
        |renderer_to_decoder_singal_recv: &mut UnboundedReceiver<RendererToDecoderSsynSignal>,
         decoder_to_renderer_sender: &UnboundedSender<DecoderToRendererSyncSignal>,
         ve_signal: &mut Option<VeSignal>| {
            singnal_to_thread(
                renderer_to_decoder_singal_recv,
                decoder_to_renderer_sender,
                ve_signal,
                DecoderToRendererSyncSignal::Sync,
            )
        };

    let mem_copy_with_sample_rate_conversion =
        |target_buffer: &mut [Vec<f32>; 2], resample_container: &mut Option<ResampleContainer>| {
            let Some(ResampleContainer {
                decode_tmp_block,
                resampler_wrapper,
                block_size,
            }) = resample_container
            else {
                return;
            };

            // for ch in 0..CHANNEL {
            //     target_buffer[ch].resize(*block_size, 0f32);
            // }

            let [target_buffer_ch_0, target_buffer_ch_1] = target_buffer;

            resampler_wrapper.proc(
                &[0, 1].map(|x| decode_tmp_block[x].as_slice()),
                &mut [target_buffer_ch_0, target_buffer_ch_1],
            );
        };

    let get_threshold_len = |resample_container: &Option<ResampleContainer>| {
        if let Some(x) = resample_container {
            x.decode_tmp_block[0].len()
        } else {
            BLOCK_SIZE_DEFAULT
        }
    };

    let mut write_exclusive: usize = 0;
    let mut head: usize = 0;

    let mut end_of_stream_reached = false;

    let mut current_played_sample: u64 = 0;

    let Some(file_sample_rate) = decoder_wrapper.get_sample_rate() else {
        return;
    };

    let Ok(file_sample_rate) = SampleRate::try_from(file_sample_rate as usize) else {
        return;
    };
    let get_block_count = |current_played_sample: u64,resample_container: &Option<ResampleContainer>| {
        let block_size = get_threshold_len(resample_container);
        current_played_sample /block_size as u64
    };

    struct ResampleContainer {
        decode_tmp_block: [Vec<f32>; 2],
        resampler_wrapper: RsamplerWrapper,
        block_size: usize,
    }

    let mut resample_container: Option<ResampleContainer> =
        if audio_device_sample_rate == file_sample_rate {
            None
        } else {
            use SampleRate::*;
            let base = match file_sample_rate {
                R44100 | R88200 | R176400 => 147,
                R48000 | R96000 | R192000 => 160,
            };
            let multiple = match file_sample_rate {
                R44100 | R48000 => 1,
                R88200 | R96000 => 2,
                R176400 | R192000 => 4,
            };

            let decode_tmp_block = (base * multiple * 100) as usize;

            Some(ResampleContainer {
                decode_tmp_block: array_init(|| vec![0f32; decode_tmp_block]),
                resampler_wrapper: RsamplerWrapper::new(
                    file_sample_rate as usize,
                    audio_device_sample_rate.rawValue(),
                    base * multiple,
                )
                .unwrap(),
                block_size: (decode_tmp_block * audio_device_sample_rate.rawValue())
                    / file_sample_rate.rawValue(),
            })
        };

    let get_block_size = || match resample_container {
        Some(ref x) => x.block_size,
        None => BLOCK_SIZE_DEFAULT,
    };

    let mut type_conversion_buff: [Vec<f32>; CHANNEL] = array_init(|| vec![0f32; get_block_size()]);

    for i in 0..shared_buffer.len() {
        shared_buffer[i] = array_init(|| vec![0f32; get_block_size()]);
    }

    macro_rules! get_exclusizebuff {
        () => {
            if let Some(ResampleContainer {
                ref mut decode_tmp_block,
                ..
            }) = resample_container
            {
                decode_tmp_block
            } else {
                &mut shared_buffer[write_exclusive]
            }
        };
    }

    'l1: loop {
        let get_block_size = || {
            if let Some(ResampleContainer { block_size, .. }) = resample_container {
                block_size
            } else {
                BLOCK_SIZE_DEFAULT
            }
        };
        if end_of_stream_reached || !decoder_control_signal.is_empty() {
            info!("decoder_control_signal.blocking_recv() ");
            match decoder_control_signal.blocking_recv() {
                Some(DecoderControlSignal::Stop) => {
                    _ = singnal_to_thread_sync(
                        &mut renderer_to_decoder_singal_recv,
                        &decoder_to_renderer_sender,
                        &mut ve_signal,
                    );
                    break 'l1;
                }
                Some(DecoderControlSignal::VeEnabled(VeEnabledSignalFromPlayerToDecoder {
                    decoder_to_ve_signal_sender,
                    decorder_to_ve_signal_recv,
                    ve_to_decoder_signal_sender,
                    ve_to_decoder_signal_recv,
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
                            get_block_count(current_played_sample,&resample_container) as i32 + len as i32 + 1
                                - NUM_OF_BLOCK as i32;

                        let target_duration = (target_block_count as f64 * get_block_size() as f64)
                            / audio_device_sample_rate.rawValue() as f64;

                        target_duration
                    };

                    decoder_to_player_notification_signal.send(
                        WorkerToPlayerNotification::VeEnabledInfo(VeEnabledInfoFromDecoder {
                            audio_device_sample_rate: audio_device_sample_rate.rawValue(),
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
                }
                Some(DecoderControlSignal::Seek(SeekSignalForDecoder {
                    sync_obj,
                    target_duration,
                    seek_no,
                    decoder_to_renderer_sync_signal_sender,
                    renderer_to_decoder_sync_signal_recv,
                    seek_signal_for_decoder_ve_channels,
                })) => 'b1: {
                    info!("decoder_seek_signal");

                    let (sync_obj_increment, ve_enabled_flg_for_completed_notification) =
                        match seek_signal_for_decoder_ve_channels {
                            Some(_) => (1, true),
                            None => (2, false),
                        };

                    decoder_to_renderer_sender = decoder_to_renderer_sync_signal_sender;
                    renderer_to_decoder_singal_recv = renderer_to_decoder_sync_signal_recv;

                    if let Some(SeekSignalForDecoderVeChannels {
                        decoder_to_ve_sync_signal_sender,
                        ve_to_decoder_sync_signal_recv,
                    }) = seek_signal_for_decoder_ve_channels
                    {
                        ve_signal = Some(VeSignal {
                            decoder_to_ve_signal_sender: decoder_to_ve_sync_signal_sender,
                            ve_to_decoder_signal_recv: ve_to_decoder_sync_signal_recv,
                        });
                    }
                    let mut seek = || {
                        let seeked = decoder_wrapper.seek(target_duration);

                        let convert_sample_count_to_sec_f64 =
                            |ts| ts as f64 / file_sample_rate.rawValue() as f64;

                        match seeked {
                            Ok(SeekedTo { actual_ts, .. }) => {
                                write_exclusive = 0;
                                head = 0;
                                end_of_stream_reached = false;
                                current_played_sample = actual_ts;
                                convert_sample_count_to_sec_f64(actual_ts)
                            }
                            Err(x) => {
                                info!("Decode Init Error {x}");
                                convert_sample_count_to_sec_f64(current_played_sample)
                            }
                        }
                    };
                    let actual_ts_sec_f64 = seek();

                    sync_obj.complete_sync.wait(sync_obj_increment);
                    let signal = SeekCompleteFromDecoderSignal {
                        actual_seek_duration_sec: actual_ts_sec_f64,
                        seek_no,
                        ve_enabled: ve_enabled_flg_for_completed_notification,
                    };
                    decoder_to_player_notification_signal
                        .send(WorkerToPlayerNotification::SeekCompleteFromDecoder(signal));
                    info!("decoder_seek_completed");
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
            let fill_buff_within_block = |exclusive_buff: &mut [Vec<f32>; 2]| {
                let copy_buff = |exclusive_buf: &mut [f32], view: &[f32]| {
                    let target_slice = &mut exclusive_buf[head..head + view.len()];
                    target_slice.copy_from_slice(view);
                };

                for ch in 0..CHANNEL {
                    copy_buff(exclusive_buff[ch].as_mut_slice(), view[ch]);
                }
            };

            let target_len = head + view[0].len();

            use std::cmp::Ordering::*;
            match Ord::cmp(&target_len, &get_threshold_len(&resample_container)) {
                Less => {
                    fill_buff_within_block(get_exclusizebuff!());
                    head = head + view[0].len();
                }
                Equal | Greater => {
                    fn fill_current_block_buff<'a>(
                        head: usize,
                        exclusive_buf: &mut [f32],
                        view: &'a [f32],
                    ) -> &'a [f32] {
                        let exclusive_buf_len = exclusive_buf.len();
                        let target_slice_spill_over = &mut exclusive_buf[head..];

                        let (current_view, spill_over_view) =
                            view.split_at(exclusive_buf_len - head);
                        target_slice_spill_over.copy_from_slice(current_view);

                        spill_over_view
                    }

                    let spill_over = [0, 1].map(|ch| {
                        fill_current_block_buff(
                            head,
                            get_exclusizebuff!()[ch].as_mut_slice(),
                            view[ch],
                        )
                    });

                    mem_copy_with_sample_rate_conversion(
                        &mut shared_buffer[write_exclusive],
                        &mut resample_container,
                    );

                    _ = singnal_to_thread_sync(
                        &mut renderer_to_decoder_singal_recv,
                        &decoder_to_renderer_sender,
                        &mut ve_signal,
                    );

                    write_exclusive = next_block(write_exclusive);

                    let fill_spill_over_block_buff =
                        |exclusive_buf_spill_over: &mut [f32], spill_over_view: &[f32]| {
                            let target_slice_spill_over =
                                &mut exclusive_buf_spill_over[0..spill_over_view.len()];

                            target_slice_spill_over.copy_from_slice(spill_over_view);
                        };

                    for ch in 0..CHANNEL {
                        fill_spill_over_block_buff(
                            get_exclusizebuff!()[ch].as_mut_slice(),
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
                    current_played_sample.add_assign(cow.chan(0).len() as u64);
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
                    current_played_sample.add_assign(cow.chan(0).len() as u64);
                }
                _ => {}
            },
            DecodeResult::Err(error) => {
                return;
                //println!("error: {error}")
            }
            DecodeResult::EndOfStream => {
                // for channel_data_ref in &mut shared_buffer[write_exclusive] {
                //     channel_data_ref[head..].fill(0f32);
                // }

                mem_copy_with_sample_rate_conversion(
                    &mut shared_buffer[write_exclusive],
                    &mut resample_container,
                );

                _ = singnal_to_thread(
                    &mut renderer_to_decoder_singal_recv,
                    &decoder_to_renderer_sender,
                    &mut ve_signal,
                    DecoderToRendererSyncSignal::EndOfStream(EndOfStreamSignal {
                        last_block: write_exclusive,
                        filled_len: head,
                    }),
                );

                write_exclusive = next_block(write_exclusive);

                player_to_ui_singnal_sender.send(PlayerToUISingnal::EndOfStream);

                end_of_stream_reached = true;
            }
            DecodeResult::None => continue,
        }
    }
}
