//! JSON storage for app-local named configurations.
//!
//! Every parameter SAVE sends to the mouse lives in one versioned JSON file: CPI level slots,
//! count, selected level, polling rate, sleep/dim timers and the three illumination colours
//! (plus the live-colour switch and local brightness).
//! Boot only *reads* it (falls back to factory defaults with a status message); writes happen on
//! explicit user actions: SAVE, or a confirmed switch/create/delete in the CONFIGS window.
//!
//! No documented protocol command manages named profiles on the device (`docs/com-spec.md` §8),
//! so this store is purely local app state.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::protocol::DefaultLighting;

/// Schema version written to and required by `config.json`.
pub const CONFIG_FILE_VERSION: u32 = 1;
/// Env override used by tests/manual verification so real settings are not touched.
pub const CONFIG_PATH_ENV: &str = "AEROX5_GUI_CONFIG";
/// Directory name used below `$HOME` on non-macOS platforms.
pub const FALLBACK_DIR: &str = ".aerox5-gui";
/// Factory illumination colours (top/mid/bot) — shared by boot, REVERT and fresh config blocks.
pub const DEFAULT_COLORS: [[u8; 3]; 3] = [[255, 43, 214], [70, 118, 255], [0, 220, 255]];
/// Neutral reactive colour for fresh blocks and old config files without the key.
pub const DEFAULT_REACTIVE_COLOR: [u8; 3] = [255, 255, 255];

/// Old config files have no `rgb_enabled` key; they must keep lighting enabled.
fn rgb_enabled_by_default() -> bool {
    true
}
/// Old config files have no `live_zone_colors` key; they must keep sending zone colours.
fn live_zone_colors_by_default() -> bool {
    true
}
/// Unscaled colours are the historical behaviour for files without a brightness key.
fn zone_brightness_by_default() -> u8 {
    100
}
/// Old config files have no reactive colour; use a neutral white.
fn reactive_color_by_default() -> [u8; 3] {
    DEFAULT_REACTIVE_COLOR
}
/// Documented input ranges from `docs/com-spec.md` §§4.5–4.6, also used by the UI sliders.
const MAX_SLEEP_MINUTES: f32 = 20.0;
const MAX_DIM_SECONDS: u32 = 1_200;

/// One named configuration: all settings the app can send to the mouse in one block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigBlock {
    pub name: String,
    /// All CPI slots are stored; entries past `cpi_count` are stale leftovers from removed levels.
    pub cpi_levels: Vec<u32>,
    /// Number of active slots (1..=5, and <= `cpi_levels.len()` after validation).
    pub cpi_count: usize,
    /// Index into `cpi_levels[..cpi_count]` (0-based; the UI displays LEVEL index+1).
    pub selected_level: usize,
    /// Polling rate in Hz (125/250/500/1000 in the UI).
    pub polling_rate: u32,
    /// Sleep timer in minutes, including half-minute steps (e.g. `5.5`).
    pub sleep_minutes: f32,
    /// Illumination dim timer in seconds.
    pub dim_seconds: u32,
    /// Whether zone RGB lighting is enabled. Older files without the key stay enabled.
    #[serde(default = "rgb_enabled_by_default")]
    pub rgb_enabled: bool,
    /// Whether live zone colour packets are sent. Older files default to sending them.
    #[serde(default = "live_zone_colors_by_default")]
    pub live_zone_colors: bool,
    /// Local brightness (0–100) applied to the three zone colours before sending.
    #[serde(default = "zone_brightness_by_default")]
    pub zone_brightness: u8,
    /// Startup effect sent with the `27`/`67` command. Older files default to `rainbow`.
    #[serde(default)]
    pub startup_lighting: DefaultLighting,
    /// Colour used for button-click reactions (`26`/`66`). Independent of `color_top`.
    #[serde(default = "reactive_color_by_default")]
    pub reactive_color: [u8; 3],
    pub color_top: [u8; 3],
    pub color_mid: [u8; 3],
    pub color_bot: [u8; 3],
}

