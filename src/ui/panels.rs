//! Left, centre and right panels of the configuration screen.

use std::time::Instant;

use eframe::egui::{
    self, Align, Align2, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2,
};

use crate::core::protocol::{DefaultLighting, POLLING_RATES, scaled_color, snap_cpi};
use crate::ui::app::{Aerox5App, RgbState, SettingsTab, ZoneState, effective_zone_colors};
use crate::ui::mouse::{MouseView, draw_mouse};
use crate::ui::theme::{
    C_AMBER, C_BLUE, C_BLUE_BRIGHT, C_BORDER, C_DIM, C_GRAPH_BG, C_GRAPH_GRID, C_MUTED,
    C_PANEL_HEADER, C_RED, C_ROW, C_ROW_ALT, C_ROW_HOVER, C_TEAL, C_TEXT, CENTER_W,
};
use crate::ui::widgets::{
    DISCRETE_SLIDER_H, Selection, WidgetState, content_frame, discrete_slider, draw_slider_labels,
    level_box, level_box_motions, panel_shell, rgb_toggle, section_header,
    section_header_with_link, tab_button, timer_slider,
};

/// Space left empty below the mouse on both tabs. The illumination controls used to live here
/// and now sit in the left column; keeping the strip stops the mouse moving when tabs change.
const CENTER_TAIL_H: f32 = 46.0;

/// Space allocated to the mouse itself; the fixed-size image is centred in it, leaving equal
/// margins above and below. Together with the tabs and the tail this matches the 417 pt side
/// columns, so the window does not grow when switching tabs.
const MOUSE_AREA_H: f32 = 313.0;

/// Gap between the two bottom columns of the right panel.
const BOTTOM_COLUMN_GAP: f32 = 8.0;

const ACTIONS: [&str; 12] = [
    "Left Trigger",
    "Right Trigger",
    "Scroll Wheel",
    "Side Button 1",
    "Side Button 2",
    "CPI Toggle",
    "Side Button 3 Up",
    "Side Button 3 Down",
    "Side Button 4",
    "Scroll Up",
    "Scroll Down",
    "Macro Editor",
];

/// Display order of the action rows. Indices stay tied to the B1–B9 callouts in `src/mouse.rs`,
/// so CPI Toggle (index 5, callout B6) is shown last among the physical buttons.
const ACTION_ORDER: [usize; 12] = [0, 1, 2, 3, 4, 6, 7, 8, 5, 9, 10, 11];

