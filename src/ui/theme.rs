//! Dark palette and global egui styling modelled on the `SteelSeries` Engine
//! "CONFIGURATION : DEFAULT" screenshot (`work/software.jpg`).

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle};

pub const LEFT_W: f32 = 215.0;
pub const CENTER_W: f32 = 312.0;
pub const RIGHT_W: f32 = 437.0;
pub const CONTENT_W: f32 = 1012.0;
pub const COL_GAP: f32 = 24.0;

/// Two-row header: configuration name on the first row, connection/battery and event status on
/// the second. The bottom bar and the centre column keep the heights measured in task 030.
pub const HEADER_H: f32 = 56.0;
pub const BOTTOM_H: f32 = 38.0;
pub const CONTENT_H: f32 = 417.0;

/// Window client size that fits the content exactly: the columns plus the central panel's
/// 10 pt margins on each side, and the header/bottom bar plus the central panel's 8 pt margins.
pub const WINDOW_W: f32 = CONTENT_W + 20.0;
pub const WINDOW_H: f32 = HEADER_H + BOTTOM_H + 16.0 + CONTENT_H;

pub const C_PAGE: Color32 = Color32::from_rgb(24, 29, 36); // #181D24
pub const C_HEADER: Color32 = Color32::from_rgb(43, 50, 59); // #2B323B
pub const C_PANEL: Color32 = Color32::from_rgb(38, 44, 53); // #262C35
pub const C_PANEL_HEADER: Color32 = Color32::from_rgb(54, 61, 72); // #363D48
pub const C_ROW: Color32 = Color32::from_rgb(32, 38, 46); // #20262E
pub const C_ROW_ALT: Color32 = Color32::from_rgb(28, 34, 41);
pub const C_ROW_HOVER: Color32 = Color32::from_rgb(47, 56, 68);
pub const C_BLUE: Color32 = Color32::from_rgb(62, 92, 126); // #3E5C7E
pub const C_BLUE_BRIGHT: Color32 = Color32::from_rgb(83, 128, 173); // #5380AD
pub const C_TEAL: Color32 = Color32::from_rgb(45, 89, 94);
pub const C_TEXT: Color32 = Color32::from_rgb(218, 223, 230); // #DADFE6
pub const C_MUTED: Color32 = Color32::from_rgb(139, 148, 162); // #8B94A2
pub const C_DIM: Color32 = Color32::from_rgb(100, 108, 120);
pub const C_BORDER: Color32 = Color32::from_rgb(16, 20, 26);
pub const C_ORANGE: Color32 = Color32::from_rgb(198, 90, 56); // #C65A38
pub const C_RED: Color32 = Color32::from_rgb(235, 74, 47);
pub const C_GREEN: Color32 = Color32::from_rgb(93, 190, 116);
pub const C_AMBER: Color32 = Color32::from_rgb(224, 166, 67);
pub const C_GRAPH_BG: Color32 = Color32::from_rgb(23, 28, 35);
pub const C_GRAPH_GRID: Color32 = Color32::from_rgb(52, 60, 72);

/// Zone colours sent (and drawn) when RGB lighting is switched off.
pub const RGB_OFF: [u8; 3] = [0, 0, 0];

/// Indicator colours for sensitivity levels 1–5, matching the flashes the mouse shows when the
/// active CPI level changes. Used by the small LED dots on the level boxes.
pub const LEVEL_COLORS: [Color32; 5] = [
    Color32::from_rgb(168, 85, 247), // 1 — purple #A855F7
    Color32::from_rgb(59, 130, 246), // 2 — blue #3B82F6
    Color32::from_rgb(34, 197, 94),  // 3 — green #22C55E
    Color32::from_rgb(234, 179, 8),  // 4 — yellow #EAB308
    Color32::from_rgb(239, 68, 68),  // 5 — red #EF4444
];

pub fn shade(color: Color32, delta: i8) -> Color32 {
    Color32::from_rgb(
        color.r().saturating_add_signed(delta),
        color.g().saturating_add_signed(delta),
        color.b().saturating_add_signed(delta),
    )
}

