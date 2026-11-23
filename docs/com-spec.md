# Aerox 5 Wireless — Communication Specification

Reverse-engineered communication protocol for configuring the **SteelSeries Aerox 5 Wireless** mouse, extracted from the [`rivalcfg`](../../rivalcfg) project (repo state `189ff05`). This document describes everything found in that codebase about how configuration data is transported and encoded.

> **Conventions**
> - All source paths are relative to the `rivalcfg` repository root unless stated otherwise.
> - Hex values are written as `0xNN`; test byte sequences are written as `02 00 6D ...` (see §2.3 for why tests start with `02`).
> - "Wired" = the profile `aerox5_wireless_wired.py`; "wireless" = `aerox5_wireless_wireless.py`, which patches the wired profile.
> - Statements taken directly from code/tests are cited with `file:line`. Inferences are labelled explicitly.

---

## 1. Device identification and interface selection

### 1.1 Supported products

All profiles use **vendor ID `0x1038` (SteelSeries)** and **`endpoint: 3`**.

| Mode | PID | Product | Source |
|---|---|---|---|
| Wired (USB) | `0x1854` | SteelSeries Aerox 5 Wireless (wired mode) | `rivalcfg/devices/aerox5_wireless_wired.py:10-15` |
| Wired (USB) | `0x185E` | Aerox 5 Wireless Destiny 2 Edition (wired mode) | `aerox5_wireless_wired.py:16-21` |
| Wired (USB) | `0x1862` | Aerox 5 Wireless Diablo IV Edition (wired mode) | `aerox5_wireless_wired.py:22-27` |
| 2.4 GHz dongle | `0x1852` | Aerox 5 Wireless (2.4 GHz wireless mode) | `rivalcfg/devices/aerox5_wireless_wireless.py:21-27` |
| 2.4 GHz dongle | `0x185C` | Aerox 5 Wireless Destiny 2 Edition (2.4 GHz wireless mode) | `aerox5_wireless_wireless.py:28-33` |
| 2.4 GHz dongle | `0x1860` | Aerox 5 Wireless Diablo IV Edition (2.4 GHz wireless mode) | `aerox5_wireless_wireless.py:34-39` |

The related **wired-only Aerox 5** (`0x1850`) uses the same command set with one important difference in polling-rate encoding (§4.2); it has no battery query in its profile — `rivalcfg/devices/aerox5.py:4-13,158-161`.

The legacy SteelSeries database maps each wired PID to its dongle PID (`ssdb/sse3.db.csv`, columns `product_id,dongle_product_id`):

```
1038,1854,1852,aerox_5_wireless,Aerox 5 Wireless
1038,185e,185c,aerox_5_wireless_destiny_2,Aerox 5 Wireless Destiny 2 Edition
1038,1862,1860,aerox_5_wireless_diablo_4,Aerox 5: Diablo IV Edition
```

The legacy database keys are `aerox_5_wireless`, `aerox_5_wireless_destiny_2` and `aerox_5_wireless_diablo_4` (`ssdb/sse3.db.csv:61,66,67`); the wired-only model uses `aerox_5` (`:58`). The README changelog lists all six wireless PIDs in the TrueMove Air DPI fix (`README.rst:93-100`).

### 1.2 `endpoint: 3` is the HID interface number

Despite the name "endpoint", `usbhid.open_device()` matches the value against hidapi's `interface_number` and opens the device by path (`rivalcfg/usbhid.py:53-97`):

```python
for interface in hid.enumerate(vendor_id, product_id):
    if interface["interface_number"] == endpoint:
        path = interface["path"]
        break
if path:
    device.open_path(path)
```

No filtering by usage page, usage, product string or serial is performed. Profiles are generated per VID/PID at import time from each module's `profile["models"]` (`rivalcfg/devices/__init__.py:213-268`); `get_profile()` raises `UnsupportedDevice` for unknown VID/PIDs (`:187-210`).

**Inference (not in rivalcfg):** on Windows, hidapi often reports `interface_number = -1`, which would prevent the strict `== 3` comparison from matching. No workaround exists in this codebase.

### 1.3 Wired vs wireless mode detection

There is no runtime detection or special open-time handling. The two modes are separate profiles selected purely by PID. All protocol differences are implemented by patching the wired profile (§3).

---

## 2. HID transport

### 2.1 General call chain

For a CLI invocation such as `rivalcfg --polling-rate 1000 --save`:

1. `get_first_mouse()` → `mouse.get_mouse()` (`rivalcfg/__init__.py:7-32`, `rivalcfg/mouse.py:11-39`).
2. `mouse.get_mouse()` resolves the profile, settings store, and calls `usbhid.open_device(vid, pid, profile["endpoint"])`.
3. `Mouse.__getattr__()` synthesizes `set_<setting>()` methods: handler → assemble bytes → `_hid_write()` → optional readback (`rivalcfg/mouse.py:316-378`).
4. `mouse.save()` sends the save command and persists local JSON settings (`rivalcfg/mouse.py:221-252`).

A single `hid.device` handle is held per `Mouse` instance; there are no global HID handles, retries, wake-up sequences or multi-write framing (`rivalcfg/mouse.py:82-111,254-262,388-395`).

### 2.2 Report framing

`Mouse.__getattr__()` assembles the payload (`rivalcfg/mouse.py:343-362`):

```python
data = handlers.<handler>.process_value(setting_info, *args)
self._hid_write(
    report_type=setting_info["report_type"],
    data=helpers.merge_bytes(setting_info["command"], data, suffix),
    packet_length=packet_length,
)
```

`Mouse._hid_write()` prepends the report ID and optionally pads (`rivalcfg/mouse.py:264-310`):