impl Aerox5App {
    pub fn actions_panel(&mut self, ui: &mut Ui) {
        panel_shell(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            section_header(ui, "ACTIONS");
            for (row, &index) in ACTION_ORDER.iter().enumerate() {
                let label = ACTIONS[index];
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 21.0), Sense::click());
                let selected = self.selected_action == index;
                let fill = if selected {
                    C_BLUE
                } else if response.hovered() {
                    C_ROW_HOVER
                } else if index == 9 || index == 10 {
                    C_TEAL
                } else if row % 2 == 0 {
                    C_ROW
                } else {
                    C_ROW_ALT
                };
                let painter = ui.painter();
                painter.rect_filled(rect, 0.0, fill);
                painter.line_segment(
                    [
                        Pos2::new(rect.left(), rect.bottom() - 0.5),
                        Pos2::new(rect.right(), rect.bottom() - 0.5),
                    ],
                    Stroke::new(1.0_f32, C_BORDER),
                );
                if selected {
                    painter.rect_filled(
                        Rect::from_min_size(rect.min, Vec2::new(2.0, rect.height())),
                        0.0,
                        C_BLUE_BRIGHT,
                    );
                }
                painter.text(
                    Pos2::new(rect.left() + 12.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(10.0),
                    C_TEXT,
                );

                if index == 11 {
                    let launch = Rect::from_min_size(
                        Pos2::new(rect.right() - 62.0, rect.center().y - 7.5),
                        Vec2::new(54.0, 15.0),
                    );
                    painter.rect_filled(launch, CornerRadius::same(2), C_PANEL_HEADER);
                    painter.rect_stroke(
                        launch,
                        CornerRadius::same(2),
                        Stroke::new(1.0_f32, C_BORDER),
                        StrokeKind::Inside,
                    );
                    painter.text(
                        launch.center(),
                        Align2::CENTER_CENTER,
                        "LAUNCH",
                        FontId::proportional(8.0),
                        C_MUTED,
                    );
                }

                let response = if index == 11 {
                    response.on_hover_text(
                        "The macro editor is not exposed by the documented Aerox 5 protocol.",
                    )
                } else {
                    response
                };
                if response.clicked() {
                    self.selected_action = index;
                }
            }
        });
    }

    pub fn bluetooth_panel(ui: &mut Ui) {
        panel_shell(ui, |ui| {
            section_header(ui, "BLUETOOTH MODE")
                .on_hover_text("Bluetooth mode is not exposed by the documented Aerox 5 protocol.");
            content_frame(ui, |ui| {
                ui.horizontal(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::new(42.0, 42.0), Sense::hover());
                    draw_bluetooth_icon(ui.painter(), icon_rect);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("CURRENTLY").size(7.5).color(C_MUTED));
                        ui.label(RichText::new("OFF").size(14.0).strong().color(C_TEXT));
                    });
                });
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "Settings for illumination and button bindings return to defaults \
                             in Bluetooth mode. Polling rate is fixed at 125 Hz.",
                        )
                        .size(8.0)
                        .color(C_MUTED),
                    )
                    .wrap(),
                );
            });
        });
    }

    pub fn center_panel(&mut self, ui: &mut Ui) {
        ui.set_width(CENTER_W);
        ui.horizontal(|ui| {
            if tab_button(
                ui,
                "Settings",
                146.0,
                (self.tab == SettingsTab::Settings).into(),
            )
            .clicked()
            {
                self.tab = SettingsTab::Settings;
            }
            ui.add_space(6.0);
            if tab_button(
                ui,
                "Illumination",
                146.0,
                (self.tab == SettingsTab::Illumination).into(),
            )
            .clicked()
            {
                self.tab = SettingsTab::Illumination;
            }
        });
        ui.add_space(8.0);

        let illumination = self.tab == SettingsTab::Illumination;
        let zone_colors = effective_zone_colors(
            RgbState::from(self.rgb_enabled),
            ZoneState::from(self.live_zone_colors),
            self.zone_brightness,
            [self.color_top, self.color_mid, self.color_bot],
        );
        draw_mouse(
            ui,
            &self.mouse_texture,
            MOUSE_AREA_H,
            MouseView::from(illumination),
            self.selected_action,
            zone_colors,
        );

        ui.add_space(CENTER_TAIL_H);
    }

    /// Illumination controls that replace the Actions box on the left while this tab is open.
    pub fn illumination_panel(&mut self, ui: &mut Ui) {
        self.illumination_rgb_box(ui);
        ui.add_space(10.0);
        self.illumination_zone_box(ui);
    }

    fn illumination_rgb_box(&mut self, ui: &mut Ui) {
        panel_shell(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("RGB LIGHTING")
                        .size(9.5)
                        .strong()
                        .color(C_TEXT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(4.0);
                    let mut on = self.rgb_enabled;
                    rgb_toggle(ui, &mut on, WidgetState::Enabled);
                    self.rgb_enabled = on;
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Effect").size(8.5).color(C_MUTED));
                    let mut mode = self.startup_lighting;
                    let selected = if self.rgb_enabled {
                        mode.label()
                    } else {
                        "off"
                    };
                    let combo = ui.add_enabled_ui(self.rgb_enabled, |ui| {
                        egui::ComboBox::from_id_salt("startup_lighting")
                            .width(110.0)
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                for option in DefaultLighting::ALL {
                                    ui.selectable_value(&mut mode, option, option.label());
                                }
                            })
                            .response
                    });
                    combo.inner.on_hover_text(
                        "This becomes the default lighting effect when the mouse starts.",
                    );
                    self.startup_lighting = mode;
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Color").size(8.5).color(C_MUTED));
                    let reactive = self.rgb_enabled && self.startup_lighting.has_reactive();
                    let old_interact_size = ui.spacing().interact_size;
                    ui.spacing_mut().interact_size = Vec2::new(32.0, 16.0);
                    if reactive {
                        let response = ui.color_edit_button_srgb(&mut self.reactive_color);
                        draw_zone_swatch_glow(
                            ui.painter(),
                            response.rect,
                            self.reactive_color,
                            WidgetState::Enabled,
                        );
                    } else {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(32.0, 16.0), Sense::hover());
                        let dimmed = scaled_color(self.reactive_color, 55);
                        let fill = egui::Color32::from_rgb(dimmed[0], dimmed[1], dimmed[2]);
                        let painter = ui.painter();
                        painter.rect_filled(rect, CornerRadius::same(2), fill);
                        draw_zone_swatch_glow(painter, rect, dimmed, WidgetState::Disabled);
                    }
                    ui.spacing_mut().interact_size = old_interact_size;
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                draw_memory_icon(ui.painter(), icon_rect);
                ui.add_space(5.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(
                        RichText::new("Saved to onboard memory")
                            .size(8.5)
                            .color(C_MUTED),
                    );
                    ui.label(
                        RichText::new("(persists on sleep & Bluetooth)")
                            .size(8.0)
                            .color(C_DIM),
                    );
                });
            });
            ui.add_space(6.0);
        });
    }

    fn illumination_zone_box(&mut self, ui: &mut Ui) {
        panel_shell(ui, |ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("LIVE ZONE COLORS").size(9.5).color(C_TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(4.0);
                    let mut on = self.live_zone_colors;
                    rgb_toggle(ui, &mut on, WidgetState::from(self.rgb_enabled));
                    self.live_zone_colors = on;
                });
            });
            ui.add_space(9.0);

            let live = self.rgb_enabled && self.live_zone_colors;
            let old_interact_size = ui.spacing().interact_size;
            ui.spacing_mut().interact_size = Vec2::new(32.0, 16.0);
            let colors = [
                &mut self.color_top,
                &mut self.color_mid,
                &mut self.color_bot,
            ];
            for (title, color) in ["TOP", "MID", "BOT"].into_iter().zip(colors) {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    let (label_rect, _) =
                        ui.allocate_exact_size(Vec2::new(32.0, 16.0), Sense::hover());
                    ui.painter().text(
                        label_rect.left_center(),
                        Align2::LEFT_CENTER,
                        title,
                        FontId::proportional(9.0),
                        if live { C_MUTED } else { C_DIM },
                    );
                    ui.add_space(10.0);
                    if live {
                        let response = ui.color_edit_button_srgb(color);
                        draw_zone_swatch_glow(
                            ui.painter(),
                            response.rect,
                            *color,
                            WidgetState::Enabled,
                        );
                    } else {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(32.0, 16.0), Sense::hover());
                        let dimmed = scaled_color(*color, 55);
                        let fill = egui::Color32::from_rgb(dimmed[0], dimmed[1], dimmed[2]);
                        let painter = ui.painter();
                        painter.rect_filled(rect, CornerRadius::same(2), fill);
                        draw_zone_swatch_glow(painter, rect, dimmed, WidgetState::Disabled);
                    }
                });
                ui.add_space(6.0);
            }
            ui.spacing_mut().interact_size = old_interact_size;

            ui.add_space(4.0);
            ui.spacing_mut().slider_width = (ui.available_width() - 8.0).max(80.0);
            let slider = ui.add_enabled(
                live,
                egui::Slider::new(&mut self.zone_brightness, 0..=100).show_value(false),
            );
            slider.on_hover_text(
                "Brightness of the three zone colours. It scales the RGB values sent to the \
                 mouse; the Aerox 5 has no separate brightness command.",
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.add_space(4.0);
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                draw_bolt_icon(ui.painter(), icon_rect);
                ui.add_space(5.0);
                ui.label(
                    RichText::new("Temporary (resets on sleep)")
                        .size(8.5)
                        .color(C_MUTED),
                );
            });
            ui.add_space(6.0);
        });
    }

    pub fn right_panel(&mut self, ui: &mut Ui) {
        self.sensitivity_levels_panel(ui);
        ui.add_space(6.0);
        self.adjusting_cpi_panel(ui);
        ui.add_space(6.0);
        // Two-column table: the panels above span both columns; these two fill one each.
        // `horizontal_top` already inserts `item_spacing.x` between children, so include it
        // in the gap budget along with the explicit space below.
        let gap = BOTTOM_COLUMN_GAP + ui.spacing().item_spacing.x;
        let column_width = (ui.available_width() - gap) * 0.5;
        let mut battery_height = 0.0_f32;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(column_width);
                battery_height = self.battery_saver_panel(ui);
            });
            ui.add_space(BOTTOM_COLUMN_GAP);
            ui.vertical(|ui| {
                ui.set_width(column_width);
                ui.allocate_ui(Vec2::new(column_width, battery_height), |ui| {
                    self.polling_panel(ui);
                });
            });
        });
    }

    fn sensitivity_levels_panel(&mut self, ui: &mut Ui) {
        panel_shell(ui, |ui| {
            let (header, reset) =
                section_header_with_link(ui, "MOUSE SENSITIVITY LEVELS", "Reset to defaults");
            header.on_hover_text(
                "Five CPI levels. The selected level is highlighted; the button under the \
                 scroll wheel cycles levels on the mouse.",
            );
            if reset.clicked() {
                self.reset_cpi_to_defaults();
            }
            content_frame(ui, |ui| {
                let mut selected = self.selected_level;
                let mut add = false;
                let elapsed = self
                    .cpi_anim_start
                    .map(|start| start.elapsed().as_secs_f32());
                let motions = level_box_motions(self.cpi_anim_from, elapsed);
                ui.horizontal(|ui| {
                    for (index, motion) in motions.iter().enumerate().take(self.cpi_count) {
                        if level_box(
                            ui,
                            index,
                            self.cpi_levels[index],
                            Selection::from(selected == index),
                            *motion,
                        )
                        .clicked()
                        {
                            selected = index;
                        }
                    }
                    if self.cpi_count < 5 {
                        ui.add_space(4.0);
                        if add_level_button(ui).clicked() {
                            add = true;
                        }
                    } else {
                        ui.add_space(4.0);
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(10.0, 58.0), Sense::hover());
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(2), C_BLUE);
                    }
                });
                self.selected_level = selected.min(self.cpi_count.saturating_sub(1));
                if add && self.cpi_count < 5 {
                    let last = self.cpi_levels[self.cpi_count - 1];
                    self.cpi_levels[self.cpi_count] = snap_cpi((last + 100).min(18000));
                    self.selected_level = self.cpi_count;
                    self.cpi_count += 1;
                    self.clear_cpi_animation();
                }
            });
        });
    }

    fn adjusting_cpi_panel(&mut self, ui: &mut Ui) {
        panel_shell(ui, |ui| {
            content_frame(ui, |ui| {
                let index = self.selected_level.min(self.cpi_count.saturating_sub(1));
                let mut remove_clicked = false;

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Adjusting CPI")
                            .size(11.5)
                            .strong()
                            .color(C_TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let enabled = self.cpi_count > 1;
                        let response = ui.add_enabled(
                            enabled,
                            egui::Button::new(
                                RichText::new("Remove Level").size(8.5).color(if enabled {
                                    C_BLUE_BRIGHT
                                } else {
                                    C_DIM
                                }),
                            )
                            .frame(false),
                        );
                        if response.clicked() {
                            remove_clicked = true;
                        }
                    });
                });

                let current = self.cpi_levels[index];
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "With a CPI (Counts Per Inch) setting of {current}, a one-inch \
                             movement of your mouse moves your cursor {current} pixels. Click the \
                             slider to adjust the selected level."
                        ))
                        .size(8.3)
                        .color(C_MUTED),
                    )
                    .wrap(),
                );

                let mut slider_rect = Rect::NOTHING;
                ui.horizontal(|ui| {
                    let value_box_width = 58.0;
                    // The row also spends `item_spacing.x` on the gap before the value box and
                    // adds an explicit 4 pt, so reserve both to keep within the panel width.
                    let row_gaps = ui.spacing().item_spacing.x + 4.0;
                    let slider_width =
                        (ui.available_width() - value_box_width - row_gaps).max(90.0);
                    ui.spacing_mut().slider_width = slider_width;
                    let response = ui.add(
                        egui::Slider::new(&mut self.cpi_levels[index], 100..=18000)
                            .step_by(100.0)
                            .show_value(false),
                    );
                    slider_rect = response.rect;
                    // account for row gap before the value box in slider_width above
                    ui.add_space(4.0);
                    let mut value = self.cpi_levels[index];
                    let drag = ui.add_sized(
                        Vec2::new(value_box_width, 20.0),
                        egui::DragValue::new(&mut value)
                            .range(100..=18000)
                            .speed(100.0)
                            .fixed_decimals(0),
                    );
                    if drag.changed() {
                        self.cpi_levels[index] = snap_cpi(value);
                    }
                });
                self.cpi_levels[index] = snap_cpi(self.cpi_levels[index]);
                // Keep the boxes ascending after an edit. The selected level follows its value
                // to its new box so the slider keeps adjusting that same number next frame.
                let (selected, sources) =
                    sort_cpi_values(&mut self.cpi_levels, self.cpi_count, index);
                self.selected_level = selected;
                if sources != [0, 1, 2, 3, 4] {
                    self.cpi_anim_from = sources;
                    self.cpi_anim_start = Some(Instant::now());
                }
                // Fractions must use the slider's 100..=18000 range, not value/max.
                let cpi_fraction = |value: f32| (value - 100.0) / (18000.0 - 100.0);
                draw_slider_labels(
                    ui,
                    slider_rect,
                    &[
                        (cpi_fraction(100.0), "100"),
                        (cpi_fraction(2000.0), "2000"),
                        (cpi_fraction(5000.0), "5000"),
                        (cpi_fraction(10000.0), "10000"),
                        (1.0, "18000"),
                    ],
                );

                if remove_clicked && self.cpi_count > 1 {
                    let remove_at = self.selected_level.min(self.cpi_count - 1);
                    for i in remove_at..self.cpi_count - 1 {
                        self.cpi_levels[i] = self.cpi_levels[i + 1];
                    }
                    self.cpi_count -= 1;
                    self.selected_level = remove_at.min(self.cpi_count - 1);
                    self.clear_cpi_animation();
                }
            });
        });
    }

    fn battery_saver_panel(&mut self, ui: &mut Ui) -> f32 {
        let response = panel_shell(ui, |ui| {
            section_header(ui, "BATTERY SAVER SETTINGS")
                .on_hover_text("Battery Saver Settings help conserve your device's battery life.");
            content_frame(ui, |ui| {
                let sleep_display = format_minutes(self.sleep_minutes);
                timer_slider(
                    ui,
                    "Sleep Timer",
                    &mut self.sleep_minutes,
                    MAX_TIMER_MINUTES,
                    TIMER_MARKERS,
                    sleep_display,
                );
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(10.0);
                // The dim timer is stored in seconds; show it in the same minutes as the sleep
                // timer and write the rounded seconds back after editing.
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_precision_loss,
                    reason = "dim_seconds is clamped to 0..=1200, which f32 represents exactly"
                )]
                let mut dim_minutes = self.dim_seconds as f32 / 60.0;
                let dim_display = format_minutes(dim_minutes);
                timer_slider(
                    ui,
                    "Illumination Dim Timer",
                    &mut dim_minutes,
                    MAX_TIMER_MINUTES,
                    TIMER_MARKERS,
                    dim_display,
                );
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "the slider is limited to 0..=20 minutes, so the rounded seconds fit u32"
                )]
                let dim_seconds = (dim_minutes * 60.0).round() as u32;
                self.dim_seconds = dim_seconds;
            });
        });
        response.response.rect.height()
    }

    fn polling_panel(&mut self, ui: &mut Ui) {
        panel_shell(ui, |ui| {
            section_header(ui, "POLLING RATE").on_hover_text("Polling Rate determines how often your computer communicates with your device. The values below are measured in Hz. 1000 Hz means that your device's data will be sent to the computer every 1 millisecond. Choosing 125 Hz lowers the frequency to every 8 milliseconds.");
            content_frame(ui, |ui| {
                // Grow the graph into the space left by the taller Battery Saver panel.
                let controls_h = ui.spacing().item_spacing.y + 2.0 + DISCRETE_SLIDER_H;
                let graph_height = (ui.available_height() - controls_h).max(26.0);
                let target = polling_pulse_count(self.polling_rate);
                if (target - self.polling_graph_to).abs() > 0.001 {
                    // Re-target from the currently displayed count so rapid changes never jump.
                    let elapsed = self
                        .polling_graph_start
                        .map(|start| start.elapsed().as_secs_f32());
                    let current =
                        interpolate_pulses(self.polling_graph_from, self.polling_graph_to, elapsed);
                    self.polling_graph_from = current;
                    self.polling_graph_to = target;
                    self.polling_graph_start = Some(Instant::now());
                }
                let elapsed = self
                    .polling_graph_start
                    .map(|start| start.elapsed().as_secs_f32());
                draw_polling_graph(
                    ui,
                    graph_height,
                    self.polling_graph_from,
                    self.polling_graph_to,
                    elapsed,
                );
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    // `ComboBox::width` is the inner width; the themed 8 pt button padding is
                    // added on both sides, so the outer button is wider than `combo_width`.
                    // Use a value large enough that its content never exceeds the minimum.
                    let combo_width = 60.0;
                    let row_gaps = ui.spacing().item_spacing.x + 4.0;
                    let slider_width = (ui.available_width() - combo_width - row_gaps).max(80.0);
                    let mut index = POLLING_RATES
                        .iter()
                        .position(|rate| *rate == self.polling_rate)
                        .unwrap_or(3);
                    let response = discrete_slider(
                        ui,
                        &mut index,
                        &["125", "250", "500", "1000"],
                        slider_width,
                    );
                    if response.clicked() || response.dragged() {
                        self.polling_rate = POLLING_RATES[index];
                    }
                    ui.add_space(4.0);
                    egui::ComboBox::from_id_salt("polling_rate_combo")
                        .width(combo_width)
                        .selected_text(self.polling_rate.to_string())
                        .show_ui(ui, |ui| {
                            for rate in POLLING_RATES {
                                ui.selectable_value(&mut self.polling_rate, rate, rate.to_string());
                            }
                        });
                });
            });
        });
    }
}

