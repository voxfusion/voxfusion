//! Carries [`AppEvent`]s from the services, which run on their own threads,
//! to the windows.

use gpui_kit::{App, AppContext as _, AsyncApp, Entity, EventEmitter, Global};

use crate::backend::{AppEvent, EventSender};

/// Subscribe to this entity to receive every [`AppEvent`].
pub struct EventHub;

impl EventEmitter<AppEvent> for EventHub {}

struct GlobalEvents {
    hub: Entity<EventHub>,
    sender: EventSender,
}

impl Global for GlobalEvents {}

/// The receiving end of the event channel, until [`init`] takes it.
pub struct EventReceiver(async_channel::Receiver<AppEvent>);

/// Creates the channel events travel on. This needs no running app, so
/// services that start before it can already be given the sender.
pub fn channel() -> (EventSender, EventReceiver) {
    let (sender, receiver) = async_channel::unbounded::<AppEvent>();

    (EventSender(sender), EventReceiver(receiver))
}

/// Starts delivering the events sent on the channel.
pub fn init(sender: EventSender, receiver: EventReceiver, cx: &mut App) {
    let hub = cx.new(|_| EventHub);

    cx.set_global(GlobalEvents {
        hub: hub.clone(),
        sender,
    });

    cx.spawn(async move |cx: &mut AsyncApp| {
        while let Ok(event) = receiver.0.recv().await {
            hub.update(cx, |_, cx| cx.emit(event));
        }
    })
    .detach();
}

pub fn hub(cx: &App) -> Entity<EventHub> {
    cx.global::<GlobalEvents>().hub.clone()
}

/// Raises `event` after the current update.
pub fn emit(cx: &App, event: AppEvent) {
    cx.global::<GlobalEvents>().sender.emit(event);
}
