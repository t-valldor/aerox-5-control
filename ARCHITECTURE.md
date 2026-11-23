# ARCHITECTURE.md — Aerox 5 Control

**This is the authoritative description of this application.** Agents and humans should read this
file before making changes and must keep it up to date when behaviour, structure, integrations,
configuration or protocol details change (`AGENTS.md` requires it for every change).

The application is a macOS-oriented desktop GUI for configuring a SteelSeries Aerox 5 mouse. It is
a **binary crate** (`aerox5-gui`, edition 2024, MSRV 1.99, `rust-toolchain.toml` pins stable) built
with `eframe`/`egui`; it talks to the mouse over HID/IOKit and stores settings in a local JSON file.
All user-facing copy (the UI and `README.md`) is in English. The low-level protocol background
lives in `docs/com-spec.md`; user documentation lives in `README.md`; this file describes how the
code is put together.

---

## 1. Repository and source layout

```text
.
├── AGENTS.md               # working agreements, task-document workflow, this file's maintenance rule
├── ARCHITECTURE.md         # this document
├── README.md               # user-facing documentation (English)
├── Cargo.toml              # dependencies, edition/MSRV, BeautifulRust lints
├── clippy.toml             # lint configuration (bool params = 0; unwrap/expect allowed in tests)
├── rust-toolchain.toml     # stable toolchain
├── scripts/
│   └── build-app.sh        # release build + macOS .app bundle, icon, signing, install/open
├── assets/
│   ├── AppIcon.png         # 1024×1024 app/runtime icon
│   └── Aerox5WirelessTop.png # top-down mouse illustration (896×1195)
├── docs/
│   ├── com-spec.md         # reverse-engineered HID protocol reference
│   └── steelseries-gg-cpi-analysis.md # static analysis of SteelSeries GG's CPI handling
├── work/
│   ├── TASKS.md            # completed-task registry
│   └── tasks/NNN.md        # task specifications and reports
└── src/
    ├── main.rs             # process entry point, eframe setup, icon embedding
    ├── core/               # non-UI logic (must not import ui)
    │   ├── mod.rs
    │   ├── configs.rs      # JSON configuration store
    │   ├── device.rs       # HID discovery, device info, HID worker thread
    │   ├── hotplug.rs      # macOS IOKit arrival/removal listener + debounce
    │   └── protocol.rs     # pure packet-encoding layer
    └── ui/                 # egui UI (may import core)
        ├── mod.rs
        ├── app.rs          # Aerox5App state, eframe::App, header/windows, HID plumbing
        ├── mouse.rs        # mouse texture, glow and B1–B9 callouts
        ├── panels.rs       # actions, illumination and settings panels, animations
        ├── theme.rs        # palette, layout constants, egui style
        └── widgets.rs      # reusable widgets and animation helpers
```

### 1.1 Module dependency direction

| Module | Depends on | Rule |
|---|---|---|
| `core::protocol` | nothing in-crate | pure byte encoding; no HID, no UI |
| `core::configs` | `core::protocol` (defaults) | pure file/JSON logic |
| `core::device` | `core::protocol` (report ID conventions) | owns all HID calls |
| `core::hotplug` | `core::device` (PIDs, `HidResponse`) | macOS-only implementation, no-op elsewhere |
| `ui::*` | any `core::*` and other `ui::*` | UI may read core; **core never imports ui** |

`src/main.rs` declares only `mod core; mod ui;` and imports `ui::app::Aerox5App` and
`ui::theme::{WINDOW_H, WINDOW_W}`. Do not reintroduce flat top-level modules.

### 1.2 Dependencies and platform gating

- Cross-platform: `chrono` (event timestamps), `eframe 0.31` (GUI), `hidapi 2.6` (HID), `serde`
  with derive and `serde_json` (config).
- macOS only: `core-foundation-sys`, `io-kit-sys`, `libc` — used exclusively by `core::hotplug`
  behind `#[cfg(target_os = "macos")]`. On other platforms `hotplug::watch` is a documented no-op
  and connection changes are detected only by the manual **Refresh** button.
- Lints are a BeautifulRust drop-in in `Cargo.toml` plus `clippy.toml`. CI check:
  `cargo clippy --all-targets --all-features -- -D warnings`. Tests allow `unwrap`/`expect`.
