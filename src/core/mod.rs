//! Non-UI logic: configuration storage, HID discovery, the `IOKit` hotplug
//! listener and the mouse's packet protocol.

pub mod configs;
pub mod device;
pub mod hotplug;
pub mod protocol;
