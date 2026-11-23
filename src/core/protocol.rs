//! Aerox 5 protocol encoding helpers.
//!
//! All packet builders return the exact bytes written to interface 3, starting
//! with the HID report ID `0x00`. Wireless commands have bit 6 (`0x40`) set,
//! matching rivalcfg's `_WIRELESS_FLAG` patch (`com-spec.md` §3).

use serde::{Deserialize, Serialize};

pub const REPORT_ID: u8 = 0x00;

pub const DEFAULT_CPI: [u32; 5] = [400, 800, 1200, 2400, 3200];
pub const DEFAULT_SELECTED_LEVEL: usize = 2;
pub const DEFAULT_POLLING: u32 = 1000;
pub const DEFAULT_SLEEP_MINUTES: f32 = 5.0;
pub const DEFAULT_DIM_SECONDS: u32 = 60;
pub const POLLING_RATES: [u32; 4] = [125, 250, 500, 1000];

/// Persisted startup lighting effect (`docs/com-spec.md` §4.9). There is deliberately no `Off`
/// variant: the RGB toggle controls that and sends the separate `00 00` value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DefaultLighting {
    Reactive,
    #[default]
    Rainbow,
    ReactiveRainbow,
}

impl DefaultLighting {
    pub const ALL: [Self; 3] = [Self::Reactive, Self::Rainbow, Self::ReactiveRainbow];

    pub fn label(self) -> &'static str {
        match self {
            Self::Reactive => "reactive",
            Self::Rainbow => "rainbow",
            Self::ReactiveRainbow => "reactive-rainbow",
        }
    }

    /// Two value bytes: byte 1 is the rainbow flag, byte 2 the reactive flag.
    pub fn bytes(self) -> [u8; 2] {
        match self {
            Self::Reactive => [0x00, 0x01],
            Self::Rainbow => [0x01, 0x00],
            Self::ReactiveRainbow => [0x01, 0x01],
        }
    }

    /// Whether this effect uses button-click light reactions (the second flag).
    pub fn has_reactive(self) -> bool {
        self.bytes()[1] != 0
    }
}

/// HID transport used for a command. The dongle sets bit 6 of the command byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Wired,
    Dongle,
}

impl From<bool> for Link {
    fn from(dongle: bool) -> Self {
        if dongle { Self::Dongle } else { Self::Wired }
    }
}

/// Polling-rate encoding: the wired-only Aerox 5 PID uses a different code table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollingEncoding {
    Standard,
    WiredOnly,
}

impl From<bool> for PollingEncoding {
    fn from(wired_only: bool) -> Self {
        if wired_only {
            Self::WiredOnly
        } else {
            Self::Standard
        }
    }
}

