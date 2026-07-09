use std::{cmp, time::Duration};

use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender, error::TryRecvError, unbounded_channel,
};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    app::PlayerToUISingnal,
    manipulation::{
        CHANNEL, DecorderToVeSyncSignal, NUM_OF_BLOCK, NUM_OF_BLOCK_VE, OscilloscopeData,
        SeekCompleteFromVeSignal, SeekSignalForVe, SharedBuffer, UiVEThreadSyncSignal,
        VESharedBuffer, VeControlSignal, VeToDecoderSyncSignal, WorkerToPlayerNotification,
    },
    utils::array_init,
};

pub enum VeControlSignalInner {
    Seek(SeekSignalForVe),
    Stop,
}

pub const FRAME_RATE: usize = 60;

pub fn ve_loop(
    shared_buffer: &SharedBuffer,
    ve_shared_buffer: &mut VESharedBuffer,
    mut ve_to_decoder_signal_sender: UnboundedSender<VeToDecoderSyncSignal>,
    mut decoder_to_ve_recv: UnboundedReceiver<DecorderToVeSyncSignal>,
    mut ve_to_ui_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    mut ui_to_ve_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
    mut ve_control_signal_inner_recv: UnboundedReceiver<VeControlSignalInner>,
    worker_to_player_notification_signal_sender_for_ve: &UnboundedSender<
        WorkerToPlayerNotification,
    >,
    // ve_cancellation_token: CancellationToken,
    sample_rate: usize,
    mut read_exclusive: usize,
) {
    const NUM_OF_BLOCK_LAST_INDEX: usize = NUM_OF_BLOCK - 1;
    let next_block = |read_exclusive| match read_exclusive {
        NUM_OF_BLOCK_LAST_INDEX => 0,
        read_exclusive => read_exclusive + 1,
    };

    let mut read_head: usize = 0;
    let mut ve_buff_write_head: usize = 0;
    //let mut read_exclusive = read_exclusive;

    let mut write_exclusive: usize = 0;
    let mut count = 0;

    let move_window = sample_rate / FRAME_RATE;

    let mut signal_to_decoder_thread =
        |decoder_to_ve_recv: &mut UnboundedReceiver<DecorderToVeSyncSignal>,
         ve_to_decoder_signal_sender: &UnboundedSender<VeToDecoderSyncSignal>| {
            decoder_to_ve_recv.blocking_recv();
            ve_to_decoder_signal_sender.send(VeToDecoderSyncSignal())
        };

    let mut signal_to_ui_thread =
        |ui_to_ve_recv: &mut UnboundedReceiver<UiVEThreadSyncSignal>,
         ve_to_ui_signal_sender: &UnboundedSender<UiVEThreadSyncSignal>| {
            ui_to_ve_recv.blocking_recv();
            ve_to_ui_signal_sender.send(UiVEThreadSyncSignal())
        };

    // loop {
    //     signal_to_decoder_thread();

    //     signal_to_ui_thread();
    // }

    // return;


    'l1: loop {
        let get_block_size = |read_exclusive: usize| shared_buffer[read_exclusive][0].len();

        match ve_control_signal_inner_recv.try_recv() {
            Ok(VeControlSignalInner::Seek(SeekSignalForVe {
                sync_obj,
                ve_to_decoder_sync_signal_sender: ve_to_decoder_sync_signal_sender_got,
                decoder_to_ve_sync_signal_recv: decoder_to_ve_sync_signal_recv_got,
            })) => {
                //     sync_obj.init_sync.wait(1);
                read_exclusive = 0;
                read_head = 0;
                ve_buff_write_head = 0;
                write_exclusive = 0;

                ve_to_decoder_signal_sender = ve_to_decoder_sync_signal_sender_got;
                decoder_to_ve_recv = decoder_to_ve_sync_signal_recv_got;
                sync_obj.complete_sync.wait(1);

                let (ve_to_ui_signal_sender_temp, ve_to_ui_signal_recv) = unbounded_channel();
                let (ui_to_ve_signal_sender, ui_to_ve_signal_recv_temp) = unbounded_channel();

                for _ in 0..NUM_OF_BLOCK_VE - 2 {
                    ui_to_ve_signal_sender.send(UiVEThreadSyncSignal());
                }

                ve_to_ui_signal_sender = ve_to_ui_signal_sender_temp;
                ui_to_ve_recv = ui_to_ve_signal_recv_temp;

                worker_to_player_notification_signal_sender_for_ve.send(
                    WorkerToPlayerNotification::SeekCompleteFromVe(SeekCompleteFromVeSignal {
                        ui_to_ve_signal_sender,
                        ve_to_ui_signal_recv,
                    }),
                );

                signal_to_decoder_thread(&mut decoder_to_ve_recv, &ve_to_decoder_signal_sender);
            }
            Ok(VeControlSignalInner::Stop) | Err(TryRecvError::Disconnected) => break 'l1,
            Err(TryRecvError::Empty) => {}
        }
        // if ve_cancellation_token.is_cancelled() {
        //     break 'l1;
        // }

        fn mem_copy_with_conversion(src: &[f32], target: &mut [(f64, f64)]) {
            for (i, ele) in src.iter().enumerate() {
                target[i].1 = *ele as f64;
            }
        }

        let available = ve_shared_buffer[write_exclusive][0].0.len() - ve_buff_write_head;

        let mut fill_buff_within_block = || {
            for ch in 0..CHANNEL {
                let target = &mut ve_shared_buffer[write_exclusive][ch].0;
                let src = &shared_buffer[read_exclusive][ch][read_head..read_head + target.len()];
                mem_copy_with_conversion(src, target);
            }
        };

        let target_len = read_head + available;

        use std::cmp::Ordering::*;
        match Ord::cmp(&target_len, &get_block_size(read_exclusive)) {
            Less => {
                fill_buff_within_block();
                read_head = read_head + move_window;
            }
            Equal => {
                fill_buff_within_block();
                _ = signal_to_decoder_thread(&mut decoder_to_ve_recv, &ve_to_decoder_signal_sender);
                read_exclusive = next_block(read_exclusive);
                read_head = 0;
            }
            Greater => {
                if get_block_size(read_exclusive) > read_head {
                    for ch in 0..CHANNEL {
                        let src = &shared_buffer[read_exclusive][ch][read_head..];
                        let target = &mut ve_shared_buffer[write_exclusive][ch].0
                            [0..get_block_size(read_exclusive) - read_head];
                        //target.copy_from_slice(src);

                        mem_copy_with_conversion(src, target);
                    }
                }

                _ = signal_to_decoder_thread(&mut decoder_to_ve_recv, &ve_to_decoder_signal_sender);

                read_exclusive = next_block(read_exclusive);

                let moved_head = read_head + available - get_block_size(read_exclusive);
                {
                    let src_start = read_head.saturating_sub(get_block_size(read_exclusive));
                    let target_start = get_block_size(read_exclusive).saturating_sub(read_head);
                    for ch in 0..CHANNEL {
                        let src = &shared_buffer[read_exclusive][ch][src_start..moved_head];
                        let target = &mut ve_shared_buffer[write_exclusive][ch].0[target_start..];
                        //target.copy_from_slice(src);

                        mem_copy_with_conversion(src, target);
                    }
                }
                read_head = moved_head;
            }
        }
        _ = signal_to_ui_thread(&mut ui_to_ve_recv, &ve_to_ui_signal_sender);

        const BLOCK_VE_LAST_INDEX: usize = NUM_OF_BLOCK_VE - 1;
        write_exclusive = match write_exclusive {
            BLOCK_VE_LAST_INDEX => 0,
            x => x + 1,
        };
    }
}
