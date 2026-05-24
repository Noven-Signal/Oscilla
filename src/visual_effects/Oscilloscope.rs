use std::cmp;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use crate::{
    manipulation::{
        BLOCK_SIZE, CHANNEL, DecorderToVeSyncSignal, NUM_OF_BLOCK_VE, OscilloscopeData,
        SharedBuffer, UiVEThreadSyncSignal, VESharedBuffer, VeControlSignal, VeToDecoderSyncSignal,
    },
    utils::array_init,
};

const FRAME_RATE: usize = 60;

pub fn ve_loop(
    shared_buffer: &SharedBuffer,
    ve_shared_buffer: &mut Option<VESharedBuffer>,
    ve_to_decoder_signal_sender: UnboundedSender<VeToDecoderSyncSignal>,
    mut decoder_to_ve_recv: UnboundedReceiver<DecorderToVeSyncSignal>,
    ve_to_ui_signal_sender: &UnboundedSender<UiVEThreadSyncSignal>,
    ui_to_ve_recv: &mut UnboundedReceiver<UiVEThreadSyncSignal>,
    ve_cancellation_token: CancellationToken,
    sample_rate: usize,
    mut read_exclusive: usize,
) {
    let next_block = |read_exclusive| match read_exclusive {
        7 => 0,
        read_exclusive => read_exclusive + 1,
    };

    let mut read_head: usize = 0;
    let mut ve_buff_write_head: usize = 0;
    //let mut read_exclusive = read_exclusive;

    let mut write_exclusive: usize = 0;
    let mut count = 0;

    let (ve_shared_buffer, move_window) = {
        let move_window = sample_rate / FRAME_RATE;

        *ve_shared_buffer = {
            let crate_move_window_size_vec =
                || (0..move_window).map(|i| (i as f64, 0f64)).collect();
            let arr = array_init(|| array_init(|| OscilloscopeData(crate_move_window_size_vec())));
            Some(arr)
        };
        let buffer_ref = ve_shared_buffer
            .as_mut()
            .expect("must be init above statement");

        (buffer_ref, move_window)
    };

    let mut signal_to_decoder_thread = || {
        decoder_to_ve_recv.blocking_recv();
        ve_to_decoder_signal_sender.send(VeToDecoderSyncSignal())
    };

    let mut signal_to_ui_thread = || {
        ui_to_ve_recv.blocking_recv();
        ve_to_ui_signal_sender.send(UiVEThreadSyncSignal())
    };

    // loop {
    //     signal_to_decoder_thread();

    //     signal_to_ui_thread();
    // }

    // return;

    'l1: loop {
        if ve_cancellation_token.is_cancelled() {
            break 'l1;
        }

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
        match Ord::cmp(&target_len, &BLOCK_SIZE) {
            Less => {
                fill_buff_within_block();
                read_head = read_head + move_window;
            }
            Equal => {
                fill_buff_within_block();
                if let Err(_) = signal_to_decoder_thread() {
                    break 'l1;
                }
                read_exclusive = next_block(read_exclusive);
                read_head = 0;
            }
            Greater => {
                if BLOCK_SIZE > read_head {
                    for ch in 0..CHANNEL {
                        let src = &shared_buffer[read_exclusive][ch][read_head..];
                        let target =
                            &mut ve_shared_buffer[write_exclusive][ch].0[0..BLOCK_SIZE - read_head];
                        //target.copy_from_slice(src);

                        mem_copy_with_conversion(src, target);
                    }
                }

                if let Err(_) = signal_to_decoder_thread() {
                    break 'l1;
                }

                read_exclusive = next_block(read_exclusive);

                let moved_head = read_head + available - BLOCK_SIZE;
                {
                    let src_start = read_head.saturating_sub(BLOCK_SIZE);
                    let target_start = BLOCK_SIZE.saturating_sub(read_head);
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
        _ = signal_to_ui_thread();

        const BLOCK_VE_LAST_INDEX: usize = NUM_OF_BLOCK_VE - 1;
        write_exclusive = match write_exclusive {
            BLOCK_VE_LAST_INDEX => 0,
            x => x + 1,
        };
    }
}