/// Sorts the active CPI values ascending in place, leaving each box's `LEVEL N` label and
/// indicator colour untouched (both are derived from the box index, not the value). Returns the
/// new index of the value that was at `selected`, plus for every slot the slot its value came
/// from, so the UI can animate each tile from its old position. Slots at or beyond `count` are
/// stale leftovers and are never touched (they report themselves as their own source).
fn sort_cpi_values(levels: &mut [u32; 5], count: usize, selected: usize) -> (usize, [usize; 5]) {
    let count = count.clamp(1, levels.len());
    let selected = selected.min(count - 1);
    let mut indexed: Vec<(u32, usize)> = levels[..count]
        .iter()
        .copied()
        .enumerate()
        .map(|(index, value)| (value, index))
        .collect();
    indexed.sort_by(|(value_a, index_a), (value_b, index_b)| {
        value_a.cmp(value_b).then_with(|| index_a.cmp(index_b))
    });

    let mut new_selected = selected;
    let mut sources = [0usize; 5];
    for (index, source) in sources.iter_mut().enumerate() {
        *source = index;
    }
    for (new_index, (value, old_index)) in indexed.into_iter().enumerate() {
        levels[new_index] = value;
        sources[new_index] = old_index;
        if old_index == selected {
            new_selected = new_index;
        }
    }
    (new_selected, sources)
}