/// `TrueMove` Air CPI table from `rivalcfg/devices/dpi/truemove_air.py`
/// (180 entries, verified against the installed rivalcfg checkout).
pub const TRUEMOVE_AIR: [(u32, u8); 180] = [
    (100, 0x00),
    (200, 0x02),
    (300, 0x03),
    (400, 0x04),
    (500, 0x05),
    (600, 0x06),
    (700, 0x07),
    (800, 0x09),
    (900, 0x0A),
    (1000, 0x0B),
    (1100, 0x0C),
    (1200, 0x0D),
    (1300, 0x0E),
    (1400, 0x10),
    (1500, 0x11),
    (1600, 0x12),
    (1700, 0x13),
    (1800, 0x14),
    (1900, 0x16),
    (2000, 0x17),
    (2100, 0x18),
    (2200, 0x19),
    (2300, 0x1A),
    (2400, 0x1B),
    (2500, 0x1D),
    (2600, 0x1E),
    (2700, 0x1F),
    (2800, 0x20),
    (2900, 0x21),
    (3000, 0x23),
    (3100, 0x25),
    (3200, 0x26),
    (3300, 0x27),
    (3400, 0x28),
    (3500, 0x29),
    (3600, 0x2A),
    (3700, 0x2C),
    (3800, 0x2D),
    (3900, 0x2E),
    (4000, 0x2F),
    (4100, 0x30),
    (4200, 0x32),
    (4300, 0x33),
    (4400, 0x34),
    (4500, 0x35),
    (4600, 0x36),
    (4700, 0x38),
    (4800, 0x39),
    (4900, 0x3A),
    (5000, 0x3B),
    (5100, 0x3C),
    (5200, 0x3E),
    (5300, 0x3F),
    (5400, 0x40),
    (5500, 0x41),
    (5600, 0x42),
    (5700, 0x44),
    (5800, 0x45),
    (5900, 0x46),
    (6000, 0x47),
    (6100, 0x48),
    (6200, 0x4A),
    (6300, 0x4B),
    (6400, 0x4C),
    (6500, 0x4D),
    (6600, 0x4E),
    (6700, 0x50),
    (6800, 0x51),
    (6900, 0x52),
    (7000, 0x53),
    (7100, 0x54),
    (7200, 0x56),
    (7300, 0x57),
    (7400, 0x58),
    (7500, 0x59),
    (7600, 0x5A),
    (7700, 0x5C),
    (7800, 0x5D),
    (7900, 0x5E),
    (8000, 0x5F),
    (8100, 0x60),
    (8200, 0x62),
    (8300, 0x63),
    (8400, 0x64),
    (8500, 0x65),
    (8600, 0x66),
    (8700, 0x68),
    (8800, 0x69),
    (8900, 0x6A),
    (9000, 0x6B),
    (9100, 0x6C),
    (9200, 0x6E),
    (9300, 0x6F),
    (9400, 0x70),
    (9500, 0x71),
    (9600, 0x72),
    (9700, 0x74),
    (9800, 0x75),
    (9900, 0x76),
    (10000, 0x77),
    (10100, 0x78),
    (10200, 0x7A),
    (10300, 0x7B),
    (10400, 0x7C),
    (10500, 0x7D),
    (10600, 0x7E),
    (10700, 0x80),
    (10800, 0x81),
    (10900, 0x82),
    (11000, 0x83),
    (11100, 0x84),
    (11200, 0x86),
    (11300, 0x87),
    (11400, 0x88),
    (11500, 0x89),
    (11600, 0x8A),
    (11700, 0x8C),
    (11800, 0x8D),
    (11900, 0x8E),
    (12000, 0x8F),
    (12100, 0x90),
    (12200, 0x92),
    (12300, 0x93),
    (12400, 0x94),
    (12500, 0x95),
    (12600, 0x96),
    (12700, 0x98),
    (12800, 0x99),
    (12900, 0x9A),
    (13000, 0x9B),
    (13100, 0x9C),
    (13200, 0x9E),
    (13300, 0x9F),
    (13400, 0xA0),
    (13500, 0xA1),
    (13600, 0xA2),
    (13700, 0xA4),
    (13800, 0xA5),
    (13900, 0xA6),
    (14000, 0xA7),
    (14100, 0xA8),
    (14200, 0xAA),
    (14300, 0xAB),
    (14400, 0xAC),
    (14500, 0xAD),
    (14600, 0xAE),
    (14700, 0xB0),
    (14800, 0xB1),
    (14900, 0xB2),
    (15000, 0xB3),
    (15100, 0xB4),
    (15200, 0xB5),
    (15300, 0xB6),
    (15400, 0xB7),
    (15500, 0xB8),
    (15600, 0xB9),
    (15700, 0xBA),
    (15800, 0xBB),
    (15900, 0xBC),
    (16000, 0xBD),
    (16100, 0xBF),
    (16200, 0xC0),
    (16300, 0xC2),
    (16400, 0xC3),
    (16500, 0xC4),
    (16600, 0xC5),
    (16700, 0xC6),
    (16800, 0xC7),
    (16900, 0xC9),
    (17000, 0xCA),
    (17100, 0xCB),
    (17200, 0xCC),
    (17300, 0xCD),
    (17400, 0xCF),
    (17500, 0xD0),
    (17600, 0xD1),
    (17700, 0xD2),
    (17800, 0xD3),
    (17900, 0xD5),
    (18000, 0xD6),
];

/// Nearest table entry. Ties go to the lower CPI, matching rivalcfg's
/// `range_choice` handler.
pub fn nearest_cpi(cpi: u32) -> (u32, u8) {
    let cpi = cpi.clamp(100, 18000);
    let mut best = TRUEMOVE_AIR[0];
    let mut best_diff = (i64::from(best.0) - i64::from(cpi)).abs();
    for &(table_cpi, byte) in TRUEMOVE_AIR.iter().skip(1) {
        let diff = (i64::from(table_cpi) - i64::from(cpi)).abs();
        if diff < best_diff {
            best = (table_cpi, byte);
            best_diff = diff;
        }
    }
    best
}