```python
def _hid_write(self, report_type=usbhid.HID_REPORT_TYPE_OUTPUT,
               report_id=0x00, data=[], packet_length=0):
    if packet_length:
        bytes_ = bytearray(helpers.merge_bytes(
            report_id, data, [0x00] * (packet_length - len(data))))
    else:
        bytes_ = bytearray(helpers.merge_bytes(report_id, data))
    ...
```

General layout:

```
[ 00 ][ command bytes ][ encoded value ][ command_suffix ][ 00 padding ]*
  ^-- report ID (always 0x00)
```

- The report ID is hard-coded to `0x00` in the default argument (`rivalcfg/mouse.py:267`, docstring `:278`); no production caller overrides it. Project docs: "report ID (always `00` on SteelSeries devices)" (`doc/env.rst:44`).
- `helpers.merge_bytes()` flattens ints and lists into one list of ints (`rivalcfg/helpers.py:11-31`).
- The Aerox 5 profiles define **no `packet_length` and no `command_suffix`** on any setting, so their reports are simply `00 | command | value` with **no padding**.
- Multi-byte numeric values are packed **little-endian** by `uint_to_little_endian_bytearray()` (`rivalcfg/helpers.py:109-134`).

### 2.3 Report types and the `02` prefix seen in tests

`rivalcfg/usbhid.py:26-30`:

```python
HID_REPORT_TYPE_OUTPUT  = 0x02
HID_REPORT_TYPE_FEATURE = 0x03
```

These are USB HID report-type codes used only to choose which hidapi function is called (`rivalcfg/mouse.py:305-310`):

```python
if report_type == usbhid.HID_REPORT_TYPE_OUTPUT:
    self._hid_device.write(bytes_)
elif report_type == usbhid.HID_REPORT_TYPE_FEATURE:
    self._hid_device.send_feature_report(bytes_)
```

**All Aerox 5 settings, battery and save commands use `HID_REPORT_TYPE_OUTPUT`** (`aerox5_wireless_wired.py:34,48,63,72,81,90,99,111,123,154,162,175,182`), i.e. plain `hid.write()`. Feature reports are not used for this mouse. `get_feature_report()` is never called on a real device.

The `02` at the start of every expected byte string in the tests is **not on the wire** — it is a tag inserted by the fake device (`rivalcfg/usbhid.py:128-145`):

```python
def write(self, data):
    self.bytes.write(struct.pack("B", HID_REPORT_TYPE_OUTPUT))  # 0x02
    self.bytes.write(data)
```

So a test value `02 00 6B 00` means: fake tag `02` + actual on-wire report `00 6B 00`. The end-to-end `.txt` spec files use the same notation `< 02 00 | ...`, described as "wValue (report type and report ID)" in `test/devices/README.rst:59-75` and `doc/env.rst:30-44`.

> **For an implementation (e.g. Rust hidapi `write()`), send only `00` + command + value.** Do not send the `02`.

### 2.4 Timing and sequencing

- `_hid_write()` sleeps **50 ms** after every write, by default (`rivalcfg/mouse.py:82-85,92-111,312-314`):

  ```python
  # Avoids sending multiple commands to quickly
  if "RIVALCFG_DEBUG_NO_COMMAND_DELAY" not in os.environ:
      time.sleep(self._command_approve_delay)   # default 0.05 s
  ```

  Minimum allowed value is 1 ms; the docs warn that too-short delays can hang/crash some mice (`mouse.py:93-101`).
- Reads use `hid.read(max_length, timeout_ms=200)` (`mouse.py:141,182,247,368`), i.e. a 200 ms timeout.
- CLI sequence: `setting write → 50 ms → [64-byte readback] → ... → save write → 50 ms → [64-byte readback] → JSON persist`.
- `no_live_deploy` appears only in the legacy `ssdb/*.csv` databases, **not** in rivalcfg runtime code; it has no effect on this HID flow. (Its usual Engine meaning is "apply on save, not live" — inference only.)

### 2.5 Readback (wireless mode only)

The wireless profile adds `readback_length = 64` to every command (§3). After each write, rivalcfg performs (`rivalcfg/mouse.py:363-376`):

```python
response = None
if "readback_length" in setting_info and setting_info["readback_length"]:
    response = self._hid_device.read(
        setting_info["readback_length"], timeout_ms=200)
```

- This is a **post-write input-report read**, not a feature-report read.
- The profile spec describes it as "required by some wireless mice like the Aerox 3 Wireless in wireless mode" (`rivalcfg/devices/__init__.py:58-61`).
- Wireless tests assert `len(response) == 64` (`test/devices/old_specs/test_aerox5_wireless_wireless.py:39,55,69,83,97,113,129,146,222,229,259`).
- The CLI discards the returned bytes; only the Python API exposes them (`rivalcfg/__main__.py:130,134`).
- **Inference:** byte 0 of the read buffer is the input report ID; payload starts at index 1 (consistent with the battery decoder reading `data[1]`).
- Nuance: the injected `readback_length` on `battery_level` is **inert** — `Mouse.battery` reads `response_length` (2) instead (`mouse.py:180-183`).

### 2.6 Debug facilities

`RIVALCFG_DEBUG_PRINT_HID_REPORT` prints `[USBHID]< <type> <id> | <payload>` on writes and `[USBHID]> | <bytes>` on reads (`mouse.py:144-145,185-186,295-303`; `doc/env.rst:23-44`). `RIVALCFG_DRY` substitutes a fake device; `RIVALCFG_PROFILE=VID:PID` forces a profile to appear plugged in (`usbhid.py:6-18,33-50,72-97`).

---

## 3. Command map (wired vs wireless)

The wireless profile is generated from the wired one by `_patch_command()` (`aerox5_wireless_wireless.py:1-16,41-47`):

