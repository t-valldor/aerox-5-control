//! Application state, header/bottom bar, windows and eframe integration.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use eframe::egui::{
    self, Align, Color32, Frame, Label, Layout, Margin, Rect, RichText, Sense, Vec2,
};

use crate::core::configs::{self, ConfigBlock, ConfigStore};
use crate::core::device::{
    BatteryInfo, ConnectionKind, DeviceInfoView, HidCommand, HidResponse, WIRED_ONLY_PID,
    hid_worker,
};
use crate::core::protocol::{
    DefaultLighting, Link, PollingEncoding, color_packet, default_lighting_packet, dim_packet,
    polling_packet, rainbow_effect_packet, reactive_color_packet, save_packet, scaled_color,
    sensitivity_packet, sleep_packet,
};
use crate::ui::mouse::load_mouse_texture;
use crate::ui::panels::{POLLING_GRAPH_SECONDS, polling_pulse_count};
use crate::ui::theme::{
    BOTTOM_H, C_AMBER, C_BORDER, C_DIM, C_GREEN, C_HEADER, C_MUTED, C_ORANGE, C_PAGE, C_PANEL,
    C_PANEL_HEADER, C_TEXT, CENTER_W, COL_GAP, CONTENT_W, HEADER_H, LEFT_W, RGB_OFF, RIGHT_W,
    configure_style,
};
use crate::ui::widgets::{
    CPI_SWAP_SECONDS, configs_button, info_button, info_row, painted_button, status_dot,
};

/// Number of event rows visible in the expanded log before it starts scrolling.
const EVENT_LOG_MAX_VISIBLE_ROWS: f32 = 15.0;
/// Kept session events; older entries are dropped so a long-running GUI cannot grow forever.
const EVENT_LOG_MAX_ENTRIES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    Settings,
    Illumination,
}

/// A configuration-list action waiting for confirmation inside the CONFIGS window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingConfigAction {
    Switch(String),
    Delete(String),
}

/// One timestamped entry in the session event history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventEntry {
    pub timestamp: String,
    pub message: String,
}

/// Chronological event history; the UI renders it newest-first.
#[derive(Clone, Debug, Default)]
pub struct EventLog {
    entries: VecDeque<EventEntry>,
}

impl EventLog {
    fn record(&mut self, message: impl Into<String>) {
        if self.entries.len() == EVENT_LOG_MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(EventEntry {
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            message: message.into(),
        });
    }

    fn iter_newest_first(&self) -> impl DoubleEndedIterator<Item = &EventEntry> {
        self.entries.iter().rev()
    }
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "the illumination switches and window-visibility flags are independent user/UI states"
)]
pub struct Aerox5App {
    pub status: String,
    /// Every status message from this session, oldest first.
    pub events: EventLog,
    /// Whether the clickable EVENT line is expanded into the history list.
    pub show_event_log: bool,
    /// Set when the list opens so its scroll area starts at the newest entry.
    event_log_reset_scroll: bool,
    pub connected_pid: Option<u16>,
    pub device_info: Option<DeviceInfoView>,
    pub battery: Option<BatteryInfo>,
    /// Bounded retry schedule while a just-connected wireless mouse has not answered yet.
    pub battery_retry: Option<BatteryRetry>,
    /// Incremented on every rescan so replies from an earlier scan can be discarded.
    pub battery_generation: u64,
    /// Texture for the centre-panel mouse image, uploaded once at startup.
    pub mouse_texture: egui::TextureHandle,

    pub cpi_levels: [u32; 5],
    pub cpi_count: usize,
    pub selected_level: usize,
    /// Source slot of the value now in each box while a swap animates; identity when settled.
    pub cpi_anim_from: [usize; 5],
    /// When the current swap started; `None` when all boxes are settled.
    pub cpi_anim_start: Option<Instant>,
    pub polling_rate: u32,
    /// Pulse-count morph state for the polling graph; `from`/`to` are counts, not rates.
    pub polling_graph_from: f32,
    pub polling_graph_to: f32,
    pub polling_graph_start: Option<Instant>,
    pub sleep_minutes: f32,
    pub dim_seconds: u32,
    pub color_top: [u8; 3],
    pub color_mid: [u8; 3],
    pub color_bot: [u8; 3],

    /// Named configurations read from the JSON file (or built-in defaults when none is saved yet).
    pub configs: ConfigStore,
    /// Resolved path of the config file — also shown in the CONFIGS window.
    pub config_path: PathBuf,
    /// Nonfatal sanitization notices from the last load or save (clamped counts/CPI values).
    pub config_warnings: Vec<String>,
    /// Inline confirmation state for the CONFIGS window (never shown elsewhere).
    pub pending_config_action: Option<PendingConfigAction>,
    /// Text field for creating a named configuration from the current working values.
    pub new_config_name: String,

    pub tab: SettingsTab,
    pub selected_action: usize,
    /// Illumination tab toggle; when off, zones are black and startup lighting is off.
    pub rgb_enabled: bool,
    /// When false, no zone colour packets are sent, so the mouse keeps its current colours.
    pub live_zone_colors: bool,
    /// Local 0–100 brightness applied to the zone colours before they are sent or drawn.
    pub zone_brightness: u8,
    /// Startup effect selected in the Illumination tab (used when RGB is on).
    pub startup_lighting: DefaultLighting,
    /// Colour used for button-click reactions; independent of the three zone colours.
    pub reactive_color: [u8; 3],
    pub show_info: bool,
    pub show_configs: bool,

    commands: Sender<HidCommand>,
    responses: Receiver<HidResponse>,
}

/// Pure boot-load decision: returns the store to use, sanitization warnings and status text.
/// Kept separate from `new()` so startup behaviour can be tested without an eframe context.
fn load_initial_state(path: &std::path::Path) -> (ConfigStore, Vec<String>, String) {
    match configs::load(path) {
        Ok(Some((store, warnings))) => {
            let status = if warnings.is_empty() {
                format!(
                    "Loaded configuration '{}' from {}",
                    store.active,
                    path.display()
                )
            } else {
                format!(
                    "Loaded '{}' with {} warning(s): {}",
                    store.active,
                    warnings.len(),
                    warnings.first().map_or("", String::as_str)
                )
            };
            (store, warnings, status)
        }
        Ok(None) => (
            ConfigStore::factory(),
            Vec::new(),
            "No saved configuration found — using built-in defaults".to_owned(),
        ),
        Err(error) => (
            ConfigStore::factory(),
            Vec::new(),
            format!("Config file error: {error}; using built-in defaults"),
        ),
    }
}

/// The wired-only model has no battery command in its profile; every other supported PID does.
fn needs_battery_query(pid: u16) -> bool {
    pid != WIRED_ONLY_PID
}

/// Whether a scan reply still belongs to the current scan generation. Kept pure so the
/// stale-reply guard can be tested without an eframe context.
fn accepts_scan_reply(current: u64, reply: u64) -> bool {
    current == reply
}

/// Whether a battery reply still belongs to the current connection and scan. Kept pure so the
/// stale-reply guard can be tested without an eframe context.
fn accepts_battery_reply(connected_pid: Option<u16>, current: u64, reply: u64) -> bool {
    connected_pid.is_some() && current == reply
}

/// Merges a battery reply into the last known state. A late reply with no decoded level must not
/// erase a level that was already read successfully. Kept pure so the rule can be tested without
/// an eframe context.
fn merge_battery_reply(current: Option<&BatteryInfo>, reply: BatteryInfo) -> BatteryInfo {
    match current {
        Some(known) if known.level.is_some() && reply.level.is_none() => known.clone(),
        _ => reply,
    }
}