pub fn dpi_byte(cpi: u32) -> u8 {
    nearest_cpi(cpi).1
}

pub fn snap_cpi(cpi: u32) -> u32 {
    nearest_cpi(cpi).0
}

fn wireless_cmd(base: u8, link: Link) -> u8 {
    match link {
        Link::Wired => base,
        Link::Dongle => base | 0x40,
    }
}

fn push_u24_le(out: &mut Vec<u8>, value: u32) {
    out.push(u8::try_from(value & 0xFF).expect("masked byte fits u8"));
    out.push(u8::try_from((value >> 8) & 0xFF).expect("masked byte fits u8"));
    out.push(u8::try_from((value >> 16) & 0xFF).expect("masked byte fits u8"));
}

/// `[00, 6D|40?, count, selected_preset, encoded CPI…]`
pub fn sensitivity_packet(levels: &[u32], selected: usize, link: Link) -> Vec<u8> {
    let count = levels.len().min(5);
    let mut packet = Vec::with_capacity(4 + count);
    packet.push(REPORT_ID);
    packet.push(wireless_cmd(0x2D, link));
    packet.push(u8::try_from(count).expect("at most five CPI levels"));
    packet.push(
        u8::try_from(selected.min(count.saturating_sub(1))).expect("selected level fits a byte"),
    );
    packet.extend(levels.iter().take(count).map(|cpi| dpi_byte(*cpi)));
    packet
}

pub fn polling_code(rate: u32, encoding: PollingEncoding) -> u8 {
    if encoding == PollingEncoding::WiredOnly {
        match rate {
            125 => 0x04,
            250 => 0x03,
            500 => 0x02,
            _ => 0x01,
        }
    } else {
        match rate {
            125 => 0x03,
            250 => 0x02,
            500 => 0x01,
            _ => 0x00,
        }
    }
}

/// `[00, 6B|40?, code]`
pub fn polling_packet(rate: u32, encoding: PollingEncoding, link: Link) -> Vec<u8> {
    vec![
        REPORT_ID,
        wireless_cmd(0x2B, link),
        polling_code(rate, encoding),
    ]
}

/// `[00, 69|40?, ms LE24]`, where ms = minutes × 60000.
pub fn sleep_packet(minutes: f32, link: Link) -> Vec<u8> {
    // Make the u24 precondition local: non-finite values become 0, finite values are clamped.
    let minutes = if minutes.is_finite() {
        minutes.clamp(0.0, 20.0)
    } else {
        0.0
    };
    let mut packet = vec![REPORT_ID, wireless_cmd(0x29, link)];
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "minutes is clamped to 0..=20 here, so the rounded millisecond value fits u24"
    )]
    let millis = (minutes * 60_000.0).round() as u32;
    push_u24_le(&mut packet, millis);
    packet
}

/// Largest whole-second value whose millisecond conversion still fits in 24 bits.
const MAX_DIM_SECONDS: u32 = 0x00FF_FFFF / 1_000;

/// `[00, 63|40?, 0F 01 00 00, ms LE24]`, where ms = seconds × 1000.
pub fn dim_packet(seconds: u32, link: Link) -> Vec<u8> {
    let seconds = seconds.min(MAX_DIM_SECONDS);
    let mut packet = vec![REPORT_ID, wireless_cmd(0x23, link), 0x0F, 0x01, 0x00, 0x00];
    push_u24_le(&mut packet, seconds.saturating_mul(1_000));
    packet
}

/// `[00, 61|40?, 01, zone, R, G, B]`
pub fn color_packet(zone: u8, rgb: [u8; 3], link: Link) -> Vec<u8> {
    vec![
        REPORT_ID,
        wireless_cmd(0x21, link),
        0x01,
        zone,
        rgb[0],
        rgb[1],
        rgb[2],
    ]
}

/// `[00, 26|40?, 00 00 00 00 00]` for off, `[00, 26|40?, 01 00 R G B]` for a reaction colour.
///
/// `docs/com-spec.md` §4.4: rivalcfg's default is `off`. A stale enabled reactive colour keeps
/// button-click light reactions alive even after zone colours and default lighting are cleared, so
/// SAVE must write this explicitly on every apply.
pub fn reactive_color_packet(color: Option<[u8; 3]>, link: Link) -> Vec<u8> {
    let [flag, red, green, blue] = color.map_or([0x00, 0x00, 0x00, 0x00], |rgb| {
        [0x01, rgb[0], rgb[1], rgb[2]]
    });
    vec![
        REPORT_ID,
        wireless_cmd(0x26, link),
        flag,
        0x00,
        red,
        green,
        blue,
    ]
}