```python
_WIRELESS_FLAG = 0b01000000   # 0x40
_READBACK_LENGTH = 64

def _patch_command(info):
    result = dict(info)
    result["command"] = list(result["command"])
    result["command"][0] |= _WIRELESS_FLAG   # only the FIRST command byte
    result["readback_length"] = _READBACK_LENGTH
    return result
```

Applied to every setting, `battery_level` and `save_command`. Bit 6 (`0x40`) is OR-ed into the command byte — **not** into the `0x00` report ID. All other bytes are unchanged.

| Setting | CLI flags | value_type | Wired command | Wireless command | Value bytes | On-wire length (wireless) |
|---|---|---|---|---|---|---|
| `sensitivity` | `-s`, `--sensitivity` | `multidpi_range_choice` | `2D` | `6D` | `2 + n` (count, preset, n×DPI) | `4 + n` (default 5 DPIs → 9) |
| `polling_rate` | `-p`, `--polling-rate` | `choice` | `2B` | `6B` | 1 | 3 |
| `z1_color` (top strip) | `--top-color`, `--z1` | `rgbcolor` | `21 01 00` | `61 01 00` | 3 (R,G,B) | 7 |
| `z2_color` (middle strip) | `--middle-color`, `--z2` | `rgbcolor` | `21 01 01` | `61 01 01` | 3 | 7 |
| `z3_color` (bottom strip) | `--bottom-color`, `--z3` | `rgbcolor` | `21 01 02` | `61 01 02` | 3 | 7 |
| `reactive_color` | `-a`, `--reactive-color` | `reactive_rgbcolor` | `26` | `66` | 5 | 7 |
| `sleep_timer` | `-t`, `--sleep-timer` | `range` | `29` | `69` | 3 (LE ms) | 5 |
| `dim_timer` | `-T`, `--dim-timer` | `range` | `23 0F 01 00 00` | `63 0F 01 00 00` | 3 (LE ms) | 9 |
| `buttons_mapping` | `-b`, `--buttons` | `buttons` | `2A` | `6A` | 55 (11 × 5) | 57 |
| `rainbow_effect` | `-e`, `--rainbow-effect` | `none` | `22 FF` | `62 FF` | 0 | 3 |
| `default_lighting` | `-d`, `--default-lighting` | `choice` | `27` | `67` | 2 | 4 |
| `battery_level` | `--battery-level` | query | `92` | `D2` | 0 (read 2) | 2 |
| `save_command` | (`--no-save` skips it) | command | `11 00` | `51 00` | 0 | 3 |

These lengths **exclude** the `02` tag seen in test recordings. For example, polling `00 6B 00` is 3 on-wire bytes but is printed as `02 00 6B 00` in the tests.

CLI options are generated dynamically from the profile (`rivalcfg/cli.py:104-169`). Generic options include `--no-save`, `-r/--reset`, `--battery-level`, `--print-debug`, `--print-udev`, `--update-udev`. There is **no `--firmware-version`** option because the profile has no `firmware_version` key (`cli.py:155-161`).

Sources: `aerox5_wireless_wired.py:29-185`, `aerox5_wireless_wireless.py:1-47`; byte sequences confirmed in `test/devices/old_specs/test_aerox5_wireless_wired.py:23-248` and `test_aerox5_wireless_wireless.py:23-258`.

---

## 4. Setting encodings in detail

All examples below show the **wireless** on-wire bytes (report ID `00`, command with `0x40` set) and, where useful, the test-visible form with the `02` fake tag.

### 4.1 `sensitivity` — DPI presets (`command 6D`)

Profile: `aerox5_wireless_wired.py:30-43`.

```
value = [ count, selected_preset, <DPI byte> × count ]
```

- Input: 1–5 comma/space-separated DPI values in `[100, 18000]`, step 100 (`input_range`), default `"400, 800, 1200, 2400, 3200"`.
- Each DPI is mapped through the 180-entry `truemove_air.choices` table (§5), rounded to the nearest available key (ties → lower key), then encoded as **1 byte** (`dpi_length_byte = 1`).
- `first_preset = 0`, `max_preset_count = 5`. When no preset is specified, `selected_preset = 0`; otherwise it is passed through unchanged.
- Handler: `rivalcfg/handlers/multidpi_range_choice.py:89-161`; nearest-choice lookup: `rivalcfg/handlers/range_choice.py:78-129`; table: `rivalcfg/devices/dpi/truemove_air.py:3-184`.

Examples (wireless, from `test_aerox5_wireless_wireless.py:23-39`):

| Input | On-wire bytes | Test bytes |
|---|---|---|
| `100` | `00 6D 01 00 00` | `02 00 6D 01 00 00` |
| `200` | `00 6D 01 00 02` | `02 00 6D 01 00 02` |
| `300` | `00 6D 01 00 03` | `02 00 6D 01 00 03` |
| `18000` | `00 6D 01 00 D6` | `02 00 6D 01 00 D6` |
| `200,400` | `00 6D 02 00 02 04` | `02 00 6D 02 00 02 04` |
| `200,400,800,1600` | `00 6D 04 00 02 04 09 12` | `02 00 6D 04 00 02 04 09 12` |
| default `400,800,1200,2400,3200` | `00 6D 05 00 04 09 0D 1B 26` | (derived from table) |

Wired equivalents use `2D` instead of `6D` (`test_aerox5_wireless_wired.py:23-31`).