/// Restores all CPI fields (including stale tail slots) to the factory table.
fn reset_cpi_fields(levels: &mut [u32; 5], count: &mut usize, selected: &mut usize) {
    *levels = crate::core::protocol::DEFAULT_CPI;
    *count = levels.len();
    *selected = crate::core::protocol::DEFAULT_SELECTED_LEVEL;
}

/// Copies a stored block's CPI slots into the fixed-size array used by the UI panels.
fn levels_array(block: &ConfigBlock) -> [u32; 5] {
    let mut levels = crate::core::protocol::DEFAULT_CPI;
    for (slot, value) in levels.iter_mut().zip(block.cpi_levels.iter()) {
        *slot = *value;
    }
    levels
}

/// Values REVERT restores: the stored active configuration, or factory defaults when no stored
/// block matches the active name. Mirrors the boot selection in `new()` so both paths agree.
fn revert_block(configs: &ConfigStore) -> ConfigBlock {
    configs
        .active_block()
        .cloned()
        .unwrap_or_else(|| ConfigBlock::named("Default"))
}

/// Master RGB switch state for the illumination helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RgbState {
    On,
    Off,
}

impl From<bool> for RgbState {
    fn from(enabled: bool) -> Self {
        if enabled { Self::On } else { Self::Off }
    }
}

/// Whether the three zone colours are live on the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneState {
    Live,
    Cleared,
}

impl From<bool> for ZoneState {
    fn from(live: bool) -> Self {
        if live { Self::Live } else { Self::Cleared }
    }
}

/// Colours for the three zone packets. Both switches must be on for the stored colours to be
/// sent; otherwise black is sent explicitly so the mouse's zones are actually cleared.
pub fn effective_zone_colors(
    rgb: RgbState,
    zones: ZoneState,
    brightness: u8,
    colors: [[u8; 3]; 3],
) -> [[u8; 3]; 3] {
    if rgb == RgbState::On && zones == ZoneState::Live {
        colors.map(|rgb| scaled_color(rgb, brightness))
    } else {
        [RGB_OFF; 3]
    }
}

/// Illumination packets written on every SAVE, in protocol order.
///
/// A reactive colour left enabled on the mouse keeps button-click reactions alive, so clear it
/// whenever the selected effect has no reactive flag (or RGB is off). The live rainbow and zone
/// colours are mutually exclusive: the last write wins. When live colours are on they are the live
/// state and no one-shot rainbow is sent; when they are off, re-activate the rainbow after the black
/// colour packets so it applies immediately. `default_lighting` only sets the persisted wake-up mode.
pub fn lighting_packets(
    rgb: RgbState,
    zones: ZoneState,
    brightness: u8,
    colors: [[u8; 3]; 3],
    reactive_color: [u8; 3],
    startup_lighting: DefaultLighting,
    link: Link,
) -> Vec<Vec<u8>> {
    let zone_colors = effective_zone_colors(rgb, zones, brightness, colors);
    let rgb_on = rgb == RgbState::On;
    let zones_live = zones == ZoneState::Live;
    let [rainbow, reactive] = startup_lighting.bytes();
    let mut packets = vec![
        color_packet(0, zone_colors[0], link),
        color_packet(1, zone_colors[1], link),
        color_packet(2, zone_colors[2], link),
        reactive_color_packet((rgb_on && reactive != 0).then_some(reactive_color), link),
    ];
    if rgb_on && !zones_live && rainbow != 0 {
        packets.push(rainbow_effect_packet(link));
    }
    packets.push(default_lighting_packet(
        rgb_on.then_some(startup_lighting),
        link,
    ));
    packets
}

/// Delays between battery queries. The first exceeds one query's worst-case round trip
/// (50 ms write delay + 200 ms read timeout), so retries never overlap in the worker.
pub const BATTERY_RETRY_DELAYS_MS: [u64; 5] = [250, 400, 700, 1200, 2000];

/// State of the bounded battery-query retry schedule started by a successful scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatteryRetry {
    pub pid: u16,
    pub link: Link,
    /// Scan generation these retries belong to.
    pub generation: u64,
    /// Number of retry queries already sent (the initial query is not counted).
    pub retries_sent: usize,
    pub next_at: Instant,
}

/// Delay before the next retry, or `None` once all five retries have been sent.
pub fn battery_retry_delay(retries_sent: usize) -> Option<Duration> {
    BATTERY_RETRY_DELAYS_MS
        .get(retries_sent)
        .map(|ms| Duration::from_millis(*ms))
}

/// Advances the schedule after a retry was sent; `None` means the schedule is exhausted.
pub fn battery_retry_after_send(retry: BatteryRetry, now: Instant) -> Option<BatteryRetry> {
    let delay = battery_retry_delay(retry.retries_sent)?;
    Some(BatteryRetry {
        retries_sent: retry.retries_sent + 1,
        next_at: now + delay,
        ..retry
    })
}