fn add_level_button(ui: &mut Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(24.0, 58.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), C_PANEL_HEADER);
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, C_BORDER),
        StrokeKind::Inside,
    );
    let center = rect.center();
    painter.line_segment(
        [
            Pos2::new(center.x - 4.0, center.y),
            Pos2::new(center.x + 4.0, center.y),
        ],
        Stroke::new(1.5_f32, C_BLUE_BRIGHT),
    );
    painter.line_segment(
        [
            Pos2::new(center.x, center.y - 4.0),
            Pos2::new(center.x, center.y + 4.0),
        ],
        Stroke::new(1.5_f32, C_BLUE_BRIGHT),
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Shared maximum for both battery-saver timers, in minutes.
const MAX_TIMER_MINUTES: f32 = 20.0;

/// Identical tick labels for both battery-saver timers.
const TIMER_MARKERS: &[(f32, &str)] = &[
    (0.0, "off"),
    (5.0 / 20.0, "5m"),
    (10.0 / 20.0, "10m"),
    (15.0 / 20.0, "15m"),
    (1.0, "20m"),
];

fn format_minutes(minutes: f32) -> String {
    if minutes == 0.0 {
        "off".to_owned()
    } else if minutes.fract() == 0.0 {
        #[expect(
            clippy::as_conversions,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the timer sliders are limited to 0..=20 minutes"
        )]
        let whole = minutes as u32;
        format!("{whole}m")
    } else {
        format!("{minutes}m")
    }
}