> **Important for implementations:** the encoded byte is **not** `DPI / 50` or any linear formula. It is a non-linear lookup table (§5). The current Rust GUI's `packet[2] = dpi / 50` is not this encoding.
>
> Historical note: rivalcfg originally used a linear `multidpi_range` mapping (`output_range [0x00, 0xD6, 1.2]`), which produced wrong DPI codes. Commits `30e7258`/`7571126` (2026-01-23) replaced it with the explicit `truemove_air.choices` table for all seven Aerox 5 PIDs; `a6bd645` added `dpi_length_byte: 1`. Corrected example codes: 200 → `0x02` (was `0x01`), 400 → `0x04` (was `0x03`), 800 → `0x09` (was `0x08`), 1600 → `0x12` (was `0x11`).

### 4.2 `polling_rate` — (`command 6B`)

Profile: `aerox5_wireless_wired.py:44-58`.

| Input (Hz) | Value byte | On-wire (wireless) | Test bytes |
|---:|---:|---|---|
| 125 | `03` | `00 6B 03` | `02 00 6B 03` |
| 250 | `02` | `00 6B 02` | `02 00 6B 02` |
| 500 | `01` | `00 6B 01` | `02 00 6B 01` |
| 1000 | `00` | `00 6B 00` | `02 00 6B 00` |

Default: `1000`. The mapping is **inverse** to frequency: 1000 Hz → `0x00`, 125 Hz → `0x03`. Source: `test_aerox5_wireless_wireless.py:41-55`.

Note: the wired-only Aerox 5 (`0x1850`) uses a different mapping (`125→0x04, 250→0x03, 500→0x02, 1000→0x01`) — `aerox5.py:29-43`. Do not mix them up.

### 4.3 LED zone colors — (`commands 61 01 00 / 61 01 01 / 61 01 02`)

Profile: `aerox5_wireless_wired.py:59-85`.

```
value = [ R, G, B ]     (0..255, in that order)
```

- `z1_color` top strip: command `21 01 00` → wireless `61 01 00`, default `red`
- `z2_color` middle strip: `21 01 01` → `61 01 01`, default `lime`
- `z3_color` bottom strip: `21 01 02` → `61 01 02`, default `blue`

Color strings are parsed by `parse_color_string()` (`rivalcfg/color_helpers.py:60-97`): named colors (e.g. `red` = `FF 00 00`, `lime` = `00 FF 00`, `blue` = `00 00 FF`), or `#RRGGBB` / `#RGB` (3-digit form expands each nibble). RGB byte order is direct, no endianness conversion (`rivalcfg/handlers/rgbcolor.py:83-104`).

Examples (wireless):

| Input | On-wire bytes | Test bytes |
|---|---|---|
| z1 `#ABCDEF` | `00 61 01 00 AB CD EF` | `02 00 61 01 00 AB CD EF` |
| z1 `red` | `00 61 01 00 FF 00 00` | `02 00 61 01 00 FF 00 00` |
| z2 `#ABCDEF` | `00 61 01 01 AB CD EF` | `02 00 61 01 01 AB CD EF` |
| z3 `#ABCDEF` | `00 61 01 02 AB CD EF` | `02 00 61 01 02 AB CD EF` |

Sources: `test_aerox5_wireless_wireless.py:57-97` (wired: `test_aerox5_wireless_wired.py:55-85`).

### 4.4 `reactive_color` — (`command 66`)

Profile: `aerox5_wireless_wired.py:86-94`, default `"off"`. Handler: `rivalcfg/handlers/reactive_rgbcolor.py:94-118`.

```
off/disable/None  -> [ 00 00 00 00 00 ]
enabled           -> [ 01 00 R G B ]
```

| Input | On-wire bytes | Test bytes |
|---|---|---|
| `red` | `00 66 01 00 FF 00 00` | `02 00 66 01 00 FF 00 00` |
| `#ff1802` | `00 66 01 00 FF 18 02` | `02 00 66 01 00 FF 18 02` |
| `disable` / `off` | `00 66 00 00 00 00 00` | `02 00 66 00 00 00 00 00` |

Source: `test_aerox5_wireless_wireless.py:99-113`.

### 4.5 `sleep_timer` — idle-to-sleep (`command 69`)

Profile: `aerox5_wireless_wired.py:95-106`; handler `rivalcfg/handlers/range.py:73-163`.

- Input: `0..20` minutes (step 1); `0` = disabled; default `5`.
- Output: `0x000000..0x124F80` (0..1,200,000), step `60000` → effectively **milliseconds = minutes × 60000**.
- Encoded as 3-byte little-endian.

| Input (min) | ms | LE bytes | On-wire (wireless) | Test bytes |
|---:|---:|---|---|---|
| 0 | 0 | `00 00 00` | `00 69 00 00 00` | `02 00 69 00 00 00` |
| 1 | 60000 | `60 EA 00` | `00 69 60 EA 00` | `02 00 69 60 EA 00` |
| 5 | 300000 | `E0 93 04` | `00 69 E0 93 04` | `02 00 69 E0 93 04` |
| 20 | 1200000 | `80 4F 12` | `00 69 80 4F 12` | `02 00 69 80 4F 12` |

Source: `test_aerox5_wireless_wireless.py:115-129`.

### 4.6 `dim_timer` — idle-to-dim (`command 63 0F 01 00 00`)

Profile: `aerox5_wireless_wired.py:107-118`.

- Input: `0..1200` seconds; `0` = disabled; default `30`.
- Output: `0..1,200,000`, step `1000` → **milliseconds = seconds × 1000**.
- 3-byte little-endian, appended after the 5-byte command prefix.

| Input (s) | ms | LE bytes | Test bytes (wireless) |
|---:|---:|---|---|
| 0 | 0 | `00 00 00` | `02 00 63 0F 01 00 00 00 00 00` |
| 30 | 30000 | `30 75 00` | `02 00 63 0F 01 00 00 30 75 00` |
| 60 | 60000 | `60 EA 00` | `02 00 63 0F 01 00 00 60 EA 00` |
| 300 | 300000 | `E0 93 04` | `02 00 63 0F 01 00 00 E0 93 04` |
| 1200 | 1200000 | `80 4F 12` | `02 00 63 0F 01 00 00 80 4F 12` |