impl Default for ConfigBlock {
    fn default() -> Self {
        Self::named("Default")
    }
}

impl ConfigBlock {
    /// Factory defaults under a given name — the single source of truth used by boot, REVERT
    /// and the creation of new configurations (which then copy current on-screen values).
    pub fn named(name: &str) -> Self {
        Self {
            name: name.trim().to_owned(),
            cpi_levels: crate::core::protocol::DEFAULT_CPI.to_vec(),
            cpi_count: crate::core::protocol::DEFAULT_CPI.len(),
            selected_level: crate::core::protocol::DEFAULT_SELECTED_LEVEL,
            polling_rate: crate::core::protocol::DEFAULT_POLLING,
            sleep_minutes: crate::core::protocol::DEFAULT_SLEEP_MINUTES,
            dim_seconds: crate::core::protocol::DEFAULT_DIM_SECONDS,
            rgb_enabled: true,
            live_zone_colors: true,
            zone_brightness: 100,
            startup_lighting: DefaultLighting::Rainbow,
            reactive_color: DEFAULT_REACTIVE_COLOR,
            color_top: DEFAULT_COLORS[0],
            color_mid: DEFAULT_COLORS[1],
            color_bot: DEFAULT_COLORS[2],
        }
    }
}

/// Top-level JSON structure: schema version, name of the active block, and all blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigStore {
    pub version: u32,
    pub active: String,
    pub configurations: Vec<ConfigBlock>,
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self::factory()
    }
}

impl ConfigStore {
    /// The built-in configuration store; created in memory only until an explicit save action.
    pub fn factory() -> Self {
        Self {
            version: CONFIG_FILE_VERSION,
            active: "Default".to_owned(),
            configurations: vec![ConfigBlock::named("Default")],
        }
    }

    pub fn active_block(&self) -> Option<&ConfigBlock> {
        self.configurations.iter().find(|b| b.name == self.active)
    }

    pub fn active_block_mut(&mut self) -> Option<&mut ConfigBlock> {
        self.configurations
            .iter_mut()
            .find(|block| block.name == self.active)
    }

    /// Selects an existing configuration by name.
    pub fn switch_active(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("configuration name is empty".to_owned());
        }
        if !self.configurations.iter().any(|block| block.name == name) {
            return Err(format!("configuration '{name}' does not exist"));
        }
        name.clone_into(&mut self.active);
        Ok(())
    }

    /// Adds a block (named from `name`) and makes it active; rejects blank/duplicate names.
    pub fn create(&mut self, name: &str, block: ConfigBlock) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("configuration name is empty".to_owned());
        }
        if self
            .configurations
            .iter()
            .any(|existing| existing.name == name)
        {
            return Err(format!("configuration '{name}' already exists"));
        }
        let mut block = block;
        name.clone_into(&mut block.name);
        self.configurations.push(block);
        name.clone_into(&mut self.active);
        Ok(())
    }

    /// Removes a custom block. The built-in Default and the active block are protected.
    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        if name == "Default" {
            return Err("the Default configuration cannot be deleted".to_owned());
        }
        if name == self.active {
            return Err("select another configuration before deleting this one".to_owned());
        }
        let before = self.configurations.len();
        self.configurations.retain(|block| block.name != name);
        if self.configurations.len() == before {
            return Err(format!("configuration '{name}' does not exist"));
        }
        Ok(())
    }

    /// Structural errors (wrong version, bad/duplicate names, active pointing nowhere) are hard
    /// failures. Correctable oddities (wrong counts/selection/CPI range) are sanitized in place
    /// and returned as warnings for the caller to surface in the status line.
    pub fn validate(&mut self) -> Result<Vec<String>, String> {
        if self.version != CONFIG_FILE_VERSION {
            return Err(format!(
                "unsupported config format version {} (expected {})",
                self.version, CONFIG_FILE_VERSION
            ));
        }
        if self.configurations.is_empty() {
            return Err("config has no named configurations".to_owned());
        }

        let mut names = Vec::new();
        let mut warnings = Vec::new();
        for block in &mut self.configurations {
            let trimmed = block.name.trim().to_owned();
            if trimmed.is_empty() {
                return Err("a configuration has an empty name".to_owned());
            }
            if names.contains(&trimmed) {
                return Err(format!("duplicate configuration name: {trimmed}"));
            }
            names.push(trimmed.clone());
            block.name = trimmed;
            sanitize_block(block, &mut warnings);
        }

        self.active = self.active.trim().to_owned();
        if self.active.is_empty() || !names.contains(&self.active) {
            return Err(format!(
                "active configuration '{}' is missing from the saved configurations",
                self.active
            ));
        }
        Ok(warnings)
    }
}