- Test suite: `cargo test` (70 tests at the time of writing).

---

## 2. Runtime model: threads, channels and message flow

There are four execution contexts; **all HID calls happen on one dedicated worker thread**.

| Context | Where started | Name | Owns / lifetime |
|---|---|---|---|
| GUI / eframe event thread | `main()` → `eframe::run_native` | eframe default | `Aerox5App`, both channel ends; process lifetime |
| HID worker | `Aerox5App::new` → `thread::spawn(move || hid_worker(…))` | unnamed | `Receiver<HidCommand>`, borrows `Sender<HidResponse>`; exits when the command channel closes or response send fails |
| IOKit watcher | `Aerox5App::new` → `core::hotplug::watch` | `aerox5-hotplug` | `IONotificationPortRef`, two `io_iterator_t`s, boxed signal sender; `CFRunLoopRun()` until process exit |
| Hotplug coalescer | spawned by `hotplug::run` | `aerox5-hotplug-coalescer` | internal `Receiver<HotplugSignal>` + cloned `Sender<HidResponse>`; exits when either channel closes |

### 2.1 Channels

| Channel | Direction | Ends | Purpose |
|---|---|---|---|
| `mpsc::channel<HidCommand>` | UI → HID worker | `Aerox5App::commands` / worker receiver | scan, battery query, apply settings |
| `mpsc::channel<HidResponse>` | worker and coalescer → UI | `Aerox5App::responses` / worker sender + hotplug clone | `Hotplug`, `Scan`, `Battery`, `Apply` |
| internal `mpsc::channel<HotplugSignal>` | IOKit callback → coalescer | boxed sender in IOKit `refcon` / coalescer receiver | collapse per-interface notification bursts |

### 2.2 Startup sequence (`Aerox5App::new`)

1. `configure_style()` applies the egui theme.
2. Create the command and response channels; clone the response sender for hotplug.
3. Spawn the HID worker; call `hotplug::watch(cloned_sender)` once.
4. Resolve the config path and load it **read-only** (`load_initial_state`); never write at boot.
5. Initialise all UI state via `apply_block(&active)` (the same path a confirmed config switch
   uses), with animations at rest.
6. `set_status(config_status)` (also records the first event) and `scan_device()`.
7. `scan_device()` → `rescan("Searching for Aerox 5...")` bumps `battery_generation`, clears
   connection/battery state and queues `HidCommand::Scan { generation }` with that generation.

### 2.3 Per-frame order (`impl eframe::App for Aerox5App::update`)

1. `poll_hid_responses()` — drain every queued `HidResponse`.
2. `poll_battery_retry()` — send the next battery retry when its deadline passes.
3. Expire the CPI slide animation and the polling-graph animation.
4. Draw header, bottom bar, central panel, info window and configs window.
5. `ctx.request_repaint()` while an animation is active; always
   `ctx.request_repaint_after(200 ms)` as the fallback cadence (also drives battery retries).

### 2.4 Response handling (`poll_hid_responses`)

- `Hotplug` → `rescan("Mouse connection changed — scanning...")`.
- `Scan { info, status, generation }` → ignored when the reply's generation differs from the
  current `battery_generation`, so an earlier rescan cannot restore stale connection state or arm a
  query for the previous device. Otherwise, if a device is present and it is not the wired-only PID,
  queue an immediate `QueryBattery` carrying the reply's generation and `Link` and arm the retry
  schedule; then store `connected_pid`/`device_info` and set the status.
- `Battery { info, status, generation }` → ignored when no device is connected or when the reply's
  generation differs from the current scan generation (`battery_generation` is incremented by every
  `rescan()`), so a reply from an earlier scan cannot overwrite fresh state. A valid level clears
  the retry schedule; a later reply with no level keeps the last known level instead of erasing it,
  and the UI shows `Waiting for battery data…` while retries are active.
- `Apply { status }` → set the status (the earlier "applying settings" message is replaced).

`set_status()` always writes the visible status and appends a timestamped `EventEntry` to the
session event log.

---

## 3. HID integration — `src/core/device.rs`

### 3.1 Identity and connection types