fn draw_bluetooth_icon(painter: &egui::Painter, rect: Rect) {
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, C_BLUE),
        StrokeKind::Inside,
    );
    let p = |x: f32, y: f32| Pos2::new(rect.left() + x, rect.top() + y);
    let stroke = Stroke::new(1.6_f32, C_BLUE_BRIGHT);
    let top = p(24.0, 6.0);
    let bottom = p(24.0, 36.0);
    let right_upper = p(33.0, 13.0);
    let right_lower = p(33.0, 29.0);
    let left_upper = p(15.0, 13.0);
    let left_lower = p(15.0, 29.0);
    painter.line_segment([top, bottom], stroke);
    painter.line_segment([top, right_upper], stroke);
    painter.line_segment([right_upper, left_lower], stroke);
    painter.line_segment([bottom, right_lower], stroke);
    painter.line_segment([right_lower, left_upper], stroke);
}

/// Seconds for the polling waveform to morph between pulse counts.
pub const POLLING_GRAPH_SECONDS: f32 = 0.25;
/// Pulse count at 1000 Hz — also the fixed number of animated slots.
pub const POLLING_MAX_PULSES: usize = 10;
const POLLING_PULSE_W: f32 = 9.0;
const POLLING_GRAPH_MARGIN: f32 = 6.0;