fn sanitize_block(block: &mut ConfigBlock, warnings: &mut Vec<String>) {
    let name = block.name.clone();
    if block.cpi_levels.is_empty() {
        block.cpi_levels = crate::core::protocol::DEFAULT_CPI.to_vec();
        warnings.push(format!(
            "{name}: CPI levels were empty; restored factory levels"
        ));
    }
    if block.cpi_levels.len() > 5 {
        warnings.push(format!("{name}: dropped CPI slots beyond five"));
        block.cpi_levels.truncate(5);
    }
    for (index, cpi) in block.cpi_levels.iter_mut().enumerate() {
        let clamped = (*cpi).clamp(100, 18000);
        if clamped != *cpi {
            warnings.push(format!(
                "{name}: CPI slot {} clamped from {cpi} to {clamped}",
                index + 1
            ));
            *cpi = clamped;
        }
    }
    if block.cpi_count < 1 {
        warnings.push(format!(
            "{name}: cpi_count was {}, using 1",
            block.cpi_count
        ));
        block.cpi_count = 1;
    }
    if block.cpi_count > block.cpi_levels.len() {
        warnings.push(format!(
            "{name}: cpi_count {} exceeds the {} stored slots, using {}",
            block.cpi_count,
            block.cpi_levels.len(),
            block.cpi_levels.len()
        ));
        block.cpi_count = block.cpi_levels.len();
    }
    if block.selected_level >= block.cpi_count {
        warnings.push(format!(
            "{name}: selected level {} out of range, using {}",
            block.selected_level,
            block.cpi_count - 1
        ));
        block.selected_level = block.cpi_count - 1;
    }
    if !crate::core::protocol::POLLING_RATES.contains(&block.polling_rate) {
        warnings.push(format!(
            "{name}: polling rate {} replaced with {}",
            block.polling_rate,
            crate::core::protocol::DEFAULT_POLLING
        ));
        block.polling_rate = crate::core::protocol::DEFAULT_POLLING;
    }
    // The device documents 0..20 minutes and 0..1200 seconds; values outside that range
    // cannot be shown by the UI sliders and are rejected by rivalcfg's input handlers.
    if !block.sleep_minutes.is_finite() || block.sleep_minutes < 0.0 {
        warnings.push(format!(
            "{name}: invalid sleep timer {}; using 0 minutes",
            block.sleep_minutes
        ));
        block.sleep_minutes = 0.0;
    } else if block.sleep_minutes > MAX_SLEEP_MINUTES {
        warnings.push(format!(
            "{name}: sleep timer {} clamped to {MAX_SLEEP_MINUTES} minutes",
            block.sleep_minutes
        ));
        block.sleep_minutes = MAX_SLEEP_MINUTES;
    }
    if block.dim_seconds > MAX_DIM_SECONDS {
        warnings.push(format!(
            "{name}: dim timer {} clamped to {MAX_DIM_SECONDS} seconds",
            block.dim_seconds
        ));
        block.dim_seconds = MAX_DIM_SECONDS;
    }
    if block.zone_brightness > 100 {
        warnings.push(format!(
            "{name}: zone brightness {} clamped to 100",
            block.zone_brightness
        ));
        block.zone_brightness = 100;
    }
}