Source: `test_aerox5_wireless_wireless.py:131-138`.

### 4.7 `buttons_mapping` — (`command 6A`)

Profile: `aerox5_wireless_wired.py:119-149`; handler `rivalcfg/handlers/buttons/buttons.py:233-329`.

The value is a fixed **55-byte block**: 11 fields × 5 bytes. Each field: `[type/id, keycode, 00, 00, 00]`.

| Offset | Button | Default value bytes | Meaning |
|---:|---|---|---|
| `0x00` | Button1 | `01 00 00 00 00` | mouse button 1 |
| `0x05` | Button2 | `02 00 00 00 00` | mouse button 2 |
| `0x0A` | Button3 | `03 00 00 00 00` | mouse button 3 |
| `0x0F` | Button4 | `04 00 00 00 00` | mouse button 4 |
| `0x14` | Button5 | `05 00 00 00 00` | mouse button 5 |
| `0x19` | Button6 | `30 00 00 00 00` | DPI switch |
| `0x1E` | Button7 | `00 00 00 00 00` | disabled |
| `0x23` | Button8 | `00 00 00 00 00` | disabled |
| `0x28` | Button9 | `00 00 00 00 00` | disabled |
| `0x2D` | ScrollUp | `31 00 00 00 00` | scroll up (id `0x31`) |
| `0x32` | ScrollDown | `32 00 00 00 00` | scroll down (id `0x32`) |

**Physical layout** (derived from the labels in `doc/devices/images/aerox_5_wireless_buttons.svg:2573-2760`):

- Button1 = left mouse click (top-left), Button2 = right click (top-right), Button3 = middle click (below/right of wheel).
- ScrollUp / ScrollDown label the wheel directions, directly below Button3.
- **Button6 = the button under the mouse wheel**; its default mapping is `dpi`, which matches the documentation note "You can switch preset using the button under the mouse wheel" (`doc/devices/aerox5_wireless.rst:36`).
- Left side column, top to bottom: Button9, Button5, Button4, Button7 (→ / forward), Button8 (← / backward).
- Buttons 7, 8 and 9 are factory-disabled (`"default": "disabled"` in the profile).

Field type byte values (`aerox5_wireless_wired.py:141-146`):

| Type | Byte | Notes |
|---|---:|---|
| disabled | `0x00` | `button_disable` |
| mouse button | `0x01`–`0x06` | ids from the `buttons` dict |
| DPI switch | `0x30` | `button_dpi_switch` |
| scroll up / down | `0x31` / `0x32` | button ids, not special actions here |
| keyboard | `0x51` | byte 1 = HID keyboard usage code |
| multimedia | `0x61` | byte 1 = multimedia code |

Special actions: only `disable`/`disabled` → `0x00` and `dpi` → `0x30` (`buttons.py:267-272`); `button_scroll_up`/`button_scroll_down` are `None` in this profile (`:274-280`).

Keyboard key codes are standard HID usages (`rivalcfg/handlers/buttons/layout_qwerty.py:8-161`): A–Z `0x04`–`0x1D`, 1–0 `0x1E`–`0x27`, Enter `0x28`, Esc `0x29`, Backspace `0x2A`, Tab `0x2B`, Space `0x2C`, F1–F12 `0x3A`–`0x45`, arrows Right/Left/Down/Up `0x4F`–`0x52`, modifiers LeftCtrl/Shift/Alt/Super `0xE0`–`0xE3`, Right* `0xE4`–`0xE7`, etc. Multimedia codes (`layout_multimedia.py:6-13`): Mute `0xE2`, Next `0xB5`, PlayPause `0xCD`, Previous `0xB6`, VolumeUp `0xE9`, VolumeDown `0xEA`.

Examples: `buttons(button2=button6)` → field at `0x05` becomes `06 00 00 00 00`; `button1=A` → `51 04 00 00 00`; `button1=Mute` → `61 E2 00 00 00`; `button1=dpi` → `30 00 00 00 00`. Sources: `test_aerox5_wireless_wireless.py:151-213`, `test/handlers/test_buttons.py:116-212`.

### 4.8 `rainbow_effect` — (`command 62 FF`)

Profile: `aerox5_wireless_wired.py:150-157`; `value_type: none` → zero value bytes (`rivalcfg/handlers/none.py:52-60`).

On-wire: `00 62 FF`; test bytes: `02 00 62 FF` (`test_aerox5_wireless_wireless.py:224-229`). Wired: `00 22 FF` (`test_aerox5_wireless_wired.py:215-219`).

### 4.9 `default_lighting` — (`command 67`)

Profile: `aerox5_wireless_wired.py:158-172`, default `"rainbow"`.

| Value | 2 value bytes | On-wire (wireless) | Test bytes |
|---|---|---|---|
| `off` | `00 00` | `00 67 00 00` | `02 00 67 00 00` |
| `reactive` | `00 01` | `00 67 00 01` | `02 00 67 00 01` |
| `rainbow` | `01 00` | `00 67 01 00` | `02 00 67 01 00` |
| `reactive-rainbow` | `01 01` | `00 67 01 01` | `02 00 67 01 01` |

Byte 1 = rainbow flag, byte 2 = reactive flag (inference from the choice values). Source: `test_aerox5_wireless_wireless.py:231-237`.

---

## 5. DPI encoding table (`truemove_air`)

