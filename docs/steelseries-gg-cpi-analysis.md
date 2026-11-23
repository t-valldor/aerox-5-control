# SteelSeries GG — how CPI values are obtained (static analysis of app 108.3.0)

**Date:** 2026-11-23 · **App inspected:** `/Applications/SteelSeries GG/SteelSeries GG.app` v108.3.0 on macOS · **Method:** static bundle reading (Info.plist, scripts, file sizes, extracted asar strings + greps across JS bundles) — no write commands sent to any device

This is a defensive read-out of the client and its companion Go core; nothing here was tested against hardware. See **§5 / Limitations** at the end.

---

## 1. Architecture discovered in the bundle

```
Electron "SteelSeries GG Client.app" (Renderer)               SteelSeriesGG Go core
┌───────────────────────────┐                                 ┌─────────────────────┐
│ render/*.js                │  fetch(`device/{id}...`, ...) → │ local HTTP routes   │
│ - deviceConfig page        │                                  │ (submits like       │
│   loads stored config into │  writes via submit():            │ `device/writeCommand`│
│   locally-held MobX tree,   │                                  │ etc.)               │
│   derives CPI for UI        │ ← push events (batteryEvent,    │ device state stored │
│ never reads live hardware  │     connectionStatus) from core  │ in local sqlite db  │
└───────────┬───────────────┘                                 └─────────▲───────────┘
            │ preload (ipcRenderer.invoke/send, exposed to window as `api` / `handlers` / `lib`) — main process relays via local bridge; not traced further in this analysis
```

**Evidence for client-side flow:** extraction of the Electron bundle confirms both sides of the contract. In `render/deviceConfig.js`, after loading device info from hash + init (in a single minified block), it does:

> `Promise.all([a.e(403),…,a.e(162),a.e(377),…])` → `c.deviceConfigInitialize()` → `whenServerIsAvailable(async () => fetch(`device/${window.ssenextData.id}`, {device: …} => init view model + applyBindings)`.
> Then the main device-loading path (see render/162.js around offset ~…) does `fetch('device/<id>/configurations', ...)` → builds an in-process settings tree per configuration and calls bindings; CPI values are read from that local, snapshotted JSON — never from a HID query.

**Key files (from extracted asar under the session scratch dir `…steelseries-gg/v/`):**
- `render/166.js` (~1.37 MB minified JS): settings trees + user CPI logic, config registries for device families, batch_resolution & set_all_resolution handling paths — **the one source file that contains all of it**. It holds ~90 occurrences of `batch_resolution`, 58 `raw_cpi`, 27 `set_all_resolution`, 11 `user_cpi`; see §3 below for coordinates.
- `render/162.js` (~9.2 MB): device + configuration wiring — the Aerox5 family maps are here as literal config-view registry entries and a separate device view registry (see offset ~158173 in this file, mapping keys like `./aerox_5_wireless/device` → module 26680) covering aerox & other mice; it also holds the main device loading / save flow via `fetch('device/{id}')` and `fetch('device/{id}/configurations')`.
- `render/693.js` (~626 KB minified JS): contains another registry of per-family deviceCard views (e.g. `./aerox_5/deviceCard` → 33447, `./aerox_5_wireless/deviceCard` → 18288) plus the submit() route map with writeCommand routes — **the actual CPI readout path is not in this file**; it is only where command submission happens.
- `main/preload.js` (~10 KB, plain JS): exposes an `api` object to renderer via contextBridge — main window control, dialogs, fs bridge, global handler registration — and has no network endpoints or CPI logic of its own; it is a narrow Electron API surface over IPC + Go core.
- `main/index.js` (~428 KB): electron Main process (window registry, certificate verification handling, quit/resume/focus handlers, system-sleep wake path registering system resume broadcast events). **Not traced further in this analysis** — it holds window/system event handling, not device I/O beyond that.
- `render/index.js` (~2.35 MB minified JS entry): main app UI bootstrap; its contents are not statically expanded into per-model logic here (the exact route call sites in this chunk were not verified to match render/693 verbatim) and holds little direct AeroX wiring — see §2 below for where actual CPI handling lives and friends — those callsites are confirmed only inside render/693.js (two occurrences around byte offset ~528781/529556) and the device configuration page code in `render/deviceConfig.js` (one occurrence near the top of file, around byte offset ~7026); see §5 limitations on exact per-model mapping — those listener registrations were not traced further inside index or any other JS chunk beyond that single entry point.

**Bundle inventory verified today (read-only):** Info.plist bundle id `com.steelseries.SteelSeries-GG`, LSUIElement=true; MacOS files sized as expected and listed below:

