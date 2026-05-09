use std::cmp;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    manipulation::{
        BLOCK_SIZE, CHANNEL, DecoderVisualEffectThreadSyncSignal, NUM_OF_BLOCK_VE,
        OscilloscopeData, SharedBuffer, UiVEThreadSyncSignal, VESharedBuffer, VeControlSignal,
    },
    utils::array_init,
};

const FRAME_RATE:usize = 60;

pub fn ve_loop(
    shared_buffer: &SharedBuffer,
    ve_shared_buffer: &mut Option<VESharedBuffer>,
    ve_to_decoder_signal_sender: UnboundedSender<DecoderVisualEffectThreadSyncSignal>,
    mut decoder_to_ve_recv: UnboundedReceiver<DecoderVisualEffectThreadSyncSignal>,
    ve_to_ui_signal_sender: UnboundedSender<UiVEThreadSyncSignal>,
    mut ui_to_ve_recv: UnboundedReceiver<UiVEThreadSyncSignal>,
    mut ve_control_signal_recv: UnboundedReceiver<VeControlSignal>,
) {
    let next_block = |read_exclusive| match read_exclusive {
        7 => 0,
        read_exclusive => read_exclusive + 1,
    };

    let mut signal_to_decoder_thread = || {
        decoder_to_ve_recv.blocking_recv();
        ve_to_decoder_signal_sender.send(DecoderVisualEffectThreadSyncSignal())
    };

    let mut signal_to_ui_thread = || {
        ui_to_ve_recv.blocking_recv();
        ve_to_ui_signal_sender.send(UiVEThreadSyncSignal())
    };

    let mut read_head: usize = 0;
    let mut ve_buff_write_head: usize = 0;
    let mut read_exclusive: usize = 0;
    let mut write_exclusive: usize = 0;
    let mut count = 0;

    let (ve_shared_buffer,move_window) = {
        let sample_rate = match ve_control_signal_recv.blocking_recv() {
            Some(VeControlSignal::NoticeSampleRate(sample_rate)) => sample_rate,
            None => return,
        };
        let move_window = sample_rate / FRAME_RATE;
        *ve_shared_buffer = Some(array_init(|| {
            array_init(|| OscilloscopeData(vec![0f32; move_window]))
        }));
        let buffer_ref = ve_shared_buffer
            .as_mut()
            .expect("must be init above statement");

        (buffer_ref, move_window)
    };

    _ = signal_to_decoder_thread();

    // loop {
    //     signal_to_decoder_thread();

    //     signal_to_ui_thread();
    // }

    // return;

    'l1: loop {
        // 'control_singal_loop: loop {
        //     if ve_control_signal_recv.is_empty() {
        //         break 'control_singal_loop;
        //     }

        //     let reciever = &mut ve_control_signal_recv;
        //     match reciever.blocking_recv() {
        //         Some(RendererControlSignal::Stop) => {
        //             _ = self.audio_client.Stop();
        //             _ = signal_to_decoder_thread();
        //             break 'l1;
        //         }
        //         _ => break 'l1,
        //     }
        // }

        let available = ve_shared_buffer[write_exclusive][0].0.len() - ve_buff_write_head;

        let mut fill_buff_within_block = || {
            for ch in 0..CHANNEL {
                let target = &mut ve_shared_buffer[write_exclusive][ch].0;
                let src = &shared_buffer[read_exclusive][ch][read_head..read_head + target.len()];
                target.copy_from_slice(src);
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
                        target.copy_from_slice(src);
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
                        target.copy_from_slice(src);
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