/// `[00, 22 FF]` wired / `[00, 62 FF]` wireless (`docs/com-spec.md` §4.8).
///
/// Volatile one-shot live rainbow. Setting any zone colour clears it, and writing it after a colour
/// write overrides those colours. SAVE sends it only when no live zone colours are selected; unlike
/// `default_lighting`, it applies immediately instead of at wake-up.
pub fn rainbow_effect_packet(link: Link) -> Vec<u8> {
    vec![REPORT_ID, wireless_cmd(0x22, link), 0xFF]
}

/// Scales an sRGB colour to `brightness_percent` (0–100), rounding each channel to nearest.
///
/// The wireless Aerox 5 profile has no brightness command, so the zone brightness slider
/// dims the RGB values themselves. 100 keeps the colour, 0 produces black.
pub fn scaled_color(rgb: [u8; 3], brightness_percent: u8) -> [u8; 3] {
    let percent = u16::from(brightness_percent.min(100));
    rgb.map(|channel| {
        let scaled = (u16::from(channel) * percent + 50) / 100;
        u8::try_from(scaled).expect("a scaled channel stays within 0..=255")
    })
}

/// `[00, 67|40? / 27, flag1, flag2]`
///
/// `docs/com-spec.md` §4.9: `None` is `off` (`00 00`), `Some(mode)` is that mode's two flags.
/// Setting any zone colour clears the onboard effect, so SAVE re-sends this packet after the
/// colour packets on every apply, not only when the selected mode changes.
pub fn default_lighting_packet(mode: Option<DefaultLighting>, link: Link) -> Vec<u8> {
    let [flag1, flag2] = mode.map_or([0x00, 0x00], DefaultLighting::bytes);
    vec![REPORT_ID, wireless_cmd(0x27, link), flag1, flag2]
}