/// Resolves the config path: explicit env override wins; otherwise platform default.
/// Never creates directories or files — that only happens in `save`.
pub fn config_path() -> PathBuf {
    if let Some(overridden) = std::env::var_os(CONFIG_PATH_ENV) {
        return PathBuf::from(overridden);
    }
    if let Some(home) = std::env::var_os("HOME") {
        #[cfg(target_os = "macos")]
        {
            return PathBuf::from(home)
                .join("Library/Application Support/Aerox 5 Control")
                .join("config.json");
        }
        #[cfg(not(target_os = "macos"))]
        {
            return PathBuf::from(home).join(FALLBACK_DIR).join("config.json");
        }
    }
    PathBuf::from(FALLBACK_DIR).join("config.json")
}

/// Parses and validates JSON text. A missing file is handled by the caller via `load`.
pub fn parse(text: &str) -> Result<(ConfigStore, Vec<String>), String> {
    let mut store: ConfigStore =
        serde_json::from_str(text).map_err(|error| format!("invalid config JSON: {error}"))?;
    let warnings = store.validate()?;
    Ok((store, warnings))
}

/// Reads the config file. `Ok(None)` means it does not exist yet (fresh start, no disk write).
pub fn load(path: &Path) -> Result<Option<(ConfigStore, Vec<String>)>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not read {}: {error}", path.display())),
    };
    parse(&text)
        .map(Some)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))
}