/// Number of pulses visible for a supported rate: 125/250/500/1000 → 3/4/6/10.
pub fn polling_pulse_count(rate: u32) -> f32 {
    let pulses = rate / 125 + 2;
    f32::from(i16::try_from(pulses).unwrap_or(0))
}

/// Eased pulse count between the previous and current rates. `None` means settled on `to`.
pub fn interpolate_pulses(from: f32, to: f32, elapsed_seconds: Option<f32>) -> f32 {
    match elapsed_seconds {
        None => to,
        Some(elapsed) => {
            let t = (elapsed / POLLING_GRAPH_SECONDS).clamp(0.0, 1.0);
            from + (to - from) * t * t * (3.0 - 2.0 * t)
        }
    }
}

/// `(x_offset, alpha)` for each of the ten fixed pulse slots. Slots beyond a rate's pulse count
/// are parked off to the right and transparent; changing rate slides/fades them into place.
pub fn polling_pulse_layout(
    width: f32,
    from: f32,
    to: f32,
    elapsed_seconds: Option<f32>,
) -> Vec<(f32, f32)> {
    let t = elapsed_seconds.map_or(1.0, |elapsed| {
        (elapsed / POLLING_GRAPH_SECONDS).clamp(0.0, 1.0)
    });
    let eased = t * t * (3.0 - 2.0 * t);
    let usable = (width - 2.0 * POLLING_GRAPH_MARGIN - POLLING_PULSE_W).max(1.0);
    let from = from.max(2.0);
    let to = to.max(2.0);
    (0..POLLING_MAX_PULSES)
        .map(|slot| {
            let index = f32::from(i16::try_from(slot).unwrap_or(0));
            let from_x = POLLING_GRAPH_MARGIN + usable * index / (from - 1.0);
            let to_x = POLLING_GRAPH_MARGIN + usable * index / (to - 1.0);
            let from_alpha = (from - index).clamp(0.0, 1.0);
            let to_alpha = (to - index).clamp(0.0, 1.0);
            (
                from_x + (to_x - from_x) * eased,
                from_alpha + (to_alpha - from_alpha) * eased,
            )
        })
        .collect()
}

