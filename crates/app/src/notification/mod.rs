//! Notification center and popup UI.

mod card;
mod config;
mod panel;
mod popup;

pub use config::{NotificationConfig, NotificationPopupPosition};
pub(crate) use panel::NotificationCenter;
pub use popup::init;

use services::{NotificationCommand, NotificationSubscriber};

pub(crate) fn dispatch_notification_command(
    subscriber: NotificationSubscriber,
    command: NotificationCommand,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime for notification command");
        rt.block_on(async move {
            let _ = subscriber.dispatch(command).await;
        });
    });
}
