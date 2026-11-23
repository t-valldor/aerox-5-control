//! HID discovery, device information and the dedicated worker thread.
//!
//! HID calls must not run on the GUI event thread. On macOS `hid_enumerate()`
//! pumps the current run loop, which re-enters winit's event handler while it
//! is already handling an event and aborts the process. A dedicated thread
//! gives hidapi its own run loop to pump.

use std::ffi::CStr;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;

use hidapi::{DeviceInfo, HidApi};

use crate::core::protocol::Link;

pub const STEELSERIES_VID: u16 = 0x1038;
/// Product IDs exposed when the mouse is connected over USB-C (including the
/// wired-only Aerox 5 and the Destiny 2 / Diablo IV editions).
pub const USB_PIDS: [u16; 4] = [0x1850, 0x1854, 0x185E, 0x1862];
/// Product IDs exposed by the 2.4 GHz dongle for the same mice.
pub const DONGLE_PIDS: [u16; 3] = [0x1852, 0x185C, 0x1860];
pub const AEROX5_PIDS: [u16; 7] = [0x1850, 0x1852, 0x1854, 0x185C, 0x185E, 0x1860, 0x1862];

/// Configuration interface used by the Aerox 5 (interface 3 in most modes).
pub const CONFIG_INTERFACE: i32 = 3;
pub const WIRED_ONLY_PID: u16 = 0x1850;

/// How the mouse is physically attached, derived from the USB product ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionKind {
    UsbC,
    WirelessDongle,
    Unknown,
}

impl ConnectionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::UsbC => "USB-C (cable)",
            Self::WirelessDongle => "2.4 GHz dongle",
            Self::Unknown => "Unknown",
        }
    }
}

/// Everything readable from a HID device entry without opening the device.
#[derive(Clone, Debug)]
pub struct DeviceInfoView {
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_name: String,
    pub hid_product: Option<String>,
    pub manufacturer: Option<String>,
    pub serial: Option<String>,
    pub release_number: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub interface_number: i32,
    pub bus_type: String,
    pub path: String,
    pub connection: ConnectionKind,
}

/// Battery data decoded according to `docs/com-spec.md` §6: the charging flag
/// is bit 7 of the second response byte and the level is `((b & 0x7F) - 1) * 5`.
#[derive(Clone, Debug)]
pub struct BatteryInfo {
    pub level: Option<u8>,
    pub charging: Option<bool>,
    pub raw: [u8; 2],
}

pub enum HidCommand {
    Scan {
        /// Scan this request belongs to; a reply from an older scan is ignored by the UI.
        generation: u64,
    },
    QueryBattery {
        pid: u16,
        link: Link,
        /// Scan this query belongs to; a reply from an older scan is ignored by the UI.
        generation: u64,
    },
    ApplySettings {
        pid: u16,
        link: Link,
        packets: Vec<Vec<u8>>,
    },
}

pub enum HidResponse {
    /// An `IOKit` arrival/removal notification fired; the device list may have changed.
    Hotplug,
    Scan {
        info: Option<DeviceInfoView>,
        status: String,
        /// Echoed from `HidCommand::Scan` so the UI can drop stale replies.
        generation: u64,
    },
    Battery {
        info: Option<BatteryInfo>,
        status: String,
        generation: u64,
    },
    Apply {
        status: String,
    },
}

fn connection_for_pid(pid: u16) -> ConnectionKind {
    if DONGLE_PIDS.contains(&pid) {
        ConnectionKind::WirelessDongle
    } else if USB_PIDS.contains(&pid) {
        ConnectionKind::UsbC
    } else {
        ConnectionKind::Unknown
    }
}

pub fn product_name_for_pid(pid: u16) -> &'static str {
    match pid {
        0x1850 => "SteelSeries Aerox 5",
        0x1854 => "Aerox 5 Wireless (USB-C)",
        0x185E => "Aerox 5 Wireless Destiny 2 (USB-C)",
        0x1862 => "Aerox 5 Wireless Diablo IV (USB-C)",
        0x1852 => "Aerox 5 Wireless (2.4 GHz)",
        0x185C => "Aerox 5 Wireless Destiny 2 (2.4 GHz)",
        0x1860 => "Aerox 5 Wireless Diablo IV (2.4 GHz)",
        _ => "Unknown device",
    }
}

