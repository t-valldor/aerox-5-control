//! Centre-panel mouse image with B1–B9 callouts and the illumination glow.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2,
};

use crate::ui::theme::{C_BLUE, C_BLUE_BRIGHT, C_BORDER, C_TEXT};
use crate::ui::widgets::Selection;

/// Aspect ratio of `assets/Aerox5WirelessTop.png` (896×1195).
const IMAGE_ASPECT: f32 = 896.0 / 1195.0;

/// Height the mouse illustration is drawn at, in points. The image is centred in the space
/// allocated by `draw_mouse`, so this leaves an equal margin above and below it.
const MOUSE_IMAGE_H: f32 = 320.0;

/// Bounds of the visible mouse inside the PNG (alpha bbox x 185..715, y 125..1092 of 896×1195).
/// The transparent padding is not part of the illustration the callouts surround.
const VISIBLE_LEFT: f32 = 185.0 / 896.0;
const VISIBLE_RIGHT: f32 = 715.0 / 896.0;
const VISIBLE_TOP: f32 = 125.0 / 1195.0;

/// What the centre panel shows on top of the mouse image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseView {
    Callouts,
    Illumination,
}

impl From<bool> for MouseView {
    fn from(illumination: bool) -> Self {
        if illumination {
            Self::Illumination
        } else {
            Self::Callouts
        }
    }
}

/// Decodes the embedded mouse image and uploads it as a reusable texture.
pub fn load_mouse_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let icon = mouse_icon_data();
    let width = usize::try_from(icon.width).expect("mouse image width fits usize");
    let height = usize::try_from(icon.height).expect("mouse image height fits usize");
    let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &icon.rgba);
    ctx.load_texture("aerox5_mouse_top", image, egui::TextureOptions::LINEAR)
}

fn mouse_icon_data() -> egui::viewport::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/Aerox5WirelessTop.png"))
        .expect("assets/Aerox5WirelessTop.png must be a valid PNG")
}

/// Action indices 0–8 correspond to callouts B1–B9.
pub fn draw_mouse(
    ui: &mut Ui,
    texture: &egui::TextureHandle,
    height: f32,
    view: MouseView,
    selected_action: usize,
    zone_colors: [[u8; 3]; 3],
) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let image_rect = fitted_image_rect(rect);
    let painter = ui.painter().with_clip_rect(rect);

    if view == MouseView::Illumination {
        draw_glow(&painter, image_rect, zone_colors);
    }

    ui.put(
        image_rect,
        egui::Image::from_texture(texture).fit_to_exact_size(image_rect.size()),
    );

    if view == MouseView::Callouts {
        draw_callouts(&painter, rect, image_rect, selected_action);
    }
}

/// Fixed-size copy of the portrait image, centred in the allocated area. Callouts and glow
/// are positioned from this rect, so they follow the image whenever its size changes.
fn fitted_image_rect(area: Rect) -> Rect {
    let mut height = MOUSE_IMAGE_H.min(area.height());
    if height * IMAGE_ASPECT > area.width() {
        height = area.width() / IMAGE_ASPECT;
    }
    Rect::from_center_size(area.center(), Vec2::new(height * IMAGE_ASPECT, height))
}

/// Soft, mouse-shaped glow behind the image, one ellipse per illumination zone.
fn draw_glow(painter: &Painter, image_rect: Rect, colors: [[u8; 3]; 3]) {
    if colors.iter().all(|rgb| *rgb == [0, 0, 0]) {
        return;
    }
    for (rgb, y_fraction) in colors.iter().zip([0.30_f32, 0.50, 0.70]) {
        let center = Pos2::new(
            image_rect.center().x,
            image_rect.top() + image_rect.height() * y_fraction,
        );
        let radius = Vec2::new(image_rect.width() * 0.42, image_rect.height() * 0.16);
        let halo = Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], 12);
        let core = Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], 26);
        painter.add(egui::Shape::ellipse_filled(center, radius * 1.25, halo));
        painter.add(egui::Shape::ellipse_filled(center, radius, core));
    }
}