| File | Size | Role (verified by inspection / strings scan where applicable) |
|---|---|---|
| MacOS/SteelSeries GG (stub launcher) | ~71 KB | thin Carbon+AppKit stub; not further analyzed beyond existence of script hookup in Resources |
| MacOS/SteelSeriesGG Go binary | ~18.4 MB | the device/UI-core bridge — sees HTTP routes via strings in this file (see §3 & §6); runs as `nohup` from launchSSENextCore.sh |
| MacOS/SteelSeriesGGUpdateService | ~8.3 MB Mach-O service binary | self-contained update daemon; not further analyzed |
| MacOS/GGTableMigrations (migration helper, x86_64) | ~4.2 MB Mach-O binary | sqlite migration tooling for Resources db; role verified only as helper by filename + launch scripts listing it is a separate step in app startup logic — no direct callsite located in this analysis beyond that hint from earlier sessions' notes (kept minimal here to avoid overclaim) |
| Frameworks/libInputLib.dylib | ~72 KB x86_64 Mach-O C++ shared lib | `UTListenerBase`, `RecordingLibBase`, `InputLib` classes; links IOKit + CoreFoundation — not further analyzed beyond existence and class names (no strings found for CPI paths) |
| Helpers/DeviceReplug (undocumented helper binary ~36 KB, x86_64 Mach-O) |  | undocumented in this analysis |
| Helpers/SteelSeries GG Client.app | Electron app nested inside the same bundle — holds render/* & main js files | see above table for per-file notes; extracted to scratch dir for analysis only and **not committed** |
| Resources: launchSSENextCore.sh / stopSSENextCore.sh | read below (§6) | sets `APP_DATA_PATH=/Library/Application Support/SteelSeries GG` via env, deletes coreProps.json at startup, runs SteelSeriesGG in background with log file; stop script kills the process by PID |
| Resources: com.steelseries.SSENext.plist | RunAtLoad=true LaunchAgent (ProgramArgument = app stub launcher + extra args) — autolaunch path through the stub + SSENext core binary, per prior notes and verified key extraction today | see §6 for exact keys; full plist contents checked read-only earlier but not dumped in this report due to length |
| Resources: version.json → {"version": "108.3.0"}; db/ directory (sqlite migration set, `migratedTables/` copy) — local persistence storage for config state is confirmed by presence of these files and launch-script wiring; **no specific table names or full schema were traced in this analysis** | see §5 Limitations |
| Resources: dbconf.yml (sqlite driver, `DBPATH #DBPATH#`) — single local database file reference; verified by filename + content type only today | not further verified here |

---

## 2. CPI data flow in the Electron client

**There are two user-visible CPI patterns in the bundle, both reading stored config snapshots (not live device state):**

### 2a. `batch_resolution` path — settings object with fields like `active_level_index` / `num_cpi_levels` / `resolution_1..5`
A local observable tree is consumed by the UI through deferred pureComputed in render/166.js, e.g. at ~offset 1129241:
```javascript
e.settings().batch_resolution && (e.user_cpi = o().pureComputed({read:()=>{const t=o().toJS(e.settings().batch_resolution); return M.deviceToFrontend(t)}, write:... }))
```
and at ~offset 647883 with an inline read function that selects CPI by active level index (0..4 → resolution_1..5 via a helper `M(x)` which is presumably the device-to-frontend conversion table; its complete implementation could not be statically extracted — see §5 limitations):
```javascript
e.user_cpi ||= o().pureComputed({read: () => { if (!e) return w /*default*/ ; const {active_level_index, num_cpi_levels, resolution_1..5} = e; ... 0 === activeLevelIndex ? currentCpI = M(resolution_1)... }
```
and also at ~offset 36554 / ~48436 — both use `g.deviceToFrontend(toJS(settings().batch_resolution))` (resp. f) — same shape, different decorated class contexts (those two sites appear to be among the config-model view models associated with Aerox-5 family by earlier read of sibling literals; exact per-model attribution is not statically verifiable and remains open — see §5 limitations).

### 2b. `set_all_resolution` + raw CPI path (sibling config pattern)
A separate sibling pattern in this same file (~offset 224220) handles a different shape... The specific association of these two paths with the Aerox-5 family specifically is not resolved by static grep; it remains unverified — see §5 Limitations for what was and was not confirmed.

```javascript
if(e && e.settings().set_all_resolution){ n.user_cpi = e => pureComputed read {deviceToFrontend}, write {level_active → settings().level_active; level_max …; raw_cpi fields written via frontendToDevice to per-level `raw_cpi.level1`/`level2` — both shown being set together inside the same write function at that site (e1d ... )}
```
explicitly: `level_active`, `level_max`, resolution levels are written; `settings().raw_cpi.level1(d.level1)` and likely level2 also appear in this branch (58 raw_cpi hits overall in the file — see §3 for more context patterns but not exact per-model assignment).

Both paths read CPI from a **locally held, previously fetched configuration snapshot**; both write through MobX observables to persist settings via device submit routes (e.g. `submit('configuration/${id}/save'...)` as seen in loadConfigs flow around offset ~1123xx–1130k — see §4 Limitations for details on exact route path used for saves, which is not fully enumerated here either) and never via HID reads from the mouse.

---

## 3. What GG Go binary exposes (static strings / struct evidence only)

**Search results:** running `strings`/`grep -E` on SteelSeriesGG directly found no occurrence of any of these CPI-related identifiers: `batch_resolution`, `raw_cpi`, `num_cpi_levels`, `active_level_index`. The same holds for libInputLib.dylib's strings (see §5 Limitations on what static text grep can and cannot prove in a minified/compiled Mach-O binary).
**Go string patterns found:** many HTTP route fragments that mirror the client submit pattern, e.g. `device/{id}`, `device/v1/{id}` / `error` strings in various contexts (e.g. error handling paths) plus several other routes we do not need to enumerate exhaustively for this analysis beyond the ones actually read by the client during load, including:
- Reads that hit only **firmware version keys** — `device/{id}/readFromDevice/fw_version_stm32` and `.../fw_version_conexant` (both seen in JS context at render/693.js; these are actual roundtrips to the device, via readFromDevice path)
- **Audio settings** — `get_audio_settings` route (seen once per JS string count = 1 callsite): same category as firmware reads (dynamic hardware fetch path with real payloads inside GG core → not CPI related); other routes in the list are all submit/write patterns that exist primarily to send commands or mutations rather than settings queries
- `v1/refreshWirelessDeviceConnectionStatus` — explicit refresh event for wireless state after deploy (not readback of sensitivity information).
**No string found anywhere related to CPI / raw_cpi / resolution fields exists in either binary examined by string grep, including no `device/*settings*` or active-level getter route nor any HID reply decoding path — this is consistent with rivalcfg/com-spec notes (see work/tasks/004.md §1) recorded earlier about there being no documented CPI read command from Aerox devices over raw HID today; GG inherits the same limitation for its stored settings persistence.**

---

## 4. Route list & payload shapes — enumerated as statically visible in render JS (grouped, incomplete by design)

The submit() wrapper used throughout is `submit(routeString, callbackOrNull [optional], {id: deviceId, command?: JSON-stringified inner object})`; seen explicitly at one writeCommand callsite with a real payload shape like `{id: id, command: JSON.stringify({pair_devices: {other_device: productId}})}` — i.e. route + deviceId + an opaque stringifiable inner command envelope, wrapped to core over the local bridge (not yet fully decoded at wire level by this analysis; treated as static evidence only).

**Routes observed via JS text grep** (grouped with counts in one pass across render/*.js files — total ~40 unique patterns, representative but not an exhaustive API contract):
- `device/writeCommandWithLightingPause` ×10; `device/command` ×9; `device/writeCommand` ×8 — all write command envelopes (most commonly with JSON stringified inner payload on the wire over local HTTP bridge).
- Other submit routes visible once per site: `application_setting`, `featureSplashInfo`, keyboard settings / deviceConfig flows like `v1/refreshWirelessDeviceConnectionStatus`, `systemSettings/setDefaultMouseSettings`; prismSync & game integration routing family (e.g. leftHanded/inactiveMode enable+ disable, updateTriggerModes etc); macros/writeCommandWithLightingPause patterns. The complete set of route strings is ~40 uniques spanning: device management + commands, configuration optimization and save flows, login / forceCloudSync / cloud_settings sync routes (with `cloud_settings` being one such top-level family visible at counts 1 each for many subroutes); keyboard layout fallback write/fetch operations seen earlier in code fragments.
**GET endpoints found by string grep** — all under local HTTP:
- Device info + config listing fetched via `fetch('device/<id>', … { device: <observed model object} )` and then a second call into `/configurations` for per-config data (the only routes that surface "current CPI snapshot in the UI" — this includes stored batch/set_all resolution values when deserialized by client code). Both are part of loadConfigs / optimizeAllConfigs flow and also drive display name & deviceName refresh after deploy operations.
- readFromDevice subroutes (fw_version_stm32, fw_version_conexant, get_audio_settings) — dynamic fetch paths for firmware/audio status only; none of these is a CPI / sensitivity route.
**No endpoint returning live active level index or per-level CPI over HID is identified anywhere in client JS text grep nor core binary strings examined.** Note caveats about static evidence strength from §5 below before taking any statement to be comprehensive beyond these observed artifacts.

---

## 5. Limitations of this analysis and what was NOT verified (important)
- **Per-model wiring is not pinned down:** the config registry map for aerox families lives inside `render/166.js` at byte offset ~1346500+, containing literal keys like "./aerox_5/configuration" → 80317 and "./aerox_5_wireless/configuration" → 84929 — however the *actual* view-layer JS that constructs those models is not found by static grep anywhere in render files (the numbers appear nowhere else as strings, and per-device wiring code was not locatable in this session). The user asked to trace batch_resolution through app.asar with exact module ids + device models covered; the honest state today: both patterns shown exist in one minified file but their precise association with Aerox-5 vs Aerox-3/other mice remains **unverified statically**.
- Wire shapes of Go core payloads (JSON schema, wire proto buffers) are not decoded by this analysis — we can enumerate the submit() routes and a representative command payload example from JS only; exact serialization format, field names beyond raw strings observed at call sites and configuration JSON passed through local snapshot fetches remain unverified.
- No hardware activity was performed (no writeCommand sent to devices). The analysis is static/offline — any full validation of these claims requires dynamic observation or live execution which are outside scope by necessity per §2 acceptance criteria & prior tasks' notes on CPI reads being nonexistent or undocumented today.
- Any claim in this report about "no route exists for X" rests only on the static greps listed above; it is not a guarantee that an alternative internal path (e.g., HID commands handled inside libInputLib) does exist and could have been missed due to minified assembly. Known rivalcfg/doc findings corroborate "no documented CPI get" for era models and GG inherits essentially the same architecture, but this statement is not a formal proof that no path exists in binary internals — only evidence from static strings scans of both sides (JS + Go core) against specific search patterns.
- Dynamic capture of traffic against an attached mouse remains deferred as an explicit optional opt-in step; it was NOT run here or assumed to pass silently. Proper verification requires a temporary local network trace/capture during user interaction with GG (which is not performed in this task by design) — see §2 acceptance criterion #4 and the Blocked section of task doc work/tasks/004.md for explicit status note today.
- Also unresolved: which one configuration class Aerox5 ids correspond to whether it uses batch_resolution or set_all_resolution + raw_cpi; the specific level→CPI mapping function (e.g. what M() is in the inline read of user_cpi at ~647883) remains partially unknown as its complete implementation is not statically extractable without running/decompiling app code and was intentionally not attempted here.
- Nothing in this report asserts that any command, device state push event or stored value is *current*; the config snapshots are fetched once at load time and only refreshed through local writes / deploy actions (not auto-refresh) — UI state may lag behind truth if a mouse changed without GG being reloaded.

---

## 6. Verdict: can we read current CPI from the Aerox 5 today via SteelSeries GG?

**No documented way exists in this app bundle, nor any discovered HID command/API for it directly by this analysis today; neither client JS route list nor Go binary strings found any sensitivity getter route or struct field tag matching raw_cpi / batch_resolution.** The closest available information flow is the local configuration snapshot read at load time (see §2 & §4) that renders stored CPI from persisted settings — which reflects last-deployed values only, not live device truth; and no active-level event push was identified in client JS text scans (no such event listener was found — e.g. batteryEvent/chargerBattery are the events registered by UI code but sensitivity state is absent). In practice:
- GG stores user-input settings locally per configuration via config snapshots fetched at launch; those give you per-level CPI numbers only if Aerox writes them in that shape (batch_resolution or set_all + raw_cpi fields as shown by the code paths identified above but NOT statically pinned to specific models)
- The client reads these locally and renders user_cpi through pureComputed; there is no HID getter path for CPI today, nor any documentable endpoint that returns live active level.
- Known evidence across bundles + rivalcfg/com-spec notes from prior task work corroborates: Aerox5 devices over raw HID expose battery levels via `00 92`/`00 D2` (per prior tasks) but never a documented sensitivity get — GG inherited this; it doesn't change that reality.
- A full validation of whether any live CPI read path exists in binary internals requires dynamic traffic capture against hardware and is left as an explicitly deferred, opt-in step per §2 acceptance criterion #4 (NOT run during this work). Any future reader should treat §6 as "no such documented or discovered route today" rather than formally proven absent at the byte level of binary internals — because static text grep is not exhaustive. Until dynamic observation confirms either way, safest inference: GG does NOT give you current CPI from hardware; it gives you last-deployed values stored locally on disk and rendered in its UI via pureComputed derived view models.
