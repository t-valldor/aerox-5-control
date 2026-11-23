mod core;
mod ui;

use ui::app::Aerox5App;
use ui::theme::{WINDOW_H, WINDOW_W};

fn main() -> eframe::Result<()> {
    // eframe otherwise installs its built-in egui icon as the macOS application/Dock icon
    // shortly after launch (see eframe's app_icon.rs), replacing the bundle's real icon.
    let app_icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/AppIcon.png"))
        .expect("assets/AppIcon.png must be a valid PNG (app icon)");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([WINDOW_W, WINDOW_H])
            .with_min_inner_size([WINDOW_W, WINDOW_H])
            .with_icon(app_icon),
        // Open centred and do not restore a stale position: the screenshot
        // layout is fixed-size and must stay fully visible when displays are
        // rearranged.
        centered: true,
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Aerox 5 Control",
        options,
        Box::new(|cc| Ok(Box::new(Aerox5App::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_app_icon_decodes_to_expected_size() {
        let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/AppIcon.png"))
            .expect("app icon PNG must decode");
        assert_eq!(icon.width, 1024);
        assert_eq!(icon.height, 1024);
        assert_eq!(icon.rgba.len(), 1024 * 1024 * 4);
        let mut min_alpha = 255u8;
        let mut max_alpha = 0u8;
        for &alpha in icon.rgba.iter().skip(3).step_by(4) {
            min_alpha = min_alpha.min(alpha);
            max_alpha = max_alpha.max(alpha);
        }
        assert_eq!(min_alpha, 0, "icon background must be transparent");
        assert_eq!(max_alpha, 255, "icon must keep opaque pixels");
    }
}