Full 180-entry mapping used by `sensitivity`, from `rivalcfg/devices/dpi/truemove_air.py:3-184`. Values not in the table are snapped to the nearest key before lookup.

| DPI | Byte | DPI | Byte | DPI | Byte | DPI | Byte | DPI | Byte | DPI | Byte |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
|100|00|200|02|300|03|400|04|500|05|600|06|
|700|07|800|09|900|0A|1000|0B|1100|0C|1200|0D|
|1300|0E|1400|10|1500|11|1600|12|1700|13|1800|14|
|1900|16|2000|17|2100|18|2200|19|2300|1A|2400|1B|
|2500|1D|2600|1E|2700|1F|2800|20|2900|21|3000|23|
|3100|25|3200|26|3300|27|3400|28|3500|29|3600|2A|
|3700|2C|3800|2D|3900|2E|4000|2F|4100|30|4200|32|
|4300|33|4400|34|4500|35|4600|36|4700|38|4800|39|
|4900|3A|5000|3B|5100|3C|5200|3E|5300|3F|5400|40|
|5500|41|5600|42|5700|44|5800|45|5900|46|6000|47|
|6100|48|6200|4A|6300|4B|6400|4C|6500|4D|6600|4E|
|6700|50|6800|51|6900|52|7000|53|7100|54|7200|56|
|7300|57|7400|58|7500|59|7600|5A|7700|5C|7800|5D|
|7900|5E|8000|5F|8100|60|8200|62|8300|63|8400|64|
|8500|65|8600|66|8700|68|8800|69|8900|6A|9000|6B|
|9100|6C|9200|6E|9300|6F|9400|70|9500|71|9600|72|
|9700|74|9800|75|9900|76|10000|77|10100|78|10200|7A|
|10300|7B|10400|7C|10500|7D|10600|7E|10700|80|10800|81|
|10900|82|11000|83|11100|84|11200|86|11300|87|11400|88|
|11500|89|11600|8A|11700|8C|11800|8D|11900|8E|12000|8F|
|12100|90|12200|92|12300|93|12400|94|12500|95|12600|96|
|12700|98|12800|99|12900|9A|13000|9B|13100|9C|13200|9E|
|13300|9F|13400|A0|13500|A1|13600|A2|13700|A4|13800|A5|
|13900|A6|14000|A7|14100|A8|14200|AA|14300|AB|14400|AC|
|14500|AD|14600|AE|14700|B0|14800|B1|14900|B2|15000|B3|
|15100|B4|15200|B5|15300|B6|15400|B7|15500|B8|15600|B9|
|15700|BA|15800|BB|15900|BC|16000|BD|16100|BF|16200|C0|
|16300|C2|16400|C3|16500|C4|16600|C5|16700|C6|16800|C7|
|16900|C9|17000|CA|17100|CB|17200|CC|17300|CD|17400|CF|
|17500|D0|17600|D1|17700|D2|17800|D3|17900|D5|18000|D6|

---

## 6. Battery level query

Profile: `aerox5_wireless_wired.py:4,174-180`; implementation `rivalcfg/mouse.py:158-205`.

```
Query:    00 92        (wired)   /  00 D2  (wireless)
Response: 2-byte read, timeout 200 ms
data[1] bit 7 (0x80)  -> charging flag
level = ((data[1] & 0x7F) - 1) * 5     -> percent
```

Code:

```python
_BATTERY_CHARGING_FLAG = 0b10000000
"battery_level": {
    "report_type": usbhid.HID_REPORT_TYPE_OUTPUT,
    "command": [0x92],
    "response_length": 2,
    "is_charging": lambda data: bool(data[1] & _BATTERY_CHARGING_FLAG),
    "level": lambda data: ((data[1] & ~_BATTERY_CHARGING_FLAG) - 1) * 5,
},
```