/// Bottom segments joining consecutive visible pulses. The resulting stepped wave starts at the
/// first pulse's left edge and ends at the last pulse's right edge — never beyond them.
fn polling_bottom_segments(slots: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let visible: Vec<f32> = slots
        .iter()
        .filter(|(_, alpha)| *alpha > 0.001)
        .map(|(x, _)| *x)
        .collect();
    visible
        .windows(2)
        .map(|pair| (pair[0] + POLLING_PULSE_W, pair[1]))
        .collect()
}

/// Adds a red line with two wider, dimmer copies behind it so it reads as a soft glow.
fn add_glowing_line(painter: &egui::Painter, points: Vec<Pos2>, alpha: f32) {
    painter.add(Shape::line(
        points.clone(),
        Stroke::new(5.0_f32, C_RED.gamma_multiply(0.12 * alpha)),
    ));
    painter.add(Shape::line(
        points.clone(),
        Stroke::new(3.0_f32, C_RED.gamma_multiply(0.25 * alpha)),
    ));
    painter.add(Shape::line(
        points,
        Stroke::new(1.3_f32, C_RED.gamma_multiply(alpha)),
    ));
}

fn draw_polling_graph(ui: &mut Ui, height: f32, from: f32, to: f32, elapsed_seconds: Option<f32>) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), C_GRAPH_BG);
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, C_BORDER),
        StrokeKind::Inside,
    );
    for i in 1_i16..6 {
        let x = rect.left() + rect.width() * f32::from(i) / 6.0;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0_f32, C_GRAPH_GRID),
        );
    }
    let base = rect.bottom() - 6.0;
    let top = rect.top() + 6.0;
    // In-flight slots slide in from beyond the right edge; confine the waveform (and its glow)
    // to the graph box so they never spill over the surrounding panel during a transition.
    let wave_painter = painter.with_clip_rect(rect);
    let slots = polling_pulse_layout(rect.width(), from, to, elapsed_seconds);
    for (start, end) in polling_bottom_segments(&slots) {
        add_glowing_line(
            &wave_painter,
            vec![
                Pos2::new(rect.left() + start, base),
                Pos2::new(rect.left() + end, base),
            ],
            1.0,
        );
    }
    for (offset, alpha) in slots {
        if alpha <= 0.001 {
            continue;
        }
        let x = rect.left() + offset;
        let points = vec![
            Pos2::new(x, base),
            Pos2::new(x, top),
            Pos2::new(x + POLLING_PULSE_W, top),
            Pos2::new(x + POLLING_PULSE_W, base),
        ];
        add_glowing_line(&wave_painter, points, alpha);
    }
}

fn draw_memory_icon(painter: &egui::Painter, rect: Rect) {
    let body = Rect::from_center_size(rect.center(), Vec2::new(9.0, 12.0));
    painter.rect_stroke(
        body,
        CornerRadius::same(1),
        Stroke::new(1.0_f32, C_MUTED),
        StrokeKind::Inside,
    );
    for offset in [-2.0_f32, 0.0, 2.0] {
        painter.line_segment(
            [
                Pos2::new(body.left() + 2.0, body.center().y + offset),
                Pos2::new(body.right() - 2.0, body.center().y + offset),
            ],
            Stroke::new(1.0_f32, C_MUTED),
        );
    }
}

fn draw_zone_swatch_glow(painter: &egui::Painter, swatch: Rect, rgb: [u8; 3], state: WidgetState) {
    let enabled = state == WidgetState::Enabled;
    let (halo_alpha, core_alpha) = if enabled { (18, 36) } else { (6, 12) };
    let halo = egui::Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], halo_alpha);
    let core = egui::Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], core_alpha);
    painter.rect_filled(swatch.expand(3.0), CornerRadius::same(5), halo);
    painter.rect_filled(swatch.expand(1.5), CornerRadius::same(4), core);
    painter.rect_stroke(
        swatch,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, C_BORDER),
        StrokeKind::Inside,
    );
}

