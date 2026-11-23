//! Small painted widgets shared by the panel implementations.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, InnerResponse, Layout,
    Margin, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::ui::theme::{
    C_BLUE, C_BLUE_BRIGHT, C_BORDER, C_DIM, C_MUTED, C_ORANGE, C_PANEL, C_PANEL_HEADER, C_ROW,
    C_ROW_HOVER, C_TEXT, LEVEL_COLORS, shade,
};

/// Minimum padding kept between a panel's frame and its contents.
pub const MIN_BOX_PADDING: i8 = 4;

/// Whether a control accepts input; replaces a bare `bool` at widget call sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetState {
    Enabled,
    Disabled,
}

impl From<bool> for WidgetState {
    fn from(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

/// Visual selection state of a tab, level box or callout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Selected,
    Unselected,
}

impl From<bool> for Selection {
    fn from(selected: bool) -> Self {
        if selected {
            Self::Selected
        } else {
            Self::Unselected
        }
    }
}

pub fn panel_shell<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let width = ui.available_width();
    Frame::new()
        .fill(C_PANEL)
        .stroke(Stroke::new(1.0_f32, C_BORDER))
        .corner_radius(CornerRadius::same(2))
        .inner_margin(Margin::same(MIN_BOX_PADDING))
        .show(ui, |ui| {
            // The frame adds padding and a 1 pt stroke on both sides; subtract them so the
            // resulting outer rect keeps exactly the requested width.
            ui.set_width((width - 2.0 - f32::from(2 * MIN_BOX_PADDING)).max(0.0));
            add(ui)
        })
}

pub fn content_frame<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .inner_margin(egui::Margin::symmetric(7, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn section_header(ui: &mut Ui, title: &str) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, C_PANEL_HEADER);
    painter.text(
        Pos2::new(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(9.5),
        C_TEXT,
    );
    response
}

/// Like [`section_header`], but with a flat text link on the right.
/// Returns (header response, link response); the link uses the same unframed, small blue text
/// style as the "Remove Level" button in the Adjusting CPI panel.
pub fn section_header_with_link(ui: &mut Ui, title: &str, link_text: &str) -> (Response, Response) {
    let (rect, header_response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, C_PANEL_HEADER);
    painter.text(
        Pos2::new(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(9.5),
        C_TEXT,
    );
    // Flat link area: no frame or fill, matching the "Remove Level" text button. Keep the
    // original right edge, which sat 12 pt left of the removed `?` marker.
    let link_right = rect.right() - 20.0;
    let link_rect = Rect::from_min_max(
        Pos2::new((link_right - 96.0).max(rect.left() + 120.0), rect.top()),
        Pos2::new(link_right, rect.bottom()),
    );
    let link_response = ui.interact(
        link_rect,
        ui.id().with("section_header_link"),
        Sense::click(),
    );
    let color = if link_response.hovered() {
        shade(C_BLUE_BRIGHT, 16)
    } else {
        C_BLUE_BRIGHT
    };
    painter.text(
        Pos2::new(link_right, rect.center().y),
        Align2::RIGHT_CENTER,
        link_text,
        FontId::proportional(8.5),
        color,
    );
    if link_response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (header_response, link_response)
}

pub fn painted_button(
    ui: &mut Ui,
    text: &str,
    width: f32,
    height: f32,
    fill: Color32,
    text_color: Color32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let fill = if response.hovered() {
        shade(fill, 16)
    } else {
        fill
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, shade(fill, -22)),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(9.5),
        text_color,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn configs_button(ui: &mut Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(86.0, 24.0), Sense::click());
    let fill = if response.hovered() {
        shade(C_PANEL_HEADER, 16)
    } else {
        C_PANEL_HEADER
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, C_BORDER),
        StrokeKind::Inside,
    );
    for i in 0_i16..3 {
        let y = rect.center().y - 4.0 + f32::from(i) * 4.0;
        painter.line_segment(
            [
                Pos2::new(rect.left() + 8.0, y),
                Pos2::new(rect.left() + 14.0, y),
            ],
            Stroke::new(1.0_f32, C_MUTED),
        );
    }
    painter.text(
        Pos2::new(rect.left() + 21.0, rect.center().y),
        Align2::LEFT_CENTER,
        "CONFIGS",
        FontId::proportional(9.0),
        C_TEXT,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn tab_button(ui: &mut Ui, text: &str, width: f32, state: Selection) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::click());
    let active = state == Selection::Selected;
    let fill = if active {
        C_PANEL
    } else if response.hovered() {
        shade(C_PANEL_HEADER, 10)
    } else {
        C_PANEL_HEADER
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, C_BORDER),
        StrokeKind::Inside,
    );
    if active {
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.bottom() - 2.0),
                Vec2::new(rect.width(), 2.0),
            ),
            0.0,
            C_BLUE_BRIGHT,
        );
    }
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.5),
        if active { C_TEXT } else { C_MUTED },
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Paint tick labels underneath a slider at the given fractions (0.0–1.0).
///
/// egui maps slider values onto a track inset by the handle radius
/// (`rect.height() / 2.5`), so labels use that same range and are centred on
/// their tick to line up with the handle positions exactly.
pub fn draw_slider_labels(ui: &mut Ui, slider_rect: Rect, markers: &[(f32, &str)]) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(slider_rect.width(), 11.0), Sense::hover());
    let handle_radius = slider_rect.height() / 2.5;
    let track = slider_rect.x_range().shrink(handle_radius);
    let painter = ui.painter();
    for &(fraction, label) in markers {
        let x = track.min + fraction.clamp(0.0, 1.0) * track.span();
        painter.text(
            Pos2::new(x, rect.top()),
            Align2::CENTER_TOP,
            label,
            FontId::proportional(8.0),
            C_DIM,
        );
    }
}