impl Aerox5App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        let (command_tx, command_rx) = channel();
        let (response_tx, response_rx) = channel();
        let hotplug_tx = response_tx.clone();
        thread::spawn(move || hid_worker(command_rx, &response_tx));
        // IOKit notifications run on their own thread and only ask for a rescan;
        // all HID reads stay on the worker thread above.
        crate::core::hotplug::watch(hotplug_tx);

        // Boot load: never writes to disk. Missing/corrupt files fall back to the built-in
        // configuration while the status line explains exactly what happened.
        let config_path = configs::config_path();
        let (configs, config_warnings, config_status) = load_initial_state(&config_path);
        let active = configs
            .active_block()
            .cloned()
            .unwrap_or_else(|| ConfigBlock::named("Default"));
        let initial_pulses = polling_pulse_count(active.polling_rate);

        let mut app = Self {
            status: String::new(),
            events: EventLog::default(),
            show_event_log: false,
            event_log_reset_scroll: false,
            connected_pid: None,
            device_info: None,
            battery: None,
            battery_retry: None,
            battery_generation: 0,
            mouse_texture: load_mouse_texture(&cc.egui_ctx),
            cpi_levels: [0; 5],
            cpi_count: 1,
            selected_level: 0,
            cpi_anim_from: [0, 1, 2, 3, 4],
            cpi_anim_start: None,
            polling_rate: 0,
            // `apply_block` deliberately leaves animation state alone; start settled at boot.
            polling_graph_from: initial_pulses,
            polling_graph_to: initial_pulses,
            polling_graph_start: None,
            sleep_minutes: 0.0,
            dim_seconds: 0,
            color_top: [0; 3],
            color_mid: [0; 3],
            color_bot: [0; 3],
            configs,
            config_path,
            config_warnings,
            pending_config_action: None,
            new_config_name: String::new(),
            tab: SettingsTab::Settings,
            selected_action: 2,
            rgb_enabled: false,
            live_zone_colors: false,
            zone_brightness: 0,
            startup_lighting: DefaultLighting::Rainbow,
            reactive_color: [0; 3],
            show_info: false,
            show_configs: false,
            commands: command_tx,
            responses: response_rx,
        };
        // Boot and confirmed config switches share exactly one initialisation path.
        app.apply_block(&active);
        app.set_status(config_status);
        app.scan_device();
        app
    }

    /// Updates the visible status and appends the same message to the event history.
    fn set_status(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.status.clone_from(&message);
        self.events.record(message);
    }

    /// Replaces the working UI values with a stored configuration block.
    pub fn apply_block(&mut self, block: &ConfigBlock) {
        self.cpi_levels = levels_array(block);
        self.cpi_count = block.cpi_count.clamp(1, 5);
        self.selected_level = block.selected_level.min(self.cpi_count - 1);
        self.polling_rate = block.polling_rate;
        self.sleep_minutes = block.sleep_minutes;
        self.dim_seconds = block.dim_seconds;
        self.rgb_enabled = block.rgb_enabled;
        self.live_zone_colors = block.live_zone_colors;
        self.zone_brightness = block.zone_brightness;
        self.startup_lighting = block.startup_lighting;
        self.reactive_color = block.reactive_color;
        self.color_top = block.color_top;
        self.color_mid = block.color_mid;
        self.color_bot = block.color_bot;
        self.clear_cpi_animation();
    }

    /// Cancels an in-flight swap; used for wholesale changes where a slide would be misleading.
    pub fn clear_cpi_animation(&mut self) {
        self.cpi_anim_from = [0, 1, 2, 3, 4];
        self.cpi_anim_start = None;
    }

    /// Snapshot of the current UI values as a named configuration block.
    pub fn block_from_state(&self, name: &str) -> ConfigBlock {
        let mut block = ConfigBlock::named(name);
        block.cpi_levels = self.cpi_levels.to_vec();
        block.cpi_count = self.cpi_count.clamp(1, 5);
        block.selected_level = self.selected_level.min(block.cpi_count - 1);
        block.polling_rate = self.polling_rate;
        block.sleep_minutes = self.sleep_minutes;
        block.dim_seconds = self.dim_seconds;
        block.rgb_enabled = self.rgb_enabled;
        block.live_zone_colors = self.live_zone_colors;
        block.zone_brightness = self.zone_brightness;
        block.startup_lighting = self.startup_lighting;
        block.reactive_color = self.reactive_color;
        block.color_top = self.color_top;
        block.color_mid = self.color_mid;
        block.color_bot = self.color_bot;
        block
    }

    /// Validates and saves a candidate store via an atomic temp-file rename. `configs::save`
    /// sanitises the store in place, so the caller commits exactly the bytes that were written.
    fn save_store_candidate(&mut self, store: &mut ConfigStore) -> Result<(), String> {
        self.config_warnings = configs::save(&self.config_path, store)?;
        Ok(())
    }

    /// Persists the in-memory store as-is (used after SAVE updates the active block).
    pub fn persist_configs(&mut self) -> Result<(), String> {
        let mut store = self.configs.clone();
        self.save_store_candidate(&mut store)?;
        self.configs = store;
        Ok(())
    }

    /// Applies a confirmed CONFIGS-window action. Persistence failures never masquerade as success.
    pub fn confirm_config_action(&mut self) {
        let Some(action) = self.pending_config_action.take() else {
            return;
        };
        match action {
            PendingConfigAction::Switch(name) => {
                let Some(block) = self
                    .configs
                    .configurations
                    .iter()
                    .find(|block| block.name == name)
                    .cloned()
                else {
                    self.set_status(format!("Configuration '{name}' no longer exists."));
                    return;
                };
                let mut candidate = self.configs.clone();
                if let Err(error) = candidate.switch_active(&name) {
                    self.set_status(error);
                    return;
                }
                self.apply_block(&block);
                self.configs = candidate;
                let message = match self.persist_configs() {
                    Ok(()) => format!("Switched to '{name}' and saved the selection."),
                    Err(error) => {
                        format!(
                            "Switched to '{name}' locally, but the config file was not saved: {error}"
                        )
                    }
                };
                self.set_status(message);
            }
            PendingConfigAction::Delete(name) => {
                let mut candidate = self.configs.clone();
                if let Err(error) = candidate.remove(&name) {
                    self.set_status(error);
                    return;
                }
                match self.save_store_candidate(&mut candidate) {
                    Ok(()) => {
                        self.configs = candidate;
                        self.set_status(format!("Deleted configuration '{name}'."));
                    }
                    Err(error) => {
                        self.set_status(format!("Could not delete '{name}': {error}"));
                    }
                }
            }
        }
    }

    /// Creates a configuration from the current working values and makes it active.
    pub fn create_config(&mut self) {
        let name = self.new_config_name.trim().to_owned();
        if name.is_empty() {
            self.set_status("Enter a name before creating a configuration.");
            return;
        }
        let mut candidate = self.configs.clone();
        let block = self.block_from_state(&name);
        if let Err(error) = candidate.create(&name, block) {
            self.set_status(error);
            return;
        }
        match self.save_store_candidate(&mut candidate) {
            Ok(()) => {
                self.configs = candidate;
                self.new_config_name.clear();
                self.set_status(format!("Created and selected configuration '{name}'."));
            }
            Err(error) => {
                self.set_status(format!("Could not create '{name}': {error}"));
            }
        }
    }

    /// Restores the five CPI levels, count and selected level to factory defaults. Local only:
    /// other settings, the active configuration and the config file on disk are untouched.
    pub fn reset_cpi_to_defaults(&mut self) {
        reset_cpi_fields(
            &mut self.cpi_levels,
            &mut self.cpi_count,
            &mut self.selected_level,
        );
        self.clear_cpi_animation();
        self.set_status("CPI levels reset to defaults (not saved yet).");
    }

    pub fn scan_device(&mut self) {
        self.rescan("Searching for Aerox 5...");
    }

    /// Starts a scan and records exactly one status message describing why.
    fn rescan(&mut self, message: &str) {
        self.set_status(message);
        self.battery_generation = self.battery_generation.wrapping_add(1);
        self.connected_pid = None;
        self.device_info = None;
        self.battery = None;
        self.battery_retry = None;
        let _ = self.commands.send(HidCommand::Scan {
            generation: self.battery_generation,
        });
    }

    pub fn apply_settings(&mut self) {
        // 1. Persist first: build a candidate store from the working UI values and write it.
        //    A failed write leaves `self.configs` untouched, so REVERT still restores the
        //    last saved block. No HID packet is built until the write succeeds.
        let active_name = self.configs.active.clone();
        let mut candidate = self.configs.clone();
        let Some(active) = candidate.active_block_mut() else {
            self.set_status("No active configuration to save.");
            return;
        };
        *active = self.block_from_state(&active_name);
        if let Err(error) = self.save_store_candidate(&mut candidate) {
            self.set_status(format!(
                "Config file could not be saved: {error} — nothing was sent to the mouse."
            ));
            return;
        }
        self.configs = candidate;

        // 2. Send the exact same packets as before to the connected mouse.
        let Some(pid) = self.connected_pid else {
            self.set_status(format!(
                "Saved '{active_name}' to the config file; cannot send: the mouse is not connected."
            ));
            return;
        };
        let link = self
            .device_info
            .as_ref()
            .is_some_and(|info| info.connection == ConnectionKind::WirelessDongle)
            .into();
        let wired_only = pid == WIRED_ONLY_PID;

        let levels = &self.cpi_levels[..self.cpi_count.clamp(1, 5)];
        let selected = self.selected_level.min(levels.len().saturating_sub(1));
        let mut packets = vec![
            sensitivity_packet(levels, selected, link),
            polling_packet(self.polling_rate, PollingEncoding::from(wired_only), link),
            sleep_packet(self.sleep_minutes, link),
            dim_packet(self.dim_seconds, link),
        ];
        packets.extend(lighting_packets(
            RgbState::from(self.rgb_enabled),
            ZoneState::from(self.live_zone_colors),
            self.zone_brightness,
            [self.color_top, self.color_mid, self.color_bot],
            self.reactive_color,
            self.startup_lighting,
            link,
        ));
        packets.push(save_packet(link));

        match self
            .commands
            .send(HidCommand::ApplySettings { pid, link, packets })
        {
            Ok(()) => {
                self.set_status(format!(
                    "Saved '{active_name}' to config; applying settings to mouse..."
                ));
            }
            Err(_) => {
                self.set_status(format!(
                    "Saved '{active_name}' to config, but could not queue the mouse update."
                ));
            }
        }
    }

    pub fn revert(&mut self) {
        // Same rule as boot: restore what is stored for the active configuration, or factory
        // defaults when no stored block matches. REVERT stays local: no file write and no HID
        // send — only unsaved on-screen edits are discarded.
        let using_stored = self.configs.active_block().is_some();
        let block = revert_block(&self.configs);
        let name = block.name.clone();
        self.apply_block(&block);
        let message = if using_stored {
            format!(
                "Local values reverted to saved configuration '{name}'. Nothing was sent to the mouse."
            )
        } else {
            "Local values reverted to defaults. Nothing was sent to the mouse.".to_owned()
        };
        self.set_status(message);
    }

    fn poll_hid_responses(&mut self) {
        while let Ok(response) = self.responses.try_recv() {
            match response {
                HidResponse::Hotplug => self.rescan("Mouse connection changed — scanning..."),
                HidResponse::Scan {
                    info,
                    status,
                    generation,
                } => {
                    // A newer rescan supersedes this reply; do not restore its connection state
                    // or arm a battery query against the previous device.
                    if !accepts_scan_reply(self.battery_generation, generation) {
                        continue;
                    }
                    if let Some(info) = &info
                        && needs_battery_query(info.product_id)
                    {
                        let link = Link::from(info.connection == ConnectionKind::WirelessDongle);
                        let _ = self.commands.send(HidCommand::QueryBattery {
                            pid: info.product_id,
                            link,
                            generation,
                        });
                        self.battery_retry = Some(BatteryRetry {
                            pid: info.product_id,
                            link,
                            generation,
                            retries_sent: 0,
                            next_at: Instant::now()
                                + battery_retry_delay(0)
                                    .expect("the first battery retry delay always exists"),
                        });
                    }
                    self.connected_pid = info.as_ref().map(|info| info.product_id);
                    self.device_info = info;
                    self.set_status(status);
                }
                HidResponse::Battery {
                    info,
                    status,
                    generation,
                } => {
                    // A reply from an earlier scan may arrive after a rescan to the same PID.
                    if accepts_battery_reply(
                        self.connected_pid,
                        self.battery_generation,
                        generation,
                    ) {
                        let has_level =
                            info.as_ref().is_some_and(|battery| battery.level.is_some());
                        if has_level {
                            self.battery_retry = None;
                            self.battery = info;
                            self.set_status(status);
                        } else {
                            if let Some(reply) = info {
                                self.battery =
                                    Some(merge_battery_reply(self.battery.as_ref(), reply));
                            }
                            let message = if self.battery_retry.is_some() {
                                "Waiting for battery data…".to_owned()
                            } else {
                                status
                            };
                            self.set_status(message);
                        }
                    }
                }
                HidResponse::Apply { status } => {
                    self.set_status(status);
                }
            }
        }
    }

    /// Sends the next battery query when its deadline passes. The 200 ms repaint fallback drives
    /// these checks; all delays are >= 250 ms, so retries never crowd the HID worker.
    fn poll_battery_retry(&mut self) {
        let Some(retry) = self.battery_retry else {
            return;
        };
        if Instant::now() < retry.next_at {
            return;
        }
        if self.connected_pid != Some(retry.pid) || retry.generation != self.battery_generation {
            self.battery_retry = None;
            return;
        }
        let _ = self.commands.send(HidCommand::QueryBattery {
            pid: retry.pid,
            link: retry.link,
            generation: retry.generation,
        });
        self.battery_retry = battery_retry_after_send(retry, Instant::now());
    }

    /// Text for the connection/battery line when a device is present. Kept pure so the exact
    /// separator and percentage format can be tested without a GUI context.
    fn connection_summary(info: &DeviceInfoView, battery: Option<&BatteryInfo>) -> String {
        let level = battery
            .and_then(|battery| battery.level)
            .map(|level| format!(" · {level}%"))
            .unwrap_or_default();
        format!(
            "{} · {}{}",
            info.connection.label(),
            info.product_name,
            level
        )
    }

    /// Dot colour and text for the connection/battery line, with or without a device.
    fn connection_status(
        info: Option<&DeviceInfoView>,
        battery: Option<&BatteryInfo>,
    ) -> (Color32, String) {
        match info {
            Some(info) => (C_GREEN, Self::connection_summary(info, battery)),
            None => (C_AMBER, "No Aerox 5 detected".to_owned()),
        }
    }

    /// Right-hand status line. Only the message and EVENT label toggle the history; the empty
    /// space in the allocation stays inert. Returns the allocation rect used to anchor the popup.
    fn event_line(&mut self, ui: &mut egui::Ui, status_w: f32) -> Rect {
        let area = ui.allocate_ui_with_layout(
            Vec2::new(status_w, 16.0),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.set_width(status_w);
                let status_response = ui.add(
                    Label::new(RichText::new(&self.status).size(11.5).color(C_MUTED))
                        .truncate()
                        .sense(Sense::click()),
                );
                ui.add_space(3.0);
                let arrow = if self.show_event_log { "▴" } else { "▾" };
                let label_response = ui.add(
                    Label::new(
                        RichText::new(format!("EVENT {arrow}"))
                            .size(9.5)
                            .strong()
                            .color(C_DIM),
                    )
                    .sense(Sense::click()),
                );
                (status_response, label_response)
            },
        );
        let response = area
            .inner
            .0
            .union(area.inner.1)
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            self.show_event_log = !self.show_event_log;
            if self.show_event_log {
                self.event_log_reset_scroll = true;
            }
        }
        area.response.rect
    }

    fn header(&mut self, ctx: &egui::Context) {
        let (dot_color, summary) =
            Self::connection_status(self.device_info.as_ref(), self.battery.as_ref());
        let config_name = self.configs.active.to_uppercase();
        let spacing = ctx.style().spacing.item_spacing.x;
        let mut event_line = None;

        egui::TopBottomPanel::top("aerox5_header")
            .exact_height(HEADER_H)
            .frame(
                Frame::new()
                    .fill(C_HEADER)
                    .inner_margin(Margin::symmetric(12, 8)),
            )
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    // Row 1 — the configuration name owns everything except the icon column.
                    ui.horizontal_top(|ui| {
                        let icon_w = 22.0;
                        let text_w = (ui.available_width() - icon_w - spacing).max(0.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(text_w, 19.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.set_width(text_w);
                                ui.label(
                                    RichText::new("CONFIGURATION")
                                        .size(10.0)
                                        .strong()
                                        .color(C_MUTED),
                                );
                                ui.label(RichText::new(":").size(10.0).color(C_DIM));
                                ui.add_space(2.0);
                                ui.add(
                                    Label::new(
                                        RichText::new(&config_name)
                                            .size(15.0)
                                            .strong()
                                            .color(C_TEXT),
                                    )
                                    .truncate(),
                                );
                            },
                        );
                        if info_button(ui).clicked() {
                            self.show_info = true;
                        }
                    });

                    // Row 2 — connection/battery on the left, event status on the right. Each
                    // side truncates inside its own area, so neither can squeeze the other out.
                    ui.horizontal_top(|ui| {
                        let available = ui.available_width();
                        let status_w = (available * 0.45).max(180.0);
                        let summary_w = (available - status_w - spacing).max(0.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(summary_w, 16.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.set_width(summary_w);
                                status_dot(ui, dot_color);
                                ui.add(
                                    Label::new(RichText::new(&summary).size(12.0).color(C_TEXT))
                                        .truncate(),
                                );
                            },
                        );
                        event_line = Some(self.event_line(ui, status_w));
                    });
                });
            });

        if let Some(anchor) = event_line {
            self.event_log_popup(ctx, anchor);
        }
    }

    /// Foreground list of all session events, opened by clicking the EVENT line.
    fn event_log_popup(&mut self, ctx: &egui::Context, anchor: Rect) {
        if !self.show_event_log {
            return;
        }
        let screen = ctx.screen_rect();
        let width = anchor
            .width()
            .max(340.0)
            .min((screen.width() - 16.0).max(200.0));
        let pos = egui::pos2((anchor.right() - width).max(8.0), anchor.bottom() + 4.0);
        let available_h = (screen.bottom() - pos.y - 8.0).max(80.0);
        // Every row is one line of 10 pt text followed by 3 pt of space plus the normal
        // vertical item spacing. Size the viewport for 15 such rows, but never past the
        // space left under the header; `auto_shrink` keeps shorter logs short.
        let row_h = ctx.fonts(|fonts| fonts.row_height(&egui::FontId::proportional(10.0)));
        let row_gap = 3.0 + ctx.style().spacing.item_spacing.y;
        let list_h = EVENT_LOG_MAX_VISIBLE_ROWS.mul_add(row_h + row_gap, -row_gap);
        let scroll_h = list_h.min((available_h - 44.0).max(row_h));

        egui::Area::new(egui::Id::new("aerox5_event_log"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                ui.set_width(width);
                // An Area's child UI is capped by the area's previous frame rect. Without
                // this the log could not grow when more events arrived after it was first
                // opened; give it the full space below the header instead.
                ui.set_max_height(available_h);
                Frame::new()
                    .fill(C_PANEL)
                    .stroke(egui::Stroke::new(1.0_f32, C_BORDER))
                    .corner_radius(egui::CornerRadius::same(2))
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(width - 16.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("EVENTS").size(9.5).strong().color(C_MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let close = ui.add(
                                    egui::Button::new(RichText::new("×").size(14.0).color(C_MUTED))
                                        .small(),
                                );
                                if close
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    self.show_event_log = false;
                                }
                            });
                        });
                        ui.add_space(4.0);

                        let mut scroll = egui::ScrollArea::vertical()
                            .id_salt("aerox5_event_log_scroll")
                            .max_height(scroll_h)
                            .auto_shrink([false, true])
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                            );
                        if self.event_log_reset_scroll {
                            scroll = scroll.vertical_scroll_offset(0.0);
                            self.event_log_reset_scroll = false;
                        }
                        scroll.show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            for event in self.events.iter_newest_first() {
                                ui.horizontal_top(|ui| {
                                    ui.label(
                                        RichText::new(&event.timestamp)
                                            .monospace()
                                            .size(9.0)
                                            .color(C_DIM),
                                    );
                                    ui.add(
                                        Label::new(
                                            RichText::new(&event.message).size(10.0).color(C_TEXT),
                                        )
                                        .wrap(),
                                    );
                                });
                                ui.add_space(3.0);
                            }
                        });
                    });
            });
    }

    fn bottom_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("aerox5_bottom")
            .exact_height(BOTTOM_H)
            .frame(
                Frame::new()
                    .fill(C_PAGE)
                    .inner_margin(Margin::symmetric(10, 4)),
            )
            .show(ctx, |ui| {
                ui.columns(3, |columns| {
                    columns[0].with_layout(Layout::left_to_right(Align::Center), |ui| {
                        if configs_button(ui).clicked() {
                            self.show_configs = true;
                        }
                    });
                    // Keep the centre column so CONFIGS and SAVE stay in place.
                    columns[1].with_layout(Layout::left_to_right(Align::Center), |_ui| {});
                    columns[2].with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if painted_button(ui, "SAVE", 64.0, 24.0, C_ORANGE, Color32::WHITE)
                            .clicked()
                        {
                            self.apply_settings();
                        }
                        ui.add_space(6.0);
                        if painted_button(ui, "REVERT", 64.0, 24.0, C_PANEL_HEADER, C_MUTED)
                            .clicked()
                        {
                            self.revert();
                        }
                    });
                });
            });
    }

    fn central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(C_PAGE)
                    .inner_margin(Margin::symmetric(10, 8)),
            )
            .show(ctx, |ui| {
                ui.set_max_width(CONTENT_W);
                ui.horizontal_top(|ui| {
                    // horizontal_top inserts `item_spacing.x` around every add_space item,
                    // so an explicit COL_GAP would render as COL_GAP + spacing. Subtract it
                    // to make each visual gap exactly COL_GAP: the three columns then add up
                    // to CONTENT_W and the window keeps equal 10 pt margins on both sides.
                    let gap = COL_GAP - ui.spacing().item_spacing.x;
                    ui.vertical(|ui| {
                        ui.set_width(LEFT_W);
                        if self.tab == SettingsTab::Illumination {
                            self.illumination_panel(ui);
                        } else {
                            self.actions_panel(ui);
                        }
                        ui.add_space(10.0);
                        if self.tab == SettingsTab::Settings {
                            Self::bluetooth_panel(ui);
                        }
                    });
                    ui.add_space(gap);
                    ui.vertical(|ui| {
                        ui.set_width(CENTER_W);
                        self.center_panel(ui);
                    });
                    ui.add_space(gap);
                    ui.vertical(|ui| {
                        ui.set_width(RIGHT_W);
                        self.right_panel(ui);
                    });
                });
            });
    }

    fn info_window(&mut self, ctx: &egui::Context) {
        if !self.show_info {
            return;
        }
        let mut open = self.show_info;
        egui::Window::new("Product information and help")
            .open(&mut open)
            .default_width(460.0)
            .default_pos(egui::pos2(330.0, 90.0))
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Refresh").clicked() {
                        self.scan_device();
                    }
                    ui.label(RichText::new(&self.status).size(9.0).color(C_MUTED));
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        egui::Grid::new("aerox5_device_info")
                            .num_columns(2)
                            .spacing([14.0, 4.0])
                            .show(ui, |ui| {
                                if let Some(info) = &self.device_info {
                                    info_row(ui, "Connection", info.connection.label());
                                    info_row(ui, "Product", &info.product_name);
                                    info_row(ui, "VID", &format!("0x{:04X}", info.vendor_id));
                                    info_row(ui, "PID", &format!("0x{:04X}", info.product_id));
                                    info_row(
                                        ui,
                                        "HID product",
                                        info.hid_product.as_deref().unwrap_or("Unknown"),
                                    );
                                    info_row(
                                        ui,
                                        "Manufacturer",
                                        info.manufacturer.as_deref().unwrap_or("Unknown"),
                                    );
                                    info_row(
                                        ui,
                                        "Serial",
                                        info.serial.as_deref().unwrap_or("Unknown"),
                                    );
                                    info_row(ui, "HID path", &info.path);
                                    info_row(ui, "Interface", &info.interface_number.to_string());
                                    info_row(ui, "Bus", &info.bus_type);
                                    info_row(
                                        ui,
                                        "Usage page",
                                        &format!("0x{:04X}", info.usage_page),
                                    );
                                    info_row(ui, "Usage", &format!("0x{:04X}", info.usage));
                                    info_row(
                                        ui,
                                        "Release",
                                        &format!("0x{:04X}", info.release_number),
                                    );
                                } else {
                                    info_row(ui, "Device", "No Aerox 5 found");
                                }

                                if let Some(battery) = &self.battery {
                                    info_row(
                                        ui,
                                        "Battery level",
                                        &battery.level.map_or_else(
                                            || "Unknown".to_owned(),
                                            |level| format!("{level} %"),
                                        ),
                                    );
                                    info_row(
                                        ui,
                                        "Charging",
                                        match battery.charging {
                                            Some(true) => "Yes",
                                            Some(false) => "No",
                                            None => "Unknown",
                                        },
                                    );
                                    info_row(
                                        ui,
                                        "Raw battery bytes",
                                        &format!("{:02X} {:02X}", battery.raw[0], battery.raw[1]),
                                    );
                                } else {
                                    info_row(ui, "Battery", "No data");
                                }
                            });
                    });
            });
        self.show_info = open;
    }

    #[expect(
        clippy::too_many_lines,
        reason = "this is one CONFIGS window layout; splitting it would scatter the confirmation state"
    )]
    fn configs_window(&mut self, ctx: &egui::Context) {
        if !self.show_configs {
            return;
        }
        let mut open = self.show_configs;
        let mut close = false;
        egui::Window::new("Configurations")
            .open(&mut open)
            .default_width(400.0)
            .default_pos(egui::pos2(360.0, 100.0))
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(RichText::new("CONFIGURATIONS").size(9.0).color(C_MUTED));
                ui.add_space(4.0);

                // Clone the list/active name so the rest of `self` stays mutable for actions.
                let blocks = self.configs.configurations.clone();
                let active = self.configs.active.clone();
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for (index, block) in blocks.iter().enumerate() {
                            let is_active = block.name == active;
                            let can_delete = block.name != "Default" && !is_active;
                            ui.push_id(index, |ui| {
                                ui.horizontal(|ui| {
                                    let label = if is_active {
                                        format!("{}  (active)", block.name.to_uppercase())
                                    } else {
                                        block.name.to_uppercase()
                                    };
                                    if ui.selectable_label(is_active, label).clicked() && !is_active
                                    {
                                        self.pending_config_action =
                                            Some(PendingConfigAction::Switch(block.name.clone()));
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if can_delete && ui.small_button("Delete").clicked() {
                                            self.pending_config_action = Some(
                                                PendingConfigAction::Delete(block.name.clone()),
                                            );
                                        }
                                    });
                                });
                                let selected_cpi = block
                                    .cpi_levels
                                    .get(block.selected_level)
                                    .copied()
                                    .unwrap_or(0);
                                ui.label(
                                    RichText::new(format!(
                                        "{} level(s) · {selected_cpi} CPI · {} Hz",
                                        block.cpi_count, block.polling_rate
                                    ))
                                    .size(8.0)
                                    .color(C_MUTED),
                                );
                            });
                        }
                    });

                ui.separator();
                if let Some(action) = self.pending_config_action.clone() {
                    match action {
                        PendingConfigAction::Switch(name) => {
                            ui.label(
                                RichText::new(format!(
                                    "Switch to '{name}'? Unsaved edits on screen will be replaced."
                                ))
                                .size(9.0)
                                .color(C_TEXT),
                            );
                            ui.horizontal(|ui| {
                                if ui.button("Switch").clicked() {
                                    self.confirm_config_action();
                                }
                                if ui.button("Cancel").clicked() {
                                    self.pending_config_action = None;
                                }
                            });
                        }
                        PendingConfigAction::Delete(name) => {
                            ui.label(
                                RichText::new(format!("Delete configuration '{name}'?"))
                                    .size(9.0)
                                    .color(C_TEXT),
                            );
                            ui.horizontal(|ui| {
                                if ui.button("Delete").clicked() {
                                    self.confirm_config_action();
                                }
                                if ui.button("Cancel").clicked() {
                                    self.pending_config_action = None;
                                }
                            });
                        }
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("NEW").size(8.5).color(C_MUTED));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_config_name)
                            .hint_text("name")
                            .desired_width(150.0),
                    );
                    if ui.button("Create").clicked() {
                        self.create_config();
                    }
                });
                ui.label(
                    RichText::new(format!("File: {}", self.config_path.display()))
                        .size(7.5)
                        .color(C_MUTED),
                );
                ui.label(
                    RichText::new(if self.config_path.exists() {
                        "Saved on disk"
                    } else {
                        "Not saved yet — built-in values in memory"
                    })
                    .size(7.5)
                    .color(C_DIM),
                );
                if !self.config_warnings.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "Config warnings: {}",
                            self.config_warnings.join("; ")
                        ))
                        .size(7.5)
                        .color(C_AMBER),
                    );
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        self.show_configs = open && !close;
    }
}