fn draw_bolt_icon(painter: &egui::Painter, rect: Rect) {
    let center = rect.center();
    painter.add(Shape::convex_polygon(
        vec![
            Pos2::new(center.x + 1.5, center.y - 5.0),
            Pos2::new(center.x - 3.5, center.y + 1.0),
            Pos2::new(center.x + 0.5, center.y + 1.0),
        ],
        C_AMBER,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![
            Pos2::new(center.x - 1.5, center.y + 5.0),
            Pos2::new(center.x + 3.5, center.y - 1.0),
            Pos2::new(center.x - 0.5, center.y - 1.0),
        ],
        C_AMBER,
        Stroke::NONE,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_order_moves_cpi_toggle_last_without_remapping_indices() {
        assert_eq!(ACTION_ORDER, [0, 1, 2, 3, 4, 6, 7, 8, 5, 9, 10, 11]);
        let mut sorted = ACTION_ORDER;
        sorted.sort_unstable();
        assert_eq!(sorted.to_vec(), (0..ACTIONS.len()).collect::<Vec<_>>());
        assert_eq!(ACTIONS[ACTION_ORDER[8]], "CPI Toggle");
        assert_eq!(ACTIONS[ACTION_ORDER[5]], "Side Button 3 Up");
    }

    #[test]
    fn sorting_moves_values_and_follows_the_selected_one() {
        let mut levels = [400, 3000, 800, 2400, 3200];
        let (selected, sources) = sort_cpi_values(&mut levels, 5, 1);
        assert_eq!(levels, [400, 800, 2400, 3000, 3200]);
        assert_eq!(selected, 3);
        assert_eq!(sources, [0, 2, 3, 1, 4]);
    }

    #[test]
    fn sorting_keeps_ties_stable_and_leaves_tail_slots_alone() {
        let mut levels = [800, 400, 400, 9999, 9999];
        let (selected, sources) = sort_cpi_values(&mut levels, 3, 1);
        assert_eq!(levels, [400, 400, 800, 9999, 9999]);
        assert_eq!(selected, 0);
        assert_eq!(sources, [1, 2, 0, 3, 4]);
    }

    #[test]
    fn sorting_an_already_sorted_list_changes_nothing() {
        let mut levels = [400, 800, 1200, 2400, 3200];
        let (selected, sources) = sort_cpi_values(&mut levels, 5, 2);
        assert_eq!(levels, [400, 800, 1200, 2400, 3200]);
        assert_eq!(selected, 2);
        assert_eq!(sources, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn polling_pulse_counts_match_the_reference_screenshots() {
        let counts: Vec<f32> = POLLING_RATES
            .iter()
            .copied()
            .map(polling_pulse_count)
            .collect();
        assert_eq!(counts, [3.0, 4.0, 6.0, 10.0]);
    }

    #[test]
    fn polling_interpolation_eases_between_counts() {
        assert_eq!(interpolate_pulses(3.0, 10.0, None), 10.0);
        assert_eq!(interpolate_pulses(3.0, 10.0, Some(0.0)), 3.0);
        let mid = interpolate_pulses(3.0, 10.0, Some(POLLING_GRAPH_SECONDS * 0.5));
        assert!((mid - 6.5).abs() < 0.0001);
        assert_eq!(
            interpolate_pulses(3.0, 10.0, Some(POLLING_GRAPH_SECONDS)),
            10.0
        );
    }

    #[test]
    fn polling_layout_settles_on_the_target_pulse_count() {
        let three = polling_pulse_layout(400.0, 3.0, 3.0, None);
        let visible: Vec<(f32, f32)> = three
            .into_iter()
            .filter(|(_, alpha)| *alpha > 0.001)
            .collect();
        assert_eq!(visible.len(), 3);
        assert!(
            visible
                .iter()
                .all(|(_, alpha)| (*alpha - 1.0).abs() < 0.0001)
        );
        assert!((visible[0].0 - 6.0).abs() < 0.001);
        assert!((visible[2].0 - 385.0).abs() < 0.001);
        let ten = polling_pulse_layout(400.0, 10.0, 10.0, None);
        assert_eq!(ten.iter().filter(|(_, alpha)| *alpha > 0.001).count(), 10);
    }

    #[test]
    fn polling_bottom_segments_only_join_visible_pulses() {
        let three = polling_pulse_layout(400.0, 3.0, 3.0, None);
        let segments = polling_bottom_segments(&three);
        assert_eq!(segments.len(), 2);
        assert!((segments[0].0 - 15.0).abs() < 0.001);
        assert!((segments[0].1 - 195.5).abs() < 0.001);
        assert!((segments[1].0 - 204.5).abs() < 0.001);
        assert!((segments[1].1 - 385.0).abs() < 0.001);
        assert!(
            segments
                .iter()
                .all(|(start, end)| *start < *end && *end <= 400.0)
        );

        let ten = polling_pulse_layout(400.0, 10.0, 10.0, None);
        assert_eq!(polling_bottom_segments(&ten).len(), 9);

        let starting = polling_pulse_layout(400.0, 3.0, 4.0, Some(0.0));
        assert_eq!(polling_bottom_segments(&starting).len(), 2);
    }

    #[test]
    fn polling_layout_morphs_slots_instead_of_jumping() {
        let start = polling_pulse_layout(400.0, 3.0, 4.0, Some(0.0));
        assert_eq!(start.iter().filter(|(_, alpha)| *alpha > 0.001).count(), 3);
        let mid = polling_pulse_layout(400.0, 3.0, 4.0, Some(POLLING_GRAPH_SECONDS * 0.5));
        assert!((mid[3].1 - 0.5).abs() < 0.0001);
        let end = polling_pulse_layout(400.0, 3.0, 4.0, Some(POLLING_GRAPH_SECONDS));
        assert_eq!(end, polling_pulse_layout(400.0, 4.0, 4.0, None));
        assert_eq!(end.iter().filter(|(_, alpha)| *alpha > 0.001).count(), 4);
    }
}
