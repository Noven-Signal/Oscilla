// use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

// use crate::manipulation::{CHANNEL, DecoderVisualEffectThreadSyncSignal, OscilloscopeData, SharedBuffer, VESharedBuffer};

// fn render_loop(
//     shared_buffer: &SharedBuffer,
//     ve_shared_buffer: &mut VESharedBuffer,
//     ve_to_decoder_signal_sender: UnboundedSender<DecoderVisualEffectThreadSyncSignal>,
//     decoder_to_ve_recv: UnboundedReceiver<DecoderVisualEffectThreadSyncSignal>,
// ) -> Result<()> {
//     let next_block = |read_exclusive| match read_exclusive {
//         7 => 0,
//         read_exclusive => read_exclusive + 1,
//     };

//     let mut singnal_to_thread = || {
//         decoder_to_ve_recv.blocking_recv();
//         ve_to_decoder_signal_sender.send(DecoderVisualEffectThreadSyncSignal())
//     };

//     let mut read_head: usize = 0;
//     let mut write_head: usize = 0;
//     let mut read_exclusive: usize = 0;
//     let mut write_exlusize: usize = 0;
//     let mut count = 0;
//     let mut vol = 1f32;

//     _ = singnal_to_thread();
    
//     'l1: loop {
//         macro_rules! dec_get_block_clo {
//             ($ident: ident,$tt:tt) => {
//                 let $ident = |read_exclusive: usize| shared_buffer.$tt[read_exclusive];
//             };
//         }
//         dec_get_block_clo!(get_block_left, channel_left_data);
//         dec_get_block_clo!(get_block_right, channel_right_data);

//         let available = ve_shared_buffer[write_exlusize][0].0.len() - write_head;

//         let output_buffer = &mut ve_shared_buffer[write_exlusize];

//         let mut fill_buff_within_block = || {
//             let slice = shared_buffer[]
//             let src_slice_ch_0 = &get_block_left(read_exclusive)[read_head..read_head + available];
//             let src_slice_ch_1 = &get_block_right(read_exclusive)[read_head..read_head + available];

//             for i in 0..src_slice_ch_0.len() {
//                 output_buffer[i * CHANNEL] = src_slice_ch_0[i] * vol;
//                 output_buffer[i * CHANNEL + 1] = src_slice_ch_1[i] * vol;
//             }
//         };

//         let target_len = read_head + available;

//         use std::cmp::Ordering::*;
//         match Ord::cmp(&target_len, &BLOCK_SIZE) {
//             Less => {
//                 fill_buff_within_block();
//                 read_head = read_head + available;
//                 //println!("less");
//             }
//             Equal => {
//                 fill_buff_within_block();
//                 if let Err(_) = singnal_to_thread() {
//                     break 'l1;
//                 }

//                 read_exclusive = next_block(read_exclusive);
//                 read_head = 0;
//                 //println!("equal");
//             }
//             Greater => {
//                 {
//                     let src_slice_current_ch_0 = &get_block_left(read_exclusive)[read_head..];
//                     let src_slice_current_ch_1 = &get_block_right(read_exclusive)[read_head..];

//                     for i in 0..src_slice_current_ch_0.len() {
//                         output_buffer[i * CHANNEL] = src_slice_current_ch_0[i] * vol;
//                         output_buffer[i * CHANNEL + 1] = src_slice_current_ch_1[i] * vol;
//                     }
//                 }

//                 if let Err(_) = singnal_to_thread() {
//                     break 'l1;
//                 }

//                 read_exclusive = next_block(read_exclusive);

//                 let spill_over_len = read_head + available - BLOCK_SIZE;
//                 {
//                     assert!(spill_over_len < BLOCK_SIZE);

//                     let src_slice_spill_over_ch_0 =
//                         &get_block_left(read_exclusive)[0..spill_over_len];
//                     let src_slice_spill_over_ch_1 =
//                         &get_block_right(read_exclusive)[0..spill_over_len];

//                     let split_len_channel_combined = {
//                         let split_len_ch = BLOCK_SIZE - read_head;
//                         split_len_ch * CHANNEL
//                     };
//                     for i in 0..src_slice_spill_over_ch_0.len() {
//                         output_buffer[i * CHANNEL + split_len_channel_combined] =
//                             src_slice_spill_over_ch_0[i] * vol;
//                         output_buffer[i * CHANNEL + 1 + split_len_channel_combined] =
//                             src_slice_spill_over_ch_1[i] * vol;
//                     }
//                 }
//                 read_head = spill_over_len;
//             }
//         };

//     }
// }