pub fn timer_slider(
    ui: &mut Ui,
    title: &str,
    value: &mut f32,
    max: f32,
    markers: &[(f32, &str)],
    display: String,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(10.0).color(C_TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(display).size(8.5).color(C_MUTED));
        });
    });
    ui.spacing_mut().slider_width = ui.available_width().max(40.0);
    let response = ui.add(
        egui::Slider::new(value, 0.0..=max)
            .step_by(0.5)
            .show_value(false),
    );
    draw_slider_labels(ui, response.rect, markers);
}

/// Height reserved by [`discrete_slider`] for its track and labels.
pub const DISCRETE_SLIDER_H: f32 = 32.0;

/// Horizontal discrete slider used for the polling-rate selector.
pub fn discrete_slider(ui: &mut Ui, index: &mut usize, labels: &[&str], width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width.max(60.0), DISCRETE_SLIDER_H),
        Sense::click_and_drag(),
    );
    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let last = labels.len().saturating_sub(1);
        let last_f = f32::from(u16::try_from(last).expect("label count fits u16"));
        #[expect(
            clippy::as_conversions,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "t is clamped to 0..=1, so the rounded value is within 0..=last"
        )]
        let picked = (t * last_f).round() as usize;
        *index = picked.min(last);
    }
    let left = rect.left() + 6.0;
    let right = rect.right() - 6.0;
    let track_y = rect.top() + 9.0;
    let painter = ui.painter();
    painter.line_segment(
        [Pos2::new(left, track_y), Pos2::new(right, track_y)],
        Stroke::new(2.0_f32, C_DIM),
    );
    let denominator = f32::from(
        u16::try_from(labels.len().saturating_sub(1).max(1)).expect("label count fits u16"),
    );
    for (i, label) in labels.iter().enumerate() {
        let i_f = f32::from(u16::try_from(i).expect("label index fits u16"));
        let x = left + (right - left) * i_f / denominator;
        let selected = i == *index;
        painter.circle_filled(
            Pos2::new(x, track_y),
            if selected { 5.0 } else { 2.0 },
            if selected { C_TEXT } else { C_DIM },
        );
        painter.text(
            Pos2::new(x, rect.top() + 24.0),
            Align2::CENTER_CENTER,
            *label,
            FontId::proportional(7.5),
            if selected { C_TEXT } else { C_DIM },
        );
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Width of a sensitivity-level box. The row uses `item_spacing.x = 6`, so moving to the
/// neighbouring slot covers `CPI_BOX_STEP` pixels.
pub const CPI_BOX_W: f32 = 74.0;
pub const CPI_BOX_STEP: f32 = CPI_BOX_W + 6.0;
/// Time a CPI value takes to slide from one box to another.
pub const CPI_SWAP_SECONDS: f32 = 0.18;

/// Visual state of one level box while values swap slots. `offset_x` moves the painted tile away
/// from its fixed logical rect; `progress` crossfades the label/dot from the source identity to
/// this box's identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelBoxMotion {
    pub offset_x: f32,
    pub from_level: usize,
    pub progress: f32,
}