/// Validates the store in place, then atomically writes it: encode + temp file in the same
/// directory + rename. Returns nonfatal sanitization warnings; hard errors abort the write.
/// Parent directories are created only here, never on boot or load.
pub fn save(path: &Path, store: &mut ConfigStore) -> Result<Vec<String>, String> {
    let warnings = store.validate()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(store)
        .map_err(|error| format!("could not encode config JSON: {error}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, format!("{json}\n"))
        .map_err(|error| format!("could not write {}: {error}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|error| {
        let _ = std::fs::remove_file(&tmp);
        format!("could not replace {}: {error}", path.display())
    })?;
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_defaults_match_todays_hardcoded_values() {
        let block = ConfigBlock::named("Default");
        assert_eq!(block.cpi_levels, vec![400, 800, 1200, 2400, 3200]);
        assert_eq!(block.cpi_count, 5);
        assert_eq!(block.selected_level, 2);
        assert_eq!(block.polling_rate, 1000);
        assert_eq!(block.sleep_minutes, 5.0);
        assert_eq!(block.dim_seconds, 60);
        assert!(block.rgb_enabled);
        assert!(block.live_zone_colors);
        assert_eq!(block.zone_brightness, 100);
        assert_eq!(block.startup_lighting, DefaultLighting::Rainbow);
        assert_eq!(block.reactive_color, [255, 255, 255]);
        assert_eq!(block.color_top, [255, 43, 214]);
        assert_eq!(block.color_mid, [70, 118, 255]);
        assert_eq!(block.color_bot, [0, 220, 255]);
    }

    #[test]
    fn json_round_trip_preserves_every_parameter() {
        let mut store = ConfigStore::factory();
        store.active_block_mut().unwrap().sleep_minutes = 5.5;
        store.active_block_mut().unwrap().dim_seconds = 330;
        let text = serde_json::to_string(&store).unwrap();
        assert!(text.contains("\"cpi_levels\""));
        assert!(text.contains("\"sleep_minutes\":5.5"));
        let (reloaded, warnings) = parse(&text).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(reloaded, store);
        assert_eq!(reloaded.active_block().unwrap().sleep_minutes, 5.5);
    }

    #[test]
    fn rgb_enabled_defaults_to_on_and_round_trips_off() {
        let legacy = r#"{"version":1,"active":"Default","configurations":[{"name":"Default","cpi_levels":[400],"cpi_count":1,"selected_level":0,"polling_rate":1000,"sleep_minutes":5,"dim_seconds":60,"color_top":[1,2,3],"color_mid":[4,5,6],"color_bot":[7,8,9]}]}"#;
        let (store, warnings) = parse(legacy).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        assert!(store.active_block().unwrap().rgb_enabled);

        let mut store = ConfigStore::factory();
        store.active_block_mut().unwrap().rgb_enabled = false;
        let text = serde_json::to_string(&store).unwrap();
        let (reloaded, warnings) = parse(&text).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        assert!(!reloaded.active_block().unwrap().rgb_enabled);
    }

    #[test]
    fn startup_lighting_defaults_to_rainbow_and_round_trips() {
        let legacy = r#"{"version":1,"active":"Default","configurations":[{"name":"Default","cpi_levels":[400],"cpi_count":1,"selected_level":0,"polling_rate":1000,"sleep_minutes":5,"dim_seconds":60,"color_top":[1,2,3],"color_mid":[4,5,6],"color_bot":[7,8,9]}]}"#;
        let (store, warnings) = parse(legacy).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(
            store.active_block().unwrap().startup_lighting,
            DefaultLighting::Rainbow
        );

        let mut store = ConfigStore::factory();
        store.active_block_mut().unwrap().startup_lighting = DefaultLighting::ReactiveRainbow;
        let text = serde_json::to_string(&store).unwrap();
        assert!(text.contains(r#""startup_lighting":"reactive-rainbow""#));
        let (reloaded, warnings) = parse(&text).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(
            reloaded.active_block().unwrap().startup_lighting,
            DefaultLighting::ReactiveRainbow
        );
    }

    #[test]
    fn reactive_color_defaults_to_white_and_round_trips() {
        let legacy = r#"{"version":1,"active":"Default","configurations":[{"name":"Default","cpi_levels":[400],"cpi_count":1,"selected_level":0,"polling_rate":1000,"sleep_minutes":5,"dim_seconds":60,"color_top":[1,2,3],"color_mid":[4,5,6],"color_bot":[7,8,9]}]}"#;
        let (store, warnings) = parse(legacy).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(
            store.active_block().unwrap().reactive_color,
            [255, 255, 255]
        );

        let mut store = ConfigStore::factory();
        store.active_block_mut().unwrap().reactive_color = [9, 8, 7];
        let text = serde_json::to_string(&store).unwrap();
        assert!(text.contains(r#""reactive_color":[9,8,7]"#));
        let (reloaded, warnings) = parse(&text).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(reloaded.active_block().unwrap().reactive_color, [9, 8, 7]);
    }

    #[test]
    fn live_zone_colors_and_brightness_default_and_round_trip() {
        let legacy = r#"{"version":1,"active":"Default","configurations":[{"name":"Default","cpi_levels":[400],"cpi_count":1,"selected_level":0,"polling_rate":1000,"sleep_minutes":5,"dim_seconds":60,"color_top":[1,2,3],"color_mid":[4,5,6],"color_bot":[7,8,9]}]}"#;
        let (store, warnings) = parse(legacy).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        let block = store.active_block().unwrap();
        assert!(block.live_zone_colors);
        assert_eq!(block.zone_brightness, 100);

        let mut store = ConfigStore::factory();
        store.active_block_mut().unwrap().live_zone_colors = false;
        store.active_block_mut().unwrap().zone_brightness = 42;
        let text = serde_json::to_string(&store).unwrap();
        let (reloaded, warnings) = parse(&text).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        let block = reloaded.active_block().unwrap();
        assert!(!block.live_zone_colors);
        assert_eq!(block.zone_brightness, 42);
    }

    #[test]
    fn brightness_above_100_is_clamped_with_a_warning() {
        let text = r#"{
            "version": 1,
            "active": "Default",
            "configurations": [{
                "name": "Default",
                "cpi_levels": [400],
                "cpi_count": 1,
                "selected_level": 0,
                "polling_rate": 1000,
                "sleep_minutes": 5,
                "dim_seconds": 60,
                "zone_brightness": 250,
                "color_top": [1,2,3],
                "color_mid": [4,5,6],
                "color_bot": [7,8,9]
            }]
        }"#;
        let (store, warnings) = parse(text).unwrap();
        assert_eq!(store.active_block().unwrap().zone_brightness, 100);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("brightness"))
        );
    }

    #[test]
    fn legacy_whole_minute_sleep_values_still_load() {
        let text = r#"{"version":1,"active":"Default","configurations":[{"name":"Default","cpi_levels":[400],"cpi_count":1,"selected_level":0,"polling_rate":1000,"sleep_minutes":5,"dim_seconds":60,"color_top":[1,2,3],"color_mid":[4,5,6],"color_bot":[7,8,9]}]}"#;
        let (store, warnings) = parse(text).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(store.active_block().unwrap().sleep_minutes, 5.0);
    }

    #[test]
    fn load_parse_errors_include_the_config_path() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aerox5-config-error-test-{}-{unique}.json",
            std::process::id()
        ));
        std::fs::write(&path, "{not json").unwrap();
        let error = load(&path).unwrap_err();
        assert!(
            error.contains(&path.display().to_string()),
            "the parse error must name the file that failed: {error}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn hard_failures_are_reported() {
        assert!(parse("not json").is_err());
        let bad_version = r#"{"version":2,"active":"Default","configurations":[{"name":"Default","cpi_levels":[400],"cpi_count":1,"selected_level":0,"polling_rate":1000,"sleep_minutes":5,"dim_seconds":60,"color_top":[1,2,3],"color_mid":[4,5,6],"color_bot":[7,8,9]}]}"#;
        assert!(parse(bad_version).unwrap_err().contains("version 2"));
        let block = r#"{"name":"Default","cpi_levels":[400],"cpi_count":1,"selected_level":0,"polling_rate":1000,"sleep_minutes":5,"dim_seconds":60,"color_top":[1,2,3],"color_mid":[4,5,6],"color_bot":[7,8,9]}"#;
        let duplicates = format!(
            "{{\"version\":1,\"active\":\"Default\",\"configurations\":[{block},{block}]}}"
        );
        assert!(parse(&duplicates).unwrap_err().contains("duplicate"));
        let missing_active =
            format!("{{\"version\":1,\"active\":\"Gaming\",\"configurations\":[{block}]}}");
        assert!(parse(&missing_active).is_err());
    }

    #[test]
    fn correctable_values_are_sanitized_with_warnings() {
        let text = r#"{
            "version": 1,
            "active": "Default",
            "configurations": [{
                "name": "Default",
                "cpi_levels": [50, 99999],
                "cpi_count": 0,
                "selected_level": 7,
                "polling_rate": 1000,
                "sleep_minutes": 5,
                "dim_seconds": 60,
                "color_top": [1,2,3],
                "color_mid": [4,5,6],
                "color_bot": [7,8,9]
            }]
        }"#;
        let (store, warnings) = parse(text).unwrap();
        let block = store.active_block().unwrap();
        assert_eq!(block.cpi_levels, vec![100, 18000]);
        assert_eq!(block.cpi_count, 1); // documented rule: count 0 is clamped to 1
        assert_eq!(block.selected_level, 0); // index clamped to count - 1
        assert_ne!(warnings, Vec::<String>::new());
    }

    #[test]
    fn timers_and_polling_are_sanitized_before_they_reach_the_mouse() {
        let text = r#"{
            "version": 1,
            "active": "Default",
            "configurations": [{
                "name": "Default",
                "cpi_levels": [400],
                "cpi_count": 1,
                "selected_level": 0,
                "polling_rate": 600,
                "sleep_minutes": 4294967295,
                "dim_seconds": 4294967295,
                "color_top": [1,2,3],
                "color_mid": [4,5,6],
                "color_bot": [7,8,9]
            }]
        }"#;
        let (store, warnings) = parse(text).unwrap();
        let block = store.active_block().unwrap();
        assert_eq!(block.polling_rate, crate::core::protocol::DEFAULT_POLLING);
        assert_eq!(block.sleep_minutes, 20.0);
        assert_eq!(block.dim_seconds, 1_200);
        assert!(warnings.iter().any(|w| w.contains("polling rate")));
        assert!(warnings.iter().any(|w| w.contains("sleep timer")));
        assert!(warnings.iter().any(|w| w.contains("dim timer")));
    }

    #[test]
    fn invalid_and_out_of_range_timers_are_reset_with_warnings() {
        let mut store = ConfigStore::factory();
        store.active_block_mut().unwrap().sleep_minutes = -1.0;
        store.active_block_mut().unwrap().dim_seconds = 5_000;
        let warnings = store.validate().unwrap();
        let block = store.active_block().unwrap();
        assert_eq!(block.sleep_minutes, 0.0);
        assert_eq!(block.dim_seconds, 1_200);
        assert!(warnings.iter().any(|w| w.contains("invalid sleep timer")));
        assert!(warnings.iter().any(|w| w.contains("dim timer 5000")));

        let mut store = ConfigStore::factory();
        store.active_block_mut().unwrap().sleep_minutes = f32::NAN;
        let warnings = store.validate().unwrap();
        assert_eq!(store.active_block().unwrap().sleep_minutes, 0.0);
        assert!(warnings.iter().any(|w| w.contains("invalid sleep timer")));
    }

    #[test]
    fn missing_keys_fall_back_to_factory_defaults() {
        let (store, _) =
            parse(r#"{"version":1,"active":"Default","configurations":[{"name":"Default"}]}"#)
                .unwrap();
        assert_eq!(
            store.active_block().unwrap(),
            &ConfigBlock::named("Default")
        );
    }

    #[test]
    fn create_switch_and_delete_follow_the_name_rules() {
        let mut store = ConfigStore::factory();
        assert!(store.create("", ConfigBlock::named("x")).is_err());
        assert!(store.create("Default", ConfigBlock::named("x")).is_err());
        store
            .create("  Gaming  ", ConfigBlock::named("copy"))
            .unwrap();
        assert_eq!(store.active, "Gaming");
        assert_eq!(store.configurations.len(), 2);
        assert_eq!(store.active_block().unwrap().name, "Gaming");
        assert!(store.remove("Gaming").is_err(), "active block is protected");
        assert!(store.remove("Default").is_err(), "default is protected");
        store.switch_active("Default").unwrap();
        assert_eq!(store.active, "Default");
        store.remove("Gaming").unwrap();
        assert_eq!(store.configurations.len(), 1);
        assert!(store.switch_active("Gaming").is_err());
    }

    #[test]
    fn save_validates_in_place_and_returns_warnings() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aerox5-config-save-validate-test-{}-{unique}.json",
            std::process::id()
        ));
        let mut store = ConfigStore::factory();
        store.active_block_mut().unwrap().zone_brightness = 250;
        let warnings = save(&path, &mut store).unwrap();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("brightness")),
            "sanitization warnings must be returned to the caller"
        );
        assert_eq!(store.active_block().unwrap().zone_brightness, 100);
        let (loaded, load_warnings) = load(&path).unwrap().unwrap();
        assert_eq!(load_warnings, Vec::<String>::new());
        assert_eq!(loaded, store, "disk and the validated store must agree");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_then_load_round_trip_uses_explicit_temp_path() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aerox5-config-test-{}-{unique}.json",
            std::process::id()
        ));
        let mut store = ConfigStore::factory();
        save(&path, &mut store).unwrap();
        let (loaded, warnings) = load(&path).unwrap().unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(loaded, store);
        let _ = std::fs::remove_file(&path);
    }
}