- `data[1]` is the second byte of the input report (byte 0 assumed to be the input report ID — inference).
- In Python, `data[1] & ~0x80` masks to the low 7 bits. Examples: low7 `0x14` (20) → 95 %; `0x15` (21) → 100 %; `0x00` or `0x80` → −5 → invalid.
- Invalid levels (<0 or >100) or decoder exceptions return `{"is_charging": None, "level": None}` (`mouse.py:188-205`), rendered by the CLI as *"Unable to get the battery level. Is the mouse turned on?"* (`__main__.py:47-59,107-116`).
- Tests: wired `02 00 92` (`test_aerox5_wireless_wired.py:236-240`), wireless `02 00 D2` (`test_aerox5_wireless_wireless.py:246-250`).
- The battery protocol was inherited from the Aerox 3 Wireless implementation (commit `7357c7d`). A known historical bug (issues #187/#212, fixed in commits `db9a426`/`a8c9b9d`/`5d81d7d`, 2023-06) reported "630 %" when the mouse was off: raw `0xFF` (charging bit + `0x7F`) yields `(127−1)×5 = 630`. The fix added the 0–100 bounds check; a later fix (`efdd8bf`, 2025-04) guarded against `None` values.

---

## 7. Save command (commit settings to onboard memory)

Profile: `aerox5_wireless_wired.py:181-184`; implementation `rivalcfg/mouse.py:221-252`.

```
Wired:    00 11 00
Wireless: 00 51 00    + 64-byte readback (200 ms timeout)
```

- No `packet_length`, so no padding.
- `mouse.save()` then persists local settings to `$XDG_CONFIG_HOME/rivalcfg/<vid>_<pid>.device.json` (default `~/.config`) (`mouse_settings.py:16-32,159-171`).
- CLI: save happens by default; `--no-save` skips it (`cli.py:104-110`, `__main__.py:132-134`).
- Tests: wired `02 00 11 00` (`test_aerox5_wireless_wired.py:244-248`), wireless `02 00 51 00` (`test_aerox5_wireless_wireless.py:254-258`).

---

## 8. Device-database defaults (legacy SteelSeries `ssdb/sse3.db.csv`)

These are factory/default configuration values from the bundled legacy database, not rivalcfg's own profile defaults. They document the device's onboard state and are useful for sanity-checking implementations.

For `1038,1854,1852` (and identically for the Destiny `185e/185c` and Diablo `1862/1860` pairs), the default JSON contains:

| Key | Value |
|---|---|
| `button_mappings` | 11 buttons: functions `1,2,3,4,5,48,81,81,81,49,50`; the three `81` entries have key codes `[58]`, `[59]`, `[60]`; `no_live_deploy: 1` |
| `batch_resolution` | `num_cpi_levels: 5`, `active_level_index: 0`, levels `4, 9, 13, 27, 38` (these are the encoded DPI bytes for 400/800/1200/2400/3200) |
| `polling_rate` | `{level: 0}` |
| `angle_snapping`, `exact_accel`, `exact_aim` | `{level: 0}` |
| `power_saving_timeout` | `{timeout: 300000}` (5 min, matches `sleep_timer` default) |
| `low_power_gaming_mode_sensor` | `{lpgm_mode_on: 0}` |
| `lighting_settings` | `brightness: 15`, `idle_brightness: 1`, `smart_lighting: 1`, `idle_timeout: 30000` |
| `default_lighting_enabled` | `{default_lighting_enabled: 1}` |
| `reactive_lighting` | `enable_custom_reactive: 1`, `enable_random_color: 0`, `color: {r:255,g:255,b:255}` |
| `aerox_5_wireless_z1/z2/z3_color_sequence` | 533-entry rainbow sequences (first colors `[255,0,17]`, `[255,0,51]`, `[255,0,85]`), `repeat: 255` |

For the **Diablo IV** edition (`1862/1860`), each zone sequence is a single `[255,0,0]` entry with `repeat: 255` — i.e. static red rather than rainbow.

For the wired-only Aerox 5 (`1038,1850`), the analogous keys use `set_all_resolution`, `global_brightness: 100`, and `aerox_5_z1/z2/z3_color_sequence`; polling `{level: 1}` (note the different polling encoding discussed in §4.2).

---

## 9. Differences between wired and 2.4 GHz wireless modes

| Aspect | Wired (1854/185E/1862) | Wireless (1852/185C/1860) |
|---|---|---|
| HID interface | `3` | `3` |
| Command byte bit 6 (`0x40`) | clear | set on **every** command |
| Write → readback | none | 64-byte input read, 200 ms timeout |
| Save | `00 11 00` | `00 51 00` + readback |
| Battery query | `00 92` (2-byte read) | `00 D2` (2-byte read; the injected 64-byte readback is unused) |
| All settings payloads | identical | identical |

The `0x40` bit is the only command-payload difference; there is no separate command set for the dongle.

---

## 10. Open questions / caveats

1. **Report ID semantics:** all writes use `0x00` as the first byte and all Aerox 5 commands are OUTPUT reports. Reads return a buffer whose byte 0 is assumed to be an input report ID; this is inference from the battery decoder, not explicitly documented.
2. **Readback contents:** the 64-byte wireless readback is discarded by the CLI, so rivalcfg does not document what those bytes contain.
3. **Windows interface numbers:** `endpoint: 3` relies on hidapi reporting USB interface numbers; possible `interface_number = -1` on Windows is unhandled (inference).
4. **Fixed 50 ms command delay:** required in practice by rivalcfg; the minimum allowed is 1 ms and lower values are warned against (`mouse.py:93-101`).
5. **`no_live_deploy`:** present in the legacy database but not used by rivalcfg at runtime.
6. **Wired-only Aerox 5 (`0x1850`) differences:** different polling-rate byte mapping and different LED command prefixes; do not reuse those commands for the Wireless model.

---

## 11. Documented behavior and caveats (from rivalcfg docs)

These are behavioral facts documented for users, not directly derivable from byte encodings.

### 11.1 Sensitivity presets
- Up to **5 presets**; the active preset is switched with the **button under the mouse wheel** (Button6, default `dpi`) — `doc/devices/aerox5_wireless.rst:26-36`.
- Setting sensitivity via the CLI **always resets the active preset to the first one** (index 0); the Python API can pass `selected_preset=...` to choose (`:38-51`). The SSDB default `batch_resolution.active_level_index: 0` corroborates this.
- The Rust GUI currently sends a single DPI value in a 64-byte example packet and should reproduce the `[count, preset, DPI-bytes…]` structure from §4.1.

### 11.2 Lighting
- Colors are **not saved in onboard memory** on these "newer SteelSeries mice" (`doc/devices/aerox5_wireless.rst:54-64`). Only the startup mode can be persisted via `--default-lighting`:
  - `off`: all LEDs off at wake-up; button clicks produce no light reaction.
  - `reactive`: LEDs off at wake-up; button clicks trigger a light reaction.
  - `rainbow`: animated rainbow at wake-up; no click reaction.
  - `reactive-rainbow`: animated rainbow at wake-up *and* click reactions.
  - Source: `doc/devices/_default_lighting_reactive.rst:1-15`, included at `aerox5_wireless.rst:74-77`.
- The **rainbow effect is onboard and volatile**: "The Aerox 5 offers an onboard rainbow light effect. Please note that the effect is reset if you change colors" (`aerox5_wireless.rst:67-71`). On this model the rainbow command is the one-shot `6x FF` flag; setting any zone color clears it.
- **Smart illumination is not supported** by rivalcfg on this device (`aerox5_wireless.rst:12-18`), although the SSDB default has `lighting_settings.smart_lighting: 1` — an Engine-side setting not exposed by rivalcfg.

### 11.3 Buttons
- `buttons()` parameter syntax: `buttons(layout=<LAYOUT>; button1=<mapping>; buttonN=<mapping>)`. Any omitted button resets to its default; `buttons` with no arguments resets all buttons (`doc/devices/_buttons.rst:8-29`).
- Warning from the docs: *"Be sure to map the mouse button `button1` on some button, else you will not be able to click with the mouse!"* (`:31-33`).
- Special actions: `disabled`, `dpi`, `ScrollUp`, `ScrollDown`. Mouse-button remapping (e.g. `button1=button2`), multimedia keys (`Mute`, `Next`, `PlayPause`, `Previous`, `VolumeUp`, `VolumeDown`) and keyboard key names are supported; only the `QWERTY` layout is available and is the default (`_buttons.rst:36-112`).
- `no_live_deploy: 1` in the SSDB rows means settings require the explicit save command rather than applying live; rivalcfg's `--no-save` skips the `11 00` / `51 00` save.

### 11.4 Battery behavior
- Battery query is defined only on the Wireless profiles (not on the wired-only `0x1850` model).
- Out-of-range or undecodable results are displayed as *"Unable to get the battery level. Is the mouse turned on?"* (`rivalcfg/__main__.py:47-59`).

---

## 12. Protocol provenance (git history)

Key commits in the rivalcfg repository that established or changed this protocol:

| Commit | Date | Relevance |
|---|---|---|
| `6f246cc` | 2022-08-22 | Initial Aerox 5 Wireless support (PR #184): created both profiles, battery query `0x92` with `0x80` charging flag and `((b & ~0x80)-1)*5` level, save `11 00`, and the wireless `_WIRELESS_FLAG = 0x40` / `readback_length = 64` patch. |
| `a5b8da1` | 2022-08-22 | Fixed a copy/paste bug: the wireless profile initially imported commands from `aerox3_wireless_wired`; corrected to `aerox5_wireless_wired`. |
| `007b4ab`, `7ebe61e` | 2022-08-22 | Added 11-button mapping table and byte-exact wired/wireless tests. |
| `b3be26a` | 2021-08-23 | Origin of the `_WIRELESS_FLAG = 0b01000000` patch pattern (Aerox 3 Wireless 2.4 GHz), later reused for Aerox 5/9 and Prime Wireless. |
| `7357c7d` | 2021-09-01 | Origin of the `0x92` battery protocol (Aerox 3 Wireless). |
| `869a20d`, `44bf27c` | 2021-11-24 | Introduced `readback_length` and its 200 ms post-write/save read mechanism. |
| `c72af6b`, `aeb8143` | 2022-12-27/28 | Added `--default-lighting` (`27 00/01`) and the doc explaining only startup lighting persists. |
| `56f4f6b`, `e4228b8` | 2023-05-23 | Added Destiny 2 (`185E/185C`) and Diablo IV (`1862/1860`) editions. |
| `db9a426`, `a8c9b9d`, `5d81d7d`, `efdd8bf` | 2023-06, 2025-04 | Battery "630 %" bug fixes and `None` handling (see §6). |
| `9145745`, `3c6d0de` | 2024-02 | Built the wired-only `aerox5` profile from Aerox 3 + Aerox 5 Wireless data. |
| `30e7258`, `7571126`, `a6bd645` | 2026-01-23 | Replaced linear DPI mapping with the `truemove_air` lookup table for all seven PIDs; added `dpi_length_byte: 1` (see §4.1). |
| `40b199f`, `3d4ef3b`, `5e340e3`, `fccc142` | 2022-2026 | Documentation: created the Aerox 5 Wireless page, added the button SVG, and added the missing Aerox 5 (wired) page. |

---

## 13. Source index

| Topic | Files |
|---|---|
| Device profiles | `rivalcfg/devices/aerox5_wireless_wired.py`, `aerox5_wireless_wireless.py`, `aerox5.py` |
| Profile generation / matching | `rivalcfg/devices/__init__.py:27-28,58-61,155-268` |
| HID open / enumeration | `rivalcfg/usbhid.py:26-30,33-50,53-97,128-189` |
| Report assembly / timing / reads | `rivalcfg/mouse.py:82-111,139-205,221-310,316-395` |
| Byte helpers | `rivalcfg/helpers.py:11-31,43-134` |
| Value handlers | `rivalcfg/handlers/{choice,range,range_choice,multidpi_range_choice,rgbcolor,reactive_rgbcolor,none}.py`, `rivalcfg/handlers/buttons/` |
| DPI table | `rivalcfg/devices/dpi/truemove_air.py:3-184` |
| Colors / key layouts | `rivalcfg/color_helpers.py`, `rivalcfg/handlers/buttons/layout_qwerty.py`, `layout_multimedia.py` |
| Byte-level tests | `test/devices/old_specs/test_aerox5_wireless_wired.py`, `test_aerox5_wireless_wireless.py`, `test/handlers/` |
| E2E spec format | `test/devices/README.rst:59-75`, `test/devices/specs/*.txt` |
| Environment/debug docs | `doc/env.rst:17-44` |
| Legacy defaults | `ssdb/sse3.db.csv:58,61,66-67` |
| Device/user docs | `doc/devices/aerox5_wireless.rst`, `doc/devices/aerox5.rst`, `doc/devices/_buttons.rst`, `doc/devices/_default_lighting_reactive.rst`, `doc/devices/_colors.rst`, `doc/devices/images/aerox_5_wireless_buttons.svg` |
| Changelog | `README.rst:78,93-100,113,121,152-164,168-185` |
| Protocol provenance | git history of the profile/doc files (§12) |