/// Pure easing/offset math for the five boxes. `from[i]` is the slot the value now in box `i`
/// came from; `elapsed_seconds = None` means the animation is settled.
pub fn level_box_motions(from: [usize; 5], elapsed_seconds: Option<f32>) -> [LevelBoxMotion; 5] {
    let t = elapsed_seconds.map_or(1.0, |elapsed| (elapsed / CPI_SWAP_SECONDS).clamp(0.0, 1.0));
    let eased = t * t * (3.0 - 2.0 * t);
    std::array::from_fn(|index| {
        let source = from[index].min(LEVEL_COLORS.len() - 1);
        let source_step = f32::from(i16::try_from(source).unwrap_or(0));
        let index_step = f32::from(i16::try_from(index).unwrap_or(0));
        LevelBoxMotion {
            offset_x: (source_step - index_step) * CPI_BOX_STEP * (1.0 - eased),
            from_level: if eased < 1.0 { source } else { index },
            progress: eased,
        }
    })
}

/// One of the five sensitivity-level boxes. The allocated rect stays at the logical slot for
/// hit-testing; only the painted tile is offset by `motion`, so clicks never chase a moving box.
pub fn level_box(
    ui: &mut Ui,
    index: usize,
    cpi: u32,
    state: Selection,
    motion: LevelBoxMotion,
) -> Response {
    let selected = state == Selection::Selected;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(CPI_BOX_W, 58.0), Sense::click());
    let tile_rect = rect.translate(Vec2::new(motion.offset_x, 0.0));
    let center = tile_rect.center();
    let painter = ui.painter();
    let fill = if selected {
        C_BLUE
    } else if response.hovered() {
        shade(C_PANEL_HEADER, 12)
    } else {
        C_PANEL_HEADER
    };
    painter.rect_filled(tile_rect, CornerRadius::same(2), fill);
    painter.rect_stroke(
        tile_rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, C_BORDER),
        StrokeKind::Inside,
    );
    if selected {
        painter.rect_filled(
            Rect::from_min_size(tile_rect.min, Vec2::new(tile_rect.width(), 3.0)),
            0.0,
            C_BLUE_BRIGHT,
        );
    }
    // The dot's colour and the LEVEL text crossfade from the source box to this box, so at rest
    // both are bound to the box index rather than to the CPI value (see task 036).
    let color = LEVEL_COLORS[motion.from_level].lerp_to_gamma(LEVEL_COLORS[index], motion.progress);
    painter.circle_filled(
        Pos2::new(center.x - 18.0, tile_rect.top() + 17.0),
        3.0,
        color,
    );
    let label_pos = Pos2::new(center.x + 7.0, tile_rect.top() + 17.0);
    if motion.from_level == index {
        painter.text(
            label_pos,
            Align2::CENTER_CENTER,
            format!("LEVEL {}", index + 1),
            FontId::proportional(8.0),
            C_TEXT,
        );
    } else {
        painter.text(
            label_pos,
            Align2::CENTER_CENTER,
            format!("LEVEL {}", motion.from_level + 1),
            FontId::proportional(8.0),
            C_TEXT.gamma_multiply(1.0 - motion.progress),
        );
        painter.text(
            label_pos,
            Align2::CENTER_CENTER,
            format!("LEVEL {}", index + 1),
            FontId::proportional(8.0),
            C_TEXT.gamma_multiply(motion.progress),
        );
    }
    painter.text(
        Pos2::new(center.x, tile_rect.top() + 38.0),
        Align2::CENTER_CENTER,
        format!("{cpi} CPI"),
        FontId::proportional(10.5),
        if selected { Color32::WHITE } else { C_TEXT },
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn info_row(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).size(9.5).color(C_MUTED));
    ui.add(egui::Label::new(RichText::new(value).size(9.5)).wrap());
    ui.end_row();
}

