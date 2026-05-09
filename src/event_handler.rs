// use std::time::Duration;

// use crossterm::event::EventStream;
// use futures::{FutureExt, SinkExt, StreamExt , future};
// use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
// use tokio_util::sync::CancellationToken;

// use crate::app::PlayerToUISingnal;

// pub enum EventHndlerToAppSignal {
//     CrossTermEvent(crossterm::event::Event),
//     PlayerToUISingnal(PlayerToUISingnal),
//     VeTick,
// }

// pub struct EventHandler {
//     event_handler_to_app_sender: UnboundedSender<EventHndlerToAppSignal>,
//     player_to_ui_singnal_receiver: UnboundedReceiver<PlayerToUISingnal>,
//     ve_event_tick_enabled: bool,
//     cancellation_token: CancellationToken
// }

// impl EventHandler {
//     pub fn new(
//         event_handler_to_app_sender: UnboundedSender<EventHndlerToAppSignal>,
//         player_to_ui_singnal_receiver: UnboundedReceiver<PlayerToUISingnal>,
//         cancellation_token: CancellationToken
//     ) -> Self {
//         Self {
//             event_handler_to_app_sender,
//             player_to_ui_singnal_receiver,
//             ve_event_tick_enabled: false,
//             cancellation_token
//         }
//     }

//     pub async fn event_loop(&mut self) -> color_eyre::Result<()> {
//         let mut event_stream = EventStream::new();

//         let mut interval = tokio::time::interval(Duration::from_secs_f64(1f64 / 60f64));
//         'l1: loop {
//             if self.cancellation_token.is_cancelled() {
//                 break 'l1;
//             }

//             let mut get_future = async || {
//                 if self.ve_event_tick_enabled {
//                     interval.tick().await;
//                 } else {
//                     future::pending::<()>().await;
//                 }
//             };

//             let send_signal = tokio::select! {
//                 signal = self.player_to_ui_singnal_receiver.recv() => {
//                     match signal{
//                         Some(signal) => EventHndlerToAppSignal::PlayerToUISingnal(signal),
//                         None => break 'l1
//                     }
//                 }
//                 crossterm_event = event_stream.next().fuse() => match crossterm_event {
//                     Some(Result::Ok(event)) =>  EventHndlerToAppSignal::CrossTermEvent(event),
//                     _ => break 'l1,
//                 },
//                 _ = get_future() => EventHndlerToAppSignal::VeTick
//             };

//             if let Err(_) = self.event_handler_to_app_sender.send(send_signal){
//                 break 'l1;
//             }
//         }

//         Ok(())
//     }
// }