/// hidapi exposes the device path as a `CStr`; render the raw bytes as hex so
/// it can be shown in the GUI.
fn path_to_hex(path: &CStr) -> String {
    path.to_bytes()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn describe_device(info: &DeviceInfo) -> DeviceInfoView {
    let pid = info.product_id();
    DeviceInfoView {
        vendor_id: info.vendor_id(),
        product_id: pid,
        product_name: product_name_for_pid(pid).to_owned(),
        hid_product: info.product_string().map(str::to_owned),
        manufacturer: info.manufacturer_string().map(str::to_owned),
        serial: info.serial_number().map(str::to_owned),
        release_number: info.release_number(),
        usage_page: info.usage_page(),
        usage: info.usage(),
        interface_number: info.interface_number(),
        bus_type: format!("{:?}", info.bus_type()),
        path: path_to_hex(info.path()),
        connection: connection_for_pid(pid),
    }
}

pub fn find_aerox5(api: &HidApi) -> Option<DeviceInfoView> {
    api.device_list()
        .find(|d| d.vendor_id() == STEELSERIES_VID && AEROX5_PIDS.contains(&d.product_id()))
        .map(describe_device)
}

fn no_device_status() -> String {
    "No Aerox 5 found. Check cable/dongle.".to_owned()
}

#[expect(
    clippy::too_many_lines,
    reason = "the worker is one cohesive command-dispatch loop; splitting it would scatter the protocol"
)]
pub fn hid_worker(commands: Receiver<HidCommand>, responses: &Sender<HidResponse>) {
    // Keep the HID work on this thread: hidapi keeps global state (the
    // IOHIDManager) bound to the run loop of the thread that uses it.
    for command in commands {
        let response = match command {
            HidCommand::Scan { generation } => match HidApi::new() {
                Ok(api) => {
                    let info = find_aerox5(&api);
                    let status = match &info {
                        Some(info) => format!(
                            "Connected: {} (PID 0x{:04X}, {})",
                            info.product_name,
                            info.product_id,
                            info.connection.label()
                        ),
                        None => no_device_status(),
                    };
                    HidResponse::Scan {
                        info,
                        status,
                        generation,
                    }
                }
                Err(e) => HidResponse::Scan {
                    info: None,
                    status: format!("HID error: {e}"),
                    generation,
                },
            },
            HidCommand::QueryBattery {
                pid,
                link,
                generation,
            } => {
                let (info, status) = match HidApi::new() {
                    Ok(api) => {
                        let target = api.device_list().find(|d| {
                            d.vendor_id() == STEELSERIES_VID
                                && d.product_id() == pid
                                && d.interface_number() == CONFIG_INTERFACE
                        });
                        match target.and_then(|device_info| device_info.open_device(&api).ok()) {
                            Some(device) => {
                                // Report ID 0x00 + mode-specific command. The dongle
                                // command is 0x92 | 0x40 = 0xD2 (com-spec.md §6).
                                let command = [
                                    0x00,
                                    match link {
                                        Link::Wired => 0x92,
                                        Link::Dongle => 0xD2,
                                    },
                                ];
                                match device.write(&command) {
                                    Ok(_) => {
                                        // rivalcfg waits 50 ms after every HID write.
                                        thread::sleep(Duration::from_millis(50));
                                        let mut response = [0u8; 2];
                                        match device.read_timeout(&mut response, 200) {
                                            Ok(2) => {
                                                let charging = response[1] & 0x80 != 0;
                                                let raw_level = i16::from(response[1] & 0x7F);
                                                let decoded = (raw_level - 1) * 5;
                                                let level =
                                                    (0..=100).contains(&decoded).then(|| {
                                                        u8::try_from(decoded)
                                                            .expect("level is within 0..=100")
                                                    });
                                                let status = match level {
                                                    Some(percent) => format!(
                                                        "Battery: {percent}%{}",
                                                        if charging { " (charging)" } else { "" }
                                                    ),
                                                    None => "Could not read the battery level. Is the mouse turned on?"
                                                        .to_owned(),
                                                };
                                                (
                                                    Some(BatteryInfo {
                                                        level,
                                                        charging: Some(charging),
                                                        raw: response,
                                                    }),
                                                    status,
                                                )
                                            }
                                            Ok(bytes) => (
                                                None,
                                                format!(
                                                    "Incomplete battery response ({bytes} bytes)."
                                                ),
                                            ),
                                            Err(e) => (None, format!("Read error: {e}")),
                                        }
                                    }
                                    Err(e) => (None, format!("Write error: {e}")),
                                }
                            }
                            None => (
                                None,
                                "Could not open the configuration interface (interface 3)."
                                    .to_owned(),
                            ),
                        }
                    }
                    Err(e) => (None, format!("HID error: {e}")),
                };
                HidResponse::Battery {
                    info,
                    status,
                    generation,
                }
            }
            HidCommand::ApplySettings { pid, link, packets } => {
                let status = match HidApi::new() {
                    Ok(api) => {
                        let target = api.device_list().find(|d| {
                            d.vendor_id() == STEELSERIES_VID
                                && d.product_id() == pid
                                && d.interface_number() == CONFIG_INTERFACE
                        });
                        match target.and_then(|info| info.open_device(&api).ok()) {
                            Some(device) => {
                                let mut failure = None;
                                for (index, packet) in packets.iter().enumerate() {
                                    if let Err(e) = device.write(packet) {
                                        failure = Some(format!(
                                            "Write error on packet {} of {}: {e}",
                                            index + 1,
                                            packets.len()
                                        ));
                                        break;
                                    }
                                    // rivalcfg waits 50 ms after every write.
                                    thread::sleep(Duration::from_millis(50));
                                    if link == Link::Dongle {
                                        // Wireless profiles perform a 64-byte
                                        // readback; its contents are not decoded.
                                        let mut drain = [0u8; 64];
                                        let _ = device.read_timeout(&mut drain, 200);
                                    }
                                }
                                failure.unwrap_or_else(|| "Settings sent to the mouse.".to_owned())
                            }
                            None => "Could not open the configuration interface (interface 3)."
                                .to_owned(),
                        }
                    }
                    Err(e) => format!("HID error: {e}"),
                };
                HidResponse::Apply { status }
            }
        };

        if responses.send(response).is_err() {
            break;
        }
    }
}