| Symbol | Value / meaning |
|---|---|
| `STEELSERIES_VID` | `0x1038` |
| `USB_PIDS` | `0x1850` (wired-only Aerox 5), `0x1854`, `0x185E`, `0x1862` (USB-C editions) |
| `DONGLE_PIDS` | `0x1852`, `0x185C`, `0x1860` (2.4 GHz dongle editions) |
| `AEROX5_PIDS` | union of the above; the only PID allow-list used by `find_aerox5` |
| `CONFIG_INTERFACE` | `3` — the HID interface used for battery and settings |
| `WIRED_ONLY_PID` | `0x1850` — excluded from battery queries |

`ConnectionKind::{UsbC, WirelessDongle, Unknown}` is derived from the PID alone (`label()` gives
`USB-C (cable)`, `2.4 GHz dongle`, `Unknown`). `product_name_for_pid()` maps each PID to the
product name shown in the UI and status line. `find_aerox5()` takes the **first** HID entry with
the SteelSeries VID and an allowed PID, without checking the interface; battery and apply later
require `interface_number() == CONFIG_INTERFACE`.

`DeviceInfoView` holds every HID-enumeration field readable without opening the device (VID, PID,
product name, HID product string, manufacturer, serial, release number, usage page/usage,
interface number, bus type, hex-encoded path and connection kind). `BatteryInfo` holds
`level: Option<u8>`, `charging: Option<bool>` and the two raw bytes.

### 3.2 Command / response protocol

`HidCommand`: `Scan { generation }`, `QueryBattery { pid, link: Link, generation }`,
`ApplySettings { pid, link: Link, packets: Vec<Vec<u8>> }`.
`HidResponse`: `Hotplug`, `Scan { info, status, generation }`,
`Battery { info, status, generation }`, `Apply { status }`.
Each command produces exactly one response; the generation carried by `Scan`/`QueryBattery` and
echoed in the reply ties it to the scan that requested it, and `Link` (not a bare bool) selects the
wired or dongle transport.