impl eframe::App for Aerox5App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_hid_responses();
        self.poll_battery_retry();
        if self
            .cpi_anim_start
            .is_some_and(|start| start.elapsed().as_secs_f32() >= CPI_SWAP_SECONDS)
        {
            self.cpi_anim_start = None;
        }
        if self
            .polling_graph_start
            .is_some_and(|start| start.elapsed().as_secs_f32() >= POLLING_GRAPH_SECONDS)
        {
            self.polling_graph_start = None;
        }
        self.header(ctx);
        self.bottom_bar(ctx);
        self.central_panel(ctx);
        self.info_window(ctx);
        self.configs_window(ctx);
        if self.cpi_anim_start.is_some() || self.polling_graph_start.is_some() {
            // The 200 ms fallback would show these short animations in only a couple of frames.
            ctx.request_repaint();
        }
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aerox5-boot-{tag}-{}-{unique}.json",
            std::process::id()
        ))
    }

    #[test]
    fn reset_cpi_fields_restores_all_slots_after_removal() {
        // Simulate two levels having been removed: stale tail values remain in the array.
        let mut levels = [400, 800, 1200, 3000, 18000];
        let mut count = 3;
        let mut selected = 2;
        reset_cpi_fields(&mut levels, &mut count, &mut selected);
        assert_eq!(levels, [400, 800, 1200, 2400, 3200]);
        assert_eq!(count, 5);
        assert_eq!(selected, 2);
    }

    #[test]
    fn missing_file_starts_from_factory_defaults() {
        let path = temp_path("missing");
        let (store, warnings, status) = load_initial_state(&path);
        assert_eq!(store, ConfigStore::factory());
        assert_eq!(warnings, Vec::<String>::new());
        assert!(status.contains("No saved configuration"));
    }

    #[test]
    fn corrupt_file_falls_back_with_a_clear_error() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "{not json").unwrap();
        let (store, _, status) = load_initial_state(&path);
        assert_eq!(store, ConfigStore::factory());
        assert!(status.contains("Config file error"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn saved_configuration_is_loaded_into_the_state_used_by_the_ui() {
        let path = temp_path("seeded");
        let mut store = ConfigStore::factory();
        store
            .create("Gaming", ConfigBlock::named("Gaming"))
            .unwrap();
        if let Some(block) = store.active_block_mut() {
            block.cpi_levels = vec![800, 1600, 3200];
            block.cpi_count = 3;
            block.selected_level = 1;
            block.polling_rate = 500;
            block.sleep_minutes = 0.0;
            block.dim_seconds = 30;
            block.color_top = [1, 2, 3];
        }
        configs::save(&path, &mut store).unwrap();

        let (loaded, warnings, status) = load_initial_state(&path);
        assert_eq!(warnings, Vec::<String>::new());
        assert!(status.contains("Loaded configuration 'Gaming'"));
        let active = loaded.active_block().unwrap();
        assert_eq!(active.cpi_levels, vec![800, 1600, 3200]);
        assert_eq!(active.cpi_count, 3);
        assert_eq!(active.selected_level, 1);
        assert_eq!(active.polling_rate, 500);
        assert_eq!(active.sleep_minutes, 0.0);
        assert_eq!(active.dim_seconds, 30);
        assert_eq!(active.color_top, [1, 2, 3]);

        // The UI fields are populated from exactly this block (levels_array + apply_block).
        let levels = levels_array(active);
        assert_eq!(levels, [800, 1600, 3200, 2400, 3200]); // stale tail slots keep factory values
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn revert_block_restores_the_stored_active_configuration() {
        let mut store = ConfigStore::factory();
        store
            .create("Gaming", ConfigBlock::named("Gaming"))
            .unwrap();
        let block = store.active_block_mut().unwrap();
        block.cpi_levels = vec![900, 1800, 3600];
        block.cpi_count = 3;
        block.selected_level = 2;
        block.polling_rate = 250;
        block.sleep_minutes = 7.5;
        block.dim_seconds = 45;
        block.color_top = [9, 8, 7];
        block.color_mid = [6, 5, 4];
        block.color_bot = [3, 2, 1];

        let expected = store.active_block().unwrap().clone();
        let reverted = revert_block(&store);
        assert_eq!(reverted, expected);
        assert_eq!(reverted.name, "Gaming");
        assert_eq!(reverted.cpi_levels, vec![900, 1800, 3600]);
        assert_eq!(reverted.polling_rate, 250);
        assert_eq!(reverted.color_bot, [3, 2, 1]);
    }

    #[test]
    fn revert_block_falls_back_to_defaults_without_a_stored_configuration() {
        let mut store = ConfigStore::factory();
        store.active = "Missing".to_owned();
        assert!(store.active_block().is_none());

        let reverted = revert_block(&store);
        assert_eq!(reverted, ConfigBlock::named("Default"));
        assert_eq!(reverted.cpi_levels, crate::core::protocol::DEFAULT_CPI);
    }

    #[test]
    fn zone_colours_are_black_unless_both_switches_are_on() {
        let colors = [[255, 43, 0], [70, 118, 255], [0, 220, 255]];
        assert_eq!(
            effective_zone_colors(RgbState::On, ZoneState::Live, 50, colors),
            [[128, 22, 0], [35, 59, 128], [0, 110, 128]]
        );
        assert_eq!(
            effective_zone_colors(RgbState::On, ZoneState::Live, 0, colors),
            [[0, 0, 0]; 3]
        );
        assert_eq!(
            effective_zone_colors(RgbState::On, ZoneState::Cleared, 100, colors),
            [[0, 0, 0]; 3]
        );
        assert_eq!(
            effective_zone_colors(RgbState::Off, ZoneState::Live, 100, colors),
            [[0, 0, 0]; 3]
        );
    }

    #[test]
    fn lighting_packets_clear_reactive_and_apply_rainbow() {
        let colors = [[255, 43, 0], [70, 118, 255], [0, 220, 255]];
        let reactive = [9, 8, 7];
        let black = vec![
            vec![0x00, 0x21, 0x01, 0x00, 0x00, 0x00, 0x00],
            vec![0x00, 0x21, 0x01, 0x01, 0x00, 0x00, 0x00],
            vec![0x00, 0x21, 0x01, 0x02, 0x00, 0x00, 0x00],
        ];
        let reactive_off = vec![0x00, 0x26, 0x00, 0x00, 0x00, 0x00, 0x00];

        // RGB off: black zones, reactive colour cleared, persisted startup off, no live effect.
        let mut expected = black;
        expected.extend([reactive_off.clone(), vec![0x00, 0x27, 0x00, 0x00]]);
        assert_eq!(
            lighting_packets(
                RgbState::Off,
                ZoneState::Live,
                100,
                colors,
                reactive,
                DefaultLighting::Rainbow,
                Link::Wired
            ),
            expected
        );

        // Rainbow with live colours on: the selected colours are the live state, so no one-shot
        // rainbow may overwrite them.
        assert_eq!(
            lighting_packets(
                RgbState::On,
                ZoneState::Live,
                100,
                colors,
                reactive,
                DefaultLighting::Rainbow,
                Link::Wired
            ),
            vec![
                vec![0x00, 0x21, 0x01, 0x00, 0xFF, 0x2B, 0x00],
                vec![0x00, 0x21, 0x01, 0x01, 0x46, 0x76, 0xFF],
                vec![0x00, 0x21, 0x01, 0x02, 0x00, 0xDC, 0xFF],
                reactive_off.clone(),
                vec![0x00, 0x27, 0x01, 0x00],
            ]
        );

        // Rainbow with live colours off: black zones, reactive cleared, one-shot rainbow re-applied.
        let expected = vec![
            vec![0x00, 0x21, 0x01, 0x00, 0x00, 0x00, 0x00],
            vec![0x00, 0x21, 0x01, 0x01, 0x00, 0x00, 0x00],
            vec![0x00, 0x21, 0x01, 0x02, 0x00, 0x00, 0x00],
            reactive_off,
            vec![0x00, 0x22, 0xFF],
            vec![0x00, 0x27, 0x01, 0x00],
        ];
        assert_eq!(
            lighting_packets(
                RgbState::On,
                ZoneState::Cleared,
                100,
                colors,
                reactive,
                DefaultLighting::Rainbow,
                Link::Wired
            ),
            expected
        );

        // Wireless, live colours off: still clear reactive and activate the live rainbow.
        let expected = vec![
            vec![0x00, 0x61, 0x01, 0x00, 0x00, 0x00, 0x00],
            vec![0x00, 0x61, 0x01, 0x01, 0x00, 0x00, 0x00],
            vec![0x00, 0x61, 0x01, 0x02, 0x00, 0x00, 0x00],
            vec![0x00, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00],
            vec![0x00, 0x62, 0xFF],
            vec![0x00, 0x67, 0x01, 0x00],
        ];
        assert_eq!(
            lighting_packets(
                RgbState::On,
                ZoneState::Cleared,
                100,
                colors,
                reactive,
                DefaultLighting::Rainbow,
                Link::Dongle
            ),
            expected
        );
    }

    #[test]
    fn lighting_packets_use_the_selected_reactive_colour() {
        let colors = [[255, 43, 0], [70, 118, 255], [0, 220, 255]];
        let reactive = [9, 8, 7];

        // Reactive modes keep a reaction colour; with live colours on they never write the
        // one-shot rainbow either.
        assert_eq!(
            lighting_packets(
                RgbState::On,
                ZoneState::Live,
                100,
                colors,
                reactive,
                DefaultLighting::Reactive,
                Link::Wired
            ),
            vec![
                vec![0x00, 0x21, 0x01, 0x00, 0xFF, 0x2B, 0x00],
                vec![0x00, 0x21, 0x01, 0x01, 0x46, 0x76, 0xFF],
                vec![0x00, 0x21, 0x01, 0x02, 0x00, 0xDC, 0xFF],
                vec![0x00, 0x26, 0x01, 0x00, 0x09, 0x08, 0x07],
                vec![0x00, 0x27, 0x00, 0x01],
            ]
        );
        assert_eq!(
            lighting_packets(
                RgbState::On,
                ZoneState::Live,
                100,
                colors,
                reactive,
                DefaultLighting::ReactiveRainbow,
                Link::Dongle
            ),
            vec![
                vec![0x00, 0x61, 0x01, 0x00, 0xFF, 0x2B, 0x00],
                vec![0x00, 0x61, 0x01, 0x01, 0x46, 0x76, 0xFF],
                vec![0x00, 0x61, 0x01, 0x02, 0x00, 0xDC, 0xFF],
                vec![0x00, 0x66, 0x01, 0x00, 0x09, 0x08, 0x07],
                vec![0x00, 0x67, 0x01, 0x01],
            ]
        );

        // Reactive-rainbow with live colours off still activates the live rainbow.
        assert_eq!(
            lighting_packets(
                RgbState::On,
                ZoneState::Cleared,
                100,
                colors,
                reactive,
                DefaultLighting::ReactiveRainbow,
                Link::Wired
            ),
            vec![
                vec![0x00, 0x21, 0x01, 0x00, 0x00, 0x00, 0x00],
                vec![0x00, 0x21, 0x01, 0x01, 0x00, 0x00, 0x00],
                vec![0x00, 0x21, 0x01, 0x02, 0x00, 0x00, 0x00],
                vec![0x00, 0x26, 0x01, 0x00, 0x09, 0x08, 0x07],
                vec![0x00, 0x22, 0xFF],
                vec![0x00, 0x27, 0x01, 0x01],
            ]
        );
    }

    #[test]
    fn battery_retry_delays_cover_five_backoff_steps() {
        assert_eq!(battery_retry_delay(0), Some(Duration::from_millis(250)));
        assert_eq!(battery_retry_delay(1), Some(Duration::from_millis(400)));
        assert_eq!(battery_retry_delay(2), Some(Duration::from_millis(700)));
        assert_eq!(battery_retry_delay(3), Some(Duration::from_millis(1200)));
        assert_eq!(battery_retry_delay(4), Some(Duration::from_millis(2000)));
        assert_eq!(battery_retry_delay(5), None);
    }

    #[test]
    fn battery_retry_advances_and_exhausts_after_five_retries() {
        let now = Instant::now();
        let mut retry = BatteryRetry {
            pid: 0x1852,
            link: Link::Dongle,
            generation: 7,
            retries_sent: 0,
            next_at: now,
        };
        for (sent, delay_ms) in [250_u64, 400, 700, 1200, 2000].into_iter().enumerate() {
            let next = battery_retry_after_send(retry, now).unwrap();
            assert_eq!(next.retries_sent, sent + 1);
            assert_eq!(next.next_at, now + Duration::from_millis(delay_ms));
            assert_eq!(next.pid, 0x1852);
            assert_eq!(next.link, Link::Dongle);
            assert_eq!(next.generation, 7, "retries stay in their scan generation");
            retry = next;
        }
        assert!(battery_retry_after_send(retry, now).is_none());
    }

    fn sample_info(connection: ConnectionKind) -> DeviceInfoView {
        DeviceInfoView {
            vendor_id: 0x1038,
            product_id: 0x1852,
            product_name: "Aerox 5 Wireless".to_owned(),
            hid_product: None,
            manufacturer: None,
            serial: None,
            release_number: 0,
            usage_page: 0,
            usage: 0,
            interface_number: 3,
            bus_type: "Usb".to_owned(),
            path: String::new(),
            connection,
        }
    }

    fn sample_battery(level: Option<u8>) -> BatteryInfo {
        BatteryInfo {
            level,
            charging: None,
            raw: [0, 0],
        }
    }

    #[test]
    fn connection_summary_includes_connection_product_and_battery() {
        let dongle = sample_info(ConnectionKind::WirelessDongle);
        assert_eq!(
            Aerox5App::connection_summary(&dongle, Some(&sample_battery(Some(87)))),
            "2.4 GHz dongle · Aerox 5 Wireless · 87%"
        );
        let wired = sample_info(ConnectionKind::UsbC);
        assert_eq!(
            Aerox5App::connection_summary(&wired, None),
            "USB-C (cable) · Aerox 5 Wireless"
        );
        assert_eq!(
            Aerox5App::connection_summary(&wired, Some(&sample_battery(None))),
            "USB-C (cable) · Aerox 5 Wireless"
        );
    }

    #[test]
    fn connection_status_falls_back_to_no_device() {
        let (color, text) = Aerox5App::connection_status(None, None);
        assert_eq!(color, C_AMBER);
        assert_eq!(text, "No Aerox 5 detected");

        let info = sample_info(ConnectionKind::WirelessDongle);
        let (color, text) =
            Aerox5App::connection_status(Some(&info), Some(&sample_battery(Some(42))));
        assert_eq!(color, C_GREEN);
        assert_eq!(text, "2.4 GHz dongle · Aerox 5 Wireless · 42%");
    }

    #[test]
    fn stale_scan_replies_are_ignored() {
        assert!(accepts_scan_reply(7, 7));
        assert!(
            !accepts_scan_reply(8, 7),
            "a reply from an earlier rescan must be dropped"
        );
    }

    #[test]
    fn a_late_no_level_reply_keeps_a_known_level() {
        let known = sample_battery(Some(42));
        let late = merge_battery_reply(Some(&known), sample_battery(None));
        assert_eq!(late.level, Some(42));
        assert_eq!(
            merge_battery_reply(None, sample_battery(Some(9))).level,
            Some(9)
        );
        assert_eq!(
            merge_battery_reply(Some(&sample_battery(None)), sample_battery(None)).level,
            None
        );
    }

    #[test]
    fn stale_battery_replies_are_ignored() {
        assert!(accepts_battery_reply(Some(0x1852), 7, 7));
        assert!(
            !accepts_battery_reply(None, 7, 7),
            "a reply after disconnect must be dropped"
        );
        assert!(
            !accepts_battery_reply(Some(0x1852), 8, 7),
            "a reply from an earlier scan must be dropped"
        );
    }

    #[test]
    fn wired_only_pid_is_not_queried_for_battery() {
        assert!(!needs_battery_query(WIRED_ONLY_PID));
        assert!(needs_battery_query(0x1852), "2.4 GHz dongle has a battery");
        assert!(
            needs_battery_query(0x1854),
            "wireless USB-C model has a battery"
        );
    }

    #[test]
    fn event_log_lists_newest_first_with_timestamps() {
        let mut log = EventLog::default();
        log.record("first");
        log.record("second");
        let entries: Vec<_> = log.iter_newest_first().collect();
        assert_eq!(entries.len(), 2, "both recorded events must be kept");
        assert_eq!(entries[0].message, "second", "newest event must come first");
        assert_eq!(entries[1].message, "first", "oldest event must come last");
        for entry in entries {
            assert_eq!(entry.timestamp.len(), 8, "timestamp must be HH:MM:SS");
            assert_eq!(entry.timestamp.as_bytes()[2], b':', "hour/minute separator");
            assert_eq!(
                entry.timestamp.as_bytes()[5],
                b':',
                "minute/second separator"
            );
        }
    }

    #[test]
    fn event_log_drops_the_oldest_entries_when_full() {
        let mut log = EventLog::default();
        for index in 0..EVENT_LOG_MAX_ENTRIES + 2 {
            log.record(format!("event {index}"));
        }
        let entries: Vec<_> = log.iter_newest_first().collect();
        assert_eq!(entries.len(), EVENT_LOG_MAX_ENTRIES);
        assert_eq!(
            entries[0].message,
            format!("event {}", EVENT_LOG_MAX_ENTRIES + 1),
            "the newest event must be kept"
        );
        assert_eq!(entries[EVENT_LOG_MAX_ENTRIES - 1].message, "event 2");
    }
}