/// Callout targets are fractions of the image rectangle; labels sit in the middle of the
/// space between the visible mouse and the edge of the allocated area.
fn draw_callouts(painter: &Painter, area: Rect, image_rect: Rect, selected_action: usize) {
    let callouts: [(usize, &str, f32, f32, f32, f32); 9] = [
        // Targets follow work/callout-mapping.png from the original software.
        (0, "B1", 0.04, 0.04, 0.37, 0.21),
        (1, "B2", 0.96, 0.04, 0.65, 0.21),
        (2, "B3", 0.50, 0.04, 0.50, 0.26),
        (8, "B9", 0.04, 0.25, 0.24, 0.29),
        (4, "B5", 0.04, 0.38, 0.22, 0.41),
        (6, "B7", 0.04, 0.49, 0.22, 0.49),
        (7, "B8", 0.04, 0.58, 0.23, 0.56),
        (3, "B4", 0.04, 0.70, 0.24, 0.62),
        (5, "B6", 0.96, 0.46, 0.50, 0.44),
    ];
    let visible_left = image_rect.left() + image_rect.width() * VISIBLE_LEFT;
    let visible_right = image_rect.left() + image_rect.width() * VISIBLE_RIGHT;
    let visible_top = image_rect.top() + image_rect.height() * VISIBLE_TOP;
    for (action, text, label_x, label_y, target_x, target_y) in callouts {
        let width = if text.len() > 2 { 38.0 } else { 30.0 };
        let height = 18.0;
        let x = if (label_x - 0.5).abs() < 1e-6 {
            image_rect.center().x - width * 0.5
        } else if label_x < 0.5 {
            area.left() + (visible_left - area.left() - width) * 0.5
        } else {
            visible_right + (area.right() - visible_right - width) * 0.5
        };
        let y = if label_y <= 0.05 {
            area.top() + (visible_top - area.top() - height) * 0.5
        } else {
            image_rect.top() + image_rect.height() * label_y
        };
        let label_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height));
        let target = Pos2::new(
            image_rect.left() + image_rect.width() * target_x,
            image_rect.top() + image_rect.height() * target_y,
        );
        draw_callout(
            painter,
            label_rect,
            target,
            text,
            (selected_action == action).into(),
        );
    }
}

fn draw_callout(painter: &Painter, rect: Rect, target: Pos2, text: &str, state: Selection) {
    let selected = state == Selection::Selected;
    let fill = if selected { C_BLUE_BRIGHT } else { C_BLUE };
    let line_start = if target.x < rect.left() {
        Pos2::new(rect.left(), rect.center().y)
    } else if target.x > rect.right() {
        Pos2::new(rect.right(), rect.center().y)
    } else if target.y < rect.top() {
        Pos2::new(rect.center().x, rect.top())
    } else {
        Pos2::new(rect.center().x, rect.bottom())
    };
    painter.line_segment(
        [line_start, target],
        Stroke::new(
            1.2_f32,
            Color32::from_rgba_unmultiplied(95, 124, 158, if selected { 230 } else { 135 }),
        ),
    );
    painter.rect_filled(rect, CornerRadius::same(2), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, if selected { C_TEXT } else { C_BORDER }),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(if text.len() > 2 { 8.0 } else { 8.5 }),
        Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_image_decodes_at_expected_size() {
        let icon = mouse_icon_data();
        assert_eq!(icon.width, 896);
        assert_eq!(icon.height, 1195);
        assert_eq!(icon.rgba.len(), 896 * 1195 * 4);
    }

    #[test]
    fn fitted_image_is_centred_with_equal_margins() {
        let area = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(312.0, 343.0));
        let image = fitted_image_rect(area);
        assert_eq!(image.height(), MOUSE_IMAGE_H);
        assert_eq!(image.width(), MOUSE_IMAGE_H * IMAGE_ASPECT);
        assert_eq!(image.top() - area.top(), area.bottom() - image.bottom());
        assert_eq!(image.left() - area.left(), area.right() - image.right());
    }
}