`hid_worker()` is a blocking `for command in commands` loop. A fresh `HidApi::new()` is created
**per command**, always on the same thread, because on macOS `hid_enumerate()` pumps the current
run loop (which would re-enter winit's event handler and abort) and hidapi binds its `IOHIDManager`
to the thread that uses it. HID work must never move to the GUI thread.

#### Scan

Enumerates HID devices and returns the first VID+PID match; the worker echoes the command's
generation so the UI can drop replies from a superseded scan. Status strings:
`Connected: {product} (PID 0x{:04X}, {connection})`,
`No Aerox 5 found. Check cable/dongle.`, or `HID error: {e}`.

#### Battery query

- Finds VID + requested PID + interface 3 and opens it.
- Writes `[0x00, 0x92]` (wired) or `[0x00, 0xD2]` (dongle: `0x92 | 0x40`).
- Sleeps **50 ms** after the write, then reads 2 bytes with a **200 ms** timeout.
- Decodes `charging = byte1 & 0x80 != 0` and `level = ((byte1 & 0x7F) - 1) * 5`, accepting only
  `0..=100`; out-of-range yields `level: None` (this guards the historical "630 %" bug).
- Errors are reported as status strings: HID/open/write/read/incomplete-response messages;
  an undecodable level says `Could not read the battery level. Is the mouse turned on?`

`needs_battery_query(pid)` is `pid != WIRED_ONLY_PID`. Retry schedule (UI side):
`BATTERY_RETRY_DELAYS_MS = [250, 400, 700, 1200, 2000]`, then stop; `BatteryRetry` tracks pid,
`Link`, scan generation, retries sent and next deadline. `poll_battery_retry()` cancels the
schedule if the connected PID or the scan generation changes; retries reuse the initial query's
generation and link.

#### Apply settings

- Same VID/PID/interface-3 lookup and open.
- Writes each packet in order; a write error at packet *n* stores
  `Write error on packet n of total: …` and **stops** — application is not atomic and there is no
  rollback.
- After every successful write it sleeps 50 ms. On a dongle link it additionally performs a
  **64-byte readback with a 200 ms timeout whose contents are discarded** (wireless profiles do
  this; wired ones do not).
- Success status: `Settings sent to the mouse.`

### 3.3 Error-path summary

| Failure | Result |
|---|---|
| `HidApi::new()` fails | `HID error: …` status, no device info |
| No VID/PID match | `No Aerox 5 found. Check cable/dongle.` |
| Interface 3 missing / cannot open | `Could not open the configuration interface (interface 3).` |
| Battery write/read/incomplete data | corresponding status; `BatteryInfo` may carry `None` level |
| Apply write fails at packet n | remaining packets skipped; earlier packets already sent |
| Response channel closed | worker exits; UI keeps last known state |

---

## 4. Hotplug / IOKit integration — `src/core/hotplug.rs`

The listener is registered **once** at startup and lives for the process lifetime; a
disconnect/connect does **not** register a new listener. The callback only signals "something
changed" — the authoritative connection state is still the PID-filtered HID scan.

### 4.1 Port and matching dictionary

`iokit_port()` resolves `kIOMainPortDefault` dynamically with `libc::dlsym(RTLD_DEFAULT, …)` and
falls back to `kIOMasterPortDefault`. `matching_dictionary(vid)` uses
`IOServiceMatching(kIOHIDDeviceKey)` plus `IOHIDVendorIDKey` as a `CFNumber(STEELSERIES_VID)`.
A **fresh dictionary is created for each registration** because
`IOServiceAddMatchingNotification` consumes one reference (including on failure — do not
release it again).

### 4.2 Registrations, re-arming and teardown

`Watcher::new` creates an `IONotificationPortRef`, adds its run-loop source to the watcher
thread's `CFRunLoopGetCurrent()` in default mode, and registers:

- `kIOFirstMatchNotification` → `on_device_change`, iterator stored as `watcher.matched`;
- `kIOTerminatedNotification` → same callback, iterator stored as `watcher.terminated`.

Both iterators are **drained** (`IOIteratorNext` + `IOObjectRelease`) at registration time; the
callback drains the iterator it receives on every event, which is what re-arms the notification
for the next event. Failing to drain stops future notifications. The boxed `Sender<HotplugSignal>`
passed as `refcon` is owned by `Watcher` and freed last in `Drop`, after the iterators and port are
released, so no callback can observe a dangling pointer. If any IOKit setup step fails,
`Watcher::new` returns `None`, the watcher thread exits and hotplug detection is unavailable;
manual **Refresh** still works.

### 4.3 Burst coalescing

One physical plug/unplug produces one notification **per matching HID interface** (about five for
this mouse), because the match filters on VID only and therefore also matches other SteelSeries
devices. `HOTPLUG_DEBOUNCE = 150 ms` and `coalesce()` implement a **trailing-edge debounce**: the
first signal starts the wait, each further signal restarts it, and once the burst subsides exactly
one `HidResponse::Hotplug` is sent to the GUI. One physical event therefore causes one rescan,
one HID enumeration, one battery query and one pair of event-log entries. The trade-off is up to
150 ms extra latency before the UI updates; a continuous signal stream can postpone the rescan.

On non-macOS platforms `watch()` does nothing.

### 4.4 Wireless caveat

IOKit observes the USB receiver, not the 2.4 GHz radio link. Turning the mouse off while the dongle
stays plugged in does not terminate a HID service, so the app still reports it as connected.

---

## 5. Protocol integration — `src/core/protocol.rs`

This module is a **pure byte encoder**: no HID I/O, no UI, no file access. See `docs/com-spec.md`
for the reverse-engineered background and test vectors.

### 5.1 Framing rules and constants

- Every packet begins with report ID `REPORT_ID = 0x00` and targets interface 3.
- Dongle variants set bit `0x40` in the **command byte only**; the report ID and payload are
  unchanged (wired `0x2D` → dongle `0x6D`, etc.).
- Multi-byte values are little-endian (`push_u24_le`).
- Defaults: `DEFAULT_CPI = [400, 800, 1200, 2400, 3200]`, `DEFAULT_SELECTED_LEVEL = 2`,
  `DEFAULT_POLLING = 1000`, `DEFAULT_SLEEP_MINUTES = 5.0`, `DEFAULT_DIM_SECONDS = 60`,
  `POLLING_RATES = [125, 250, 500, 1000]`.

### 5.2 Enums and CPI encoding

- `DefaultLighting::{Reactive, Rainbow (default), ReactiveRainbow}`, kebab-case in JSON; there is
  deliberately **no `Off` variant** — RGB off is represented by `None` and sent as `00 00`.
  `bytes()` → Reactive `00 01`, Rainbow `01 00`, ReactiveRainbow `01 01`; `has_reactive()` checks
  the reactive flag byte.
- `Link::{Wired, Dongle}` (`From<bool>`: `true` → Dongle);
  `PollingEncoding::{Standard, WiredOnly}` (wired-only PID `0x1850` uses a different code table).
- `TRUEMOVE_AIR` is a 180-entry non-linear CPI lookup table (100 → `0x00`, 400 → `0x04`,
  1200 → `0x0D`, 3200 → `0x26`, 18000 → `0xD6`, …). `nearest_cpi`, `dpi_byte` and `snap_cpi`
  clamp to `100..=18000` and snap to the nearest table key (ties to the lower value).

### 5.3 Packet builders

Dongle form = wired command byte OR `0x40`; all values shown in wired form.

| Builder | Wire format | Notes |
|---|---|---|
| `sensitivity_packet(levels, selected, link)` | `00 2D count selected DPI…` | max 5 levels, each encoded via `dpi_byte` |
| `polling_packet(rate, encoding, link)` | `00 2B code` | Standard: 125→03, 250→02, 500→01, 1000→00; WiredOnly: 04/03/02/01 |
| `sleep_packet(minutes, link)` | `00 29 <u24 ms>` | `round(minutes × 60000)`; the builder clamps finite minutes to `0..=20` and maps non-finite ones to 0, and the config loader clamps as well |
| `dim_packet(seconds, link)` | `00 23 0F 01 00 00 <u24 ms>` | `seconds × 1000`; builder clamps seconds to `0..=16_777` so the LE24 field cannot truncate, config clamps to `0..=1200` |
| `color_packet(zone, rgb, link)` | `00 21 01 zone R G B` | zones 0/1/2 = top/mid/bottom |
| `reactive_color_packet(color, link)` | `00 26 00 00 00 00 00` or `00 26 01 00 R G B` | must always be written: a stale enabled colour keeps reactions alive |
| `rainbow_effect_packet(link)` | `00 22 FF` | one-shot, volatile; a later colour write clears it |
| `default_lighting_packet(mode, link)` | `00 27 flag1 flag2` / `00 27 00 00` for `None` | persisted wake-up effect; colour writes clear it, so it is re-sent |
| `save_packet(link)` | `00 11 00` | commits settings to onboard memory |
| battery query (in `core::device`) | `00 92` / `00 D2` | 2-byte response; decoding described in §3.2 |

`scaled_color(rgb, percent)` scales each channel with `(channel × percent + 50) / 100`; there is
no wireless brightness command, so brightness is applied to RGB values before sending.

### 5.4 Apply order

`Aerox5App::apply_settings()` persists JSON first and aborts before building any packet if the
write fails. Then, when a device is connected, it queues in this order: sensitivity, polling,
sleep, dim, the lighting packets, and finally `save_packet`. `lighting_packets()` emits three zone
colour packets, the reactive-colour packet (cleared when RGB is off or the effect has no reactive
flag), an optional one-shot rainbow (only when RGB is on, live zone colours are off and the effect
has a rainbow flag — otherwise it would overwrite zone colours), and the persisted startup-lighting
packet (`None` when RGB is off). `effective_zone_colors()` sends scaled stored colours only when
both RGB and live-zone switches are on; otherwise it sends explicit black.

---

## 6. Configuration integration — `src/core/configs.rs`

All settings that SAVE sends are stored in **one local version-1 JSON file**. Named profiles exist
only in this file; the mouse protocol has no profile-management command. Boot is read-only; writes
happen only on explicit SAVE or a confirmed switch/create/delete in the CONFIGS window.

### 6.1 File path resolution (`config_path`)

1. `AEROX5_GUI_CONFIG` environment variable wins (used by tests and manual verification).
2. Otherwise `$HOME/Library/Application Support/Aerox 5 Control/config.json` on macOS;
   `$HOME/.aerox5-gui/config.json` elsewhere.
3. If `HOME` is unset: relative `.aerox5-gui/config.json`.

Path resolution, boot and load never create directories or files.

### 6.2 JSON schema

Top level: `version: u32` (must be `1`), `active: String` (name of the active block),
`configurations: Vec<ConfigBlock>` (non-empty). `ConfigBlock` fields:

| Key | Type | Factory default | Validation / sanitisation |
|---|---|---|---|
| `name` | string | `"Default"` | trimmed; empty/duplicate is a hard error |
| `cpi_levels` | u32 array | `[400,800,1200,2400,3200]` | empty → factory + warning; >5 → truncated; each clamped to `100..=18000` |
| `cpi_count` | usize | `5` | clamped to `1..=cpi_levels.len()`; tail entries are stale leftovers |
| `selected_level` | usize | `2` | clamped to `cpi_count - 1` |
| `polling_rate` | u32 | `1000` | must be in `POLLING_RATES` else reset + warning |
| `sleep_minutes` | f32 | `5.0` | finite, `>= 0`, clamped to `<= 20` |
| `dim_seconds` | u32 | `60` | clamped to `<= 1200` |
| `rgb_enabled` | bool | `true` | legacy default when key absent |
| `live_zone_colors` | bool | `true` | legacy default when key absent |
| `zone_brightness` | u8 | `100` | clamped to `<= 100` |
| `startup_lighting` | enum | `"rainbow"` | `reactive` / `rainbow` / `reactive-rainbow` |
| `reactive_color` | RGB triple | `[255,255,255]` | independent of zone colours |
| `color_top` / `color_mid` / `color_bot` | RGB triples | `[255,43,214]` / `[70,118,255]` / `[0,220,255]` | no numeric constraint |

Container-level `#[serde(default)]` plus field-level legacy defaults back-fill missing keys in old
files. There is no version migration; any version other than `1` is a hard error.

### 6.3 Load / validate / save

- `parse(text)` deserialises and validates; `load(path)` maps `NotFound` to `Ok(None)`, I/O errors
  to `could not read {path}: …`, and parse/validate errors to `could not parse {path}: …`.
- `ConfigStore::validate()` returns hard errors (wrong version, no configurations, empty/duplicate
  names, missing/empty `active`) or a vector of warnings from `sanitize_block()` for clampable
  values. Warnings are shown in the CONFIGS window; corrupt files fall back to in-memory factory
  defaults with an explanatory status.
- `save(path, &mut store)` validates the store **in place** (hard errors abort, sanitizations are
  applied) and returns the warnings, creates the parent directory, writes pretty JSON plus a
  trailing newline to `<name>.json.tmp` in the same directory, then `rename`s it over the target —
  an atomic replacement. The UI saves a candidate clone and commits that same, now-validated store
  only on success; the returned warnings replace `config_warnings` and are shown in the CONFIGS
  window, so memory and disk cannot diverge.
- `ConfigStore` API: `factory()`, `active_block()` / `active_block_mut()`, `switch_active(name)`,
  `create(name, block)` (new block becomes active), `remove(name)` (refuses `Default` and the
  active block). The CONFIGS window implements switch/delete confirmation and shows the file path
  and saved/unsaved state.

### 6.4 UI-side state transitions

- `apply_block()` copies a stored block into working UI state; `block_from_state()` snapshots UI
  values into a named `ConfigBlock`.
- `reset_cpi_to_defaults()` and `revert()` are **local only**: no file write and no HID send.
- SAVE writes JSON then applies settings; if no device is connected the config is saved but no
  packet is sent.

---

## 7. UI integration — `src/ui/*`

### 7.1 Entry point and theme

`main()` embeds `assets/AppIcon.png` as the runtime icon so eframe does not replace the bundle icon,
and configures `NativeOptions`: fixed inner/minimum size from `WINDOW_W = 1032`, `WINDOW_H = 527`,
centred window, `persist_window = false`. `ui::theme` defines column widths (215/312/437 pt),
content width 1012, column gap 24, header 56, bottom bar 38, `shade()` and the palette
(`C_PAGE`, `C_PANEL`, `C_HEADER`, text/palette constants, `RGB_OFF`, five CPI indicator colours),
plus `configure_style()` for egui visuals, spacing and text styles.

### 7.2 Application state (`Aerox5App`)

Grouped responsibilities:

- status/event history: `status`, `events`, `show_event_log`, `event_log_reset_scroll`;
- device: `connected_pid`, `device_info`, `battery`, `battery_retry`, `battery_generation`,
  `mouse_texture`;
- sensitivity: `cpi_levels`, `cpi_count`, `selected_level`, CPI animation state;
- polling/timers/colours: `polling_rate`, polling graph animation state, `sleep_minutes`,
  `dim_seconds`, `color_top/mid/bot`;
- config: `configs`, `config_path`, `config_warnings` (last load or save), pending action,
  new-config name;
- UI flags: `tab`, `selected_action`, `rgb_enabled`, `live_zone_colors`, `zone_brightness`,
  `startup_lighting`, `reactive_color`, `show_info`, `show_configs`;
- plumbing: `commands: Sender<HidCommand>`, `responses: Receiver<HidResponse>`.

`RgbState` and `ZoneState` are explicit enums (used instead of bool parameters); `SettingsTab`
selects Settings/Illumination; `PendingConfigAction` models Switch/Delete confirmation.

### 7.3 Header, status and event log

Two-row 56 pt header: row 1 shows the truncated active configuration name and the round `i` button;
row 2 shows a status dot plus the connection summary on the left and the clickable event line on the
right. `event_line()` and the `EVENT ▾/▴` label both toggle the log popup. `EventLog` stores session
events in a bounded `VecDeque` (newest-appended, `%H:%M:%S` timestamps); `EVENT_LOG_MAX_ENTRIES =
256` drops the oldest entry when full. The popup renders `iter_newest_first()` and starts scrolled
to the newest entry. `EVENT_LOG_MAX_VISIBLE_ROWS = 15` sizes the popup viewport. The popup has an
`EVENTS` header and a `×` close button.
The bottom bar has `CONFIGS` on the left and `SAVE` / `REVERT` on the right.

### 7.4 Central panel and panels

`central_panel()` fixes the three columns at 215/312/437 pt with 24 pt visual gaps.

- **Left:** Settings tab → ACTIONS list (12 entries with `ACTION_ORDER` keeping indices tied to the
  B1–B9 callouts; CPI Toggle displayed last) plus a display-only BLUETOOTH MODE panel; Illumination
  tab → RGB controls (master toggle, effect dropdown over `DefaultLighting::ALL`, reactive colour)
  and live-zone controls (toggle, TOP/MID/BOT swatches, 0–100 brightness). Button mapping, Bluetooth
  and the macro editor are deliberately non-functional with explanatory tooltips: the documented
  protocol does not expose them.
- **Centre:** 146×44 Settings/Illumination tabs, the mouse illustration with either callouts or
  three-zone glow, and an intentionally empty tail strip so the mouse does not move between tabs.
- **Right:** CPI level boxes, the Adjusting CPI editor, and bottom battery-saver/polling panels.
  CPI values are 100–18000 in steps of 100, snapped via `snap_cpi`; ascending auto-sort animates
  tiles with `CPI_SWAP_SECONDS = 0.18` while fixed rects keep receiving clicks; max five levels.
  Timers use 0.5-minute steps and `MAX_TIMER_MINUTES = 20`; the polling waveform morphs between
  pulse counts computed as `rate / 125 + 2` over `POLLING_GRAPH_SECONDS = 0.25`.

`ui::widgets` provides `panel_shell`, `content_frame`, section headers, flat link buttons, tab
buttons, sliders, discrete sliders, `level_box` (+ motion computation), `rgb_toggle`, `status_dot`
and `info_button`. `ui::mouse` embeds `../../assets/Aerox5WirelessTop.png` once into a texture,
centres it with equal margins, draws the illumination glow behind it and overlays the nine B1–B9
callouts.

### 7.5 Windows

- **Product information and help** (`i` button): refresh button, two-column grid of device fields
  from `DeviceInfoView`, battery level/charging/raw bytes, and the current status.
- **Configurations** (`CONFIGS` button): scrollable list of blocks with active marker and summaries,
  confirmed switch/delete, inline NEW/create, file path, saved/unsaved state and config warnings
  from the last load or save.

---

## 8. Build and packaging integration — `scripts`

- Usage: `scripts/build-app.sh [--install] [--open] [--no-sign] [-h|--help]`; requires macOS,
  `Cargo.toml` and `cargo` in `PATH`. Config: app name "Aerox 5 Control", bundle id
  `se.aerox5.control`, minimum macOS 11.0, output `dist/Aerox 5 Control.app`.
- Steps: `cargo build --release`; create a fresh `Contents/MacOS` + `Contents/Resources` bundle and
  copy/chmod the binary; generate `AppIcon.icns` from `assets/AppIcon.png` using a temporary
  `.iconset` directory and `sips -z` for all ten sizes 16–1024, then `iconutil`; generate
  `Info.plist` (names, identifier, executable, version, `LSMinimumSystemVersion`,
  `NSHighResolutionCapable`, optional icon) and lint it with `plutil`; ad-hoc `codesign --force
  --deep --sign -` unless `--no-sign`.
- `--install` copies the bundle to `/Applications`; `--open` touches the bundle, refreshes
  LaunchServices with `lsregister` and runs `open` to avoid stale Dock icons.
- **Packaging stops at the signed `.app`.** No DMG or other installer is produced.

---

## 9. Cross-cutting invariants

1. **No HID calls on the GUI thread.** All `HidApi` use stays on the single HID worker; the IOKit
   callback only drains its iterator and sends an internal signal.
2. **One listener, registered once.** Registration happens in `Aerox5App::new`; hotplug events
   never add listeners. Each callback must drain its iterator to keep the notification armed.
3. **The PID-filtered scan is authoritative.** The VID-only IOKit match is only a "something
   changed" trigger; connection state comes from `find_aerox5()`.
4. **Bursts are coalesced** with a 150 ms trailing debounce so one physical event means one scan,
   one battery query and one pair of log entries.
5. **Boot never writes.** Configuration is read-only until an explicit user action.
6. **Persist before send.** SAVE writes JSON first and aborts before building packets on failure;
   REVERT is local only.
7. **Atomic config writes** via sibling temp file + rename; `save` validates in place and callers
   commit the same validated store that was written.
8. **One response channel** shared by the worker and coalescer; the UI drains it once per frame.
9. **Scan and battery replies are generation-checked.** A superseded scan cannot restore connection
   state or arm a query for the previous device; a late no-level reply cannot erase a known level.
10. **core never imports ui.** New shared logic belongs in `core`, new egui code in `ui`.

## 10. Known limitations and risks

- Interface 3 is required for battery/apply; Windows hidapi often reports `-1` and is unsupported.
- `find_aerox5` takes the first VID+PID entry regardless of interface, so the displayed interface
  may differ from the one later used for communication.
- The IOKit match is VID-only, so other SteelSeries devices and every HID interface of the mouse
  generate events; the debounce coalesces them but adds up to 150 ms latency.
- Wireless mouse power-off with the dongle inserted still appears "connected".
- On non-macOS or after IOKit setup failure there is no automatic hotplug detection; use Refresh.
- Apply is not atomic: a write failure can leave partially applied settings and no rollback.
- The dongle's 64-byte post-write readback is performed but discarded.
- Battery and `Scan` replies are dropped after disconnect/rescan when their generation is stale;
  a late reply with no decoded level preserves the last known level instead of clearing it.
  Replies are not otherwise sequenced beyond the single FIFO response channel.
- The retry schedule is driven by the 200 ms repaint cadence and recomputes deadlines from `now`
  at send time, so it can drift; the first 250 ms delay equals the theoretical worst-case query.
- Battery decoding (report ID byte plus payload byte) is inferred from rivalcfg, not documented by
  the device; charging can be known even when the level cannot be decoded.
- The HID worker is unnamed and detached; it lives until its channels close.
- Button mapping, Bluetooth mode and macro editing are display-only.

## 11. Document index

| Document | Purpose |
|---|---|
| `AGENTS.md` | Working agreements and task-document workflow; requires updates to this file |
| `ARCHITECTURE.md` | This document — architecture and integrations, authoritative for agents |
| `README.md` | User-facing documentation and limitations (English) |
| `docs/com-spec.md` | Reverse-engineered HID protocol reference and test vectors |
| `docs/steelseries-gg-cpi-analysis.md` | Analysis of how SteelSeries GG obtains CPI values |
| `work/TASKS.md` | Registry of completed tasks |
| `work/tasks/NNN.md` | Detailed specifications and findings for each task |