/// `[00, 51|40? / 11, 00]`
pub fn save_packet(link: Link) -> Vec<u8> {
    vec![REPORT_ID, wireless_cmd(0x11, link), 0x00]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_unique() {
        assert_eq!(TRUEMOVE_AIR.len(), 180);
        for pair in TRUEMOVE_AIR.windows(2) {
            assert!(pair[0].0 < pair[1].0);
        }
    }

    #[test]
    fn known_cpi_encodings_match_spec() {
        assert_eq!(dpi_byte(100), 0x00);
        assert_eq!(dpi_byte(200), 0x02);
        assert_eq!(dpi_byte(400), 0x04);
        assert_eq!(dpi_byte(800), 0x09);
        assert_eq!(dpi_byte(1500), 0x11);
        assert_eq!(dpi_byte(18000), 0xD6);
    }

    #[test]
    fn sensitivity_packet_matches_spec_examples() {
        assert_eq!(
            sensitivity_packet(&[200, 400, 800, 1600], 0, Link::Dongle),
            vec![0x00, 0x6D, 0x04, 0x00, 0x02, 0x04, 0x09, 0x12]
        );
        assert_eq!(
            sensitivity_packet(&[1500], 0, Link::Wired),
            vec![0x00, 0x2D, 0x01, 0x00, 0x11]
        );
    }

    #[test]
    fn has_reactive_matches_the_second_effect_flag() {
        assert!(!DefaultLighting::Rainbow.has_reactive());
        assert!(DefaultLighting::Reactive.has_reactive());
        assert!(DefaultLighting::ReactiveRainbow.has_reactive());
    }

    #[test]
    fn default_lighting_packet_matches_spec() {
        assert_eq!(
            default_lighting_packet(None, Link::Wired),
            vec![0x00, 0x27, 0x00, 0x00]
        );
        assert_eq!(
            default_lighting_packet(Some(DefaultLighting::Reactive), Link::Wired),
            vec![0x00, 0x27, 0x00, 0x01]
        );
        assert_eq!(
            default_lighting_packet(Some(DefaultLighting::Rainbow), Link::Wired),
            vec![0x00, 0x27, 0x01, 0x00]
        );
        assert_eq!(
            default_lighting_packet(Some(DefaultLighting::ReactiveRainbow), Link::Wired),
            vec![0x00, 0x27, 0x01, 0x01]
        );
        assert_eq!(
            default_lighting_packet(None, Link::Dongle),
            vec![0x00, 0x67, 0x00, 0x00]
        );
        assert_eq!(
            default_lighting_packet(Some(DefaultLighting::Reactive), Link::Dongle),
            vec![0x00, 0x67, 0x00, 0x01]
        );
        assert_eq!(
            default_lighting_packet(Some(DefaultLighting::Rainbow), Link::Dongle),
            vec![0x00, 0x67, 0x01, 0x00]
        );
        assert_eq!(
            default_lighting_packet(Some(DefaultLighting::ReactiveRainbow), Link::Dongle),
            vec![0x00, 0x67, 0x01, 0x01]
        );
    }

    #[test]
    fn reactive_color_and_rainbow_packets_match_spec() {
        assert_eq!(
            reactive_color_packet(None, Link::Wired),
            vec![0x00, 0x26, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            reactive_color_packet(None, Link::Dongle),
            vec![0x00, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            reactive_color_packet(Some([0xFF, 0x18, 0x02]), Link::Wired),
            vec![0x00, 0x26, 0x01, 0x00, 0xFF, 0x18, 0x02]
        );
        assert_eq!(
            reactive_color_packet(Some([0xFF, 0x18, 0x02]), Link::Dongle),
            vec![0x00, 0x66, 0x01, 0x00, 0xFF, 0x18, 0x02]
        );
        assert_eq!(rainbow_effect_packet(Link::Wired), vec![0x00, 0x22, 0xFF]);
        assert_eq!(rainbow_effect_packet(Link::Dongle), vec![0x00, 0x62, 0xFF]);
    }

    #[test]
    fn default_lighting_labels_and_default_match_spec() {
        assert_eq!(DefaultLighting::default(), DefaultLighting::Rainbow);
        assert_eq!(
            DefaultLighting::ALL.map(DefaultLighting::label),
            ["reactive", "rainbow", "reactive-rainbow"]
        );
        assert_eq!(
            DefaultLighting::ALL.map(DefaultLighting::bytes),
            [[0, 1], [1, 0], [1, 1]]
        );
    }

    #[test]
    fn brightness_scales_colours_and_clamps() {
        let color = [255, 43, 0];
        assert_eq!(scaled_color(color, 100), color);
        assert_eq!(scaled_color(color, 50), [128, 22, 0]);
        assert_eq!(scaled_color(color, 0), [0, 0, 0]);
        assert_eq!(scaled_color(color, 200), color, "above 100 clamps to 100");
    }

    #[test]
    fn sleep_packet_clamps_out_of_range_and_non_finite_minutes() {
        assert_eq!(
            sleep_packet(f32::NAN, Link::Wired),
            vec![0x00, 0x29, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            sleep_packet(-1.0, Link::Wired),
            vec![0x00, 0x29, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            sleep_packet(100.0, Link::Wired),
            vec![0x00, 0x29, 0x80, 0x4F, 0x12]
        );
    }

    #[test]
    fn dim_packet_clamps_oversized_seconds() {
        let expected = vec![0x00, 0x23, 0x0F, 0x01, 0x00, 0x00, 0x28, 0xFF, 0xFF];
        assert_eq!(dim_packet(MAX_DIM_SECONDS, Link::Wired), expected);
        assert_eq!(dim_packet(MAX_DIM_SECONDS + 1, Link::Wired), expected);
        assert_eq!(dim_packet(u32::MAX, Link::Wired), expected);
    }

    #[test]
    fn timers_and_polling_match_spec() {
        assert_eq!(
            sleep_packet(5.0, Link::Dongle),
            vec![0x00, 0x69, 0xE0, 0x93, 0x04]
        );
        assert_eq!(
            sleep_packet(5.5, Link::Dongle),
            vec![0x00, 0x69, 0x10, 0x09, 0x05]
        );
        assert_eq!(
            dim_packet(30, Link::Dongle),
            vec![0x00, 0x63, 0x0F, 0x01, 0x00, 0x00, 0x30, 0x75, 0x00]
        );
        assert_eq!(
            polling_packet(1000, PollingEncoding::Standard, Link::Dongle),
            vec![0x00, 0x6B, 0x00]
        );
        assert_eq!(
            polling_packet(125, PollingEncoding::WiredOnly, Link::Dongle),
            vec![0x00, 0x6B, 0x04]
        );
    }
}