/// Orange/grey pill toggle used by the Illumination tab's RGB and live-zone switches.
/// A disabled toggle keeps its on/off state but draws dimmed and cannot be clicked.
pub fn rgb_toggle(ui: &mut Ui, on: &mut bool, state: WidgetState) -> Response {
    let enabled = state == WidgetState::Enabled;
    let (rect, mut response) = ui.allocate_exact_size(
        Vec2::new(30.0, 18.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if enabled && response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let track = if *on { C_ORANGE } else { C_ROW };
    let track = if enabled {
        track
    } else {
        track.gamma_multiply(0.5)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(9), track);
    painter.rect_stroke(
        rect,
        CornerRadius::same(9),
        Stroke::new(1.0_f32, C_BORDER),
        StrokeKind::Inside,
    );
    let knob_x = if *on {
        rect.right() - 9.0
    } else {
        rect.left() + 9.0
    };
    let knob = if *on { Color32::WHITE } else { C_MUTED };
    painter.circle_filled(
        Pos2::new(knob_x, rect.center().y),
        6.5,
        if enabled { knob } else { C_DIM },
    );
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response
}

/// Filled connection dot. Drawn as a shape instead of the `●` glyph, which is not present in
/// every font in egui's fallback chain and can render as a hollow box.
pub fn status_dot(ui: &mut Ui, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(11.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0_f32, color);
    response
}

/// Circular info button for the header. Replaces the long `PRODUCT INFORMATION AND HELP ?`
/// link with a compact icon; a click opens the device information window.
pub fn info_button(ui: &mut Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    let hovered = response.hovered();
    let pressed = response.is_pointer_button_down_on();
    let center = rect.center();
    let radius = if pressed { 8.0 } else { 9.0 };
    let background = if pressed {
        C_BLUE
    } else if hovered {
        C_ROW_HOVER
    } else {
        C_PANEL_HEADER
    };
    let foreground = if hovered || pressed { C_TEXT } else { C_MUTED };
    let outline = if hovered { C_BLUE_BRIGHT } else { C_MUTED };

    let painter = ui.painter();
    painter.circle_filled(center, radius, background);
    painter.circle_stroke(center, radius, Stroke::new(1.0_f32, outline));
    painter.circle_filled(center + Vec2::new(0.0, -3.8), 1.1, foreground);
    painter.line_segment(
        [center + Vec2::new(0.0, -1.2), center + Vec2::new(0.0, 4.0)],
        Stroke::new(1.6_f32, foreground),
    );
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settled_motions_have_no_offset_and_destination_identity() {
        let motions = level_box_motions([0, 2, 1, 3, 4], None);
        for (index, motion) in motions.iter().enumerate() {
            assert_eq!(motion.offset_x, 0.0);
            assert_eq!(motion.from_level, index);
            assert_eq!(motion.progress, 1.0);
        }
    }

    #[test]
    fn a_swap_starts_at_the_source_positions() {
        let motions = level_box_motions([0, 2, 1, 3, 4], Some(0.0));
        assert_eq!(motions[0].offset_x, 0.0);
        assert_eq!(motions[1].offset_x, CPI_BOX_STEP);
        assert_eq!(motions[2].offset_x, -CPI_BOX_STEP);
        assert_eq!(motions[3].offset_x, 0.0);
        assert_eq!(motions[1].from_level, 2);
        assert_eq!(motions[2].from_level, 1);
        assert_eq!(motions[1].progress, 0.0);
    }

    #[test]
    fn the_midpoint_halves_the_offset_with_smoothstep() {
        let motions = level_box_motions([0, 2, 1, 3, 4], Some(CPI_SWAP_SECONDS * 0.5));
        assert!((motions[1].progress - 0.5).abs() < 0.0001);
        assert!((motions[1].offset_x - CPI_BOX_STEP * 0.5).abs() < 0.0001);
    }

    #[test]
    fn an_expired_animation_is_fully_settled() {
        let motions = level_box_motions([0, 2, 1, 3, 4], Some(CPI_SWAP_SECONDS));
        for (index, motion) in motions.iter().enumerate() {
            assert_eq!(motion.offset_x, 0.0);
            assert_eq!(motion.from_level, index);
            assert_eq!(motion.progress, 1.0);
        }
    }
}