pub fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    style.visuals = egui::Visuals::dark();
    style.visuals.dark_mode = true;
    style.visuals.panel_fill = C_PAGE;
    style.visuals.window_fill = C_PANEL;
    style.visuals.window_stroke = Stroke::new(1.0_f32, C_BORDER);
    style.visuals.window_corner_radius = CornerRadius::same(2);
    style.visuals.menu_corner_radius = CornerRadius::same(2);
    style.visuals.extreme_bg_color = Color32::from_rgb(18, 22, 28);
    style.visuals.faint_bg_color = C_ROW_ALT;
    style.visuals.code_bg_color = C_ROW;
    style.visuals.override_text_color = Some(C_TEXT);
    style.visuals.selection.bg_fill = C_BLUE;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, C_TEXT);
    style.visuals.hyperlink_color = C_BLUE_BRIGHT;
    style.visuals.warn_fg_color = C_ORANGE;
    style.visuals.error_fg_color = C_RED;
    style.visuals.slider_trailing_fill = false;
    style.visuals.striped = false;

    let widgets = &mut style.visuals.widgets;
    widgets.noninteractive.bg_fill = C_PANEL;
    widgets.noninteractive.weak_bg_fill = C_PANEL;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, C_BORDER);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, C_TEXT);
    widgets.noninteractive.corner_radius = CornerRadius::same(2);

    widgets.inactive.bg_fill = C_ROW;
    widgets.inactive.weak_bg_fill = C_ROW;
    widgets.inactive.bg_stroke = Stroke::new(1.0_f32, C_BORDER);
    widgets.inactive.fg_stroke = Stroke::new(1.0_f32, C_TEXT);
    widgets.inactive.corner_radius = CornerRadius::same(2);

    widgets.hovered.bg_fill = C_ROW_HOVER;
    widgets.hovered.weak_bg_fill = C_ROW_HOVER;
    widgets.hovered.bg_stroke = Stroke::new(1.0_f32, C_BLUE);
    widgets.hovered.fg_stroke = Stroke::new(1.0_f32, C_TEXT);
    widgets.hovered.corner_radius = CornerRadius::same(2);

    widgets.active.bg_fill = C_BLUE;
    widgets.active.weak_bg_fill = C_BLUE;
    widgets.active.bg_stroke = Stroke::new(1.0_f32, C_BLUE_BRIGHT);
    widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    widgets.active.corner_radius = CornerRadius::same(2);

    widgets.open.bg_fill = C_BLUE;
    widgets.open.weak_bg_fill = C_BLUE;
    widgets.open.bg_stroke = Stroke::new(1.0_f32, C_BLUE_BRIGHT);
    widgets.open.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    widgets.open.corner_radius = CornerRadius::same(2);

    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.interact_size = egui::vec2(40.0, 18.0);
    style.spacing.slider_width = 180.0;
    style.spacing.scroll.bar_width = 5.0;
    style.spacing.scroll.floating = true;

    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(17.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(11.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(10.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(9.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(10.0));

    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_window_heights_match_the_content() {
        assert_eq!(HEADER_H, 56.0);
        assert_eq!(BOTTOM_H, 38.0);
        assert_eq!(CONTENT_H, 417.0);
        assert_eq!(WINDOW_H, HEADER_H + BOTTOM_H + 16.0 + CONTENT_H);
        assert_eq!(WINDOW_H, 527.0);
        // Two text rows plus the vertical margins (8 pt top/bottom) must fit the header.
        const {
            assert!(HEADER_H >= 2.0 * 18.0 + 4.0 + 2.0 * 8.0);
        }
    }

    #[test]
    fn level_colors_match_the_mouse_indicators() {
        assert_eq!(
            LEVEL_COLORS,
            [
                Color32::from_rgb(168, 85, 247), // #A855F7
                Color32::from_rgb(59, 130, 246), // #3B82F6
                Color32::from_rgb(34, 197, 94),  // #22C55E
                Color32::from_rgb(234, 179, 8),  // #EAB308
                Color32::from_rgb(239, 68, 68),  // #EF4444
            ]
        );
    }
}
