//! Theme tokens and their application to egui (dark, rounded, purple accent).

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};

pub const APP_BG: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x05, 0x04, 0x10);
pub const PANEL_BORDER: Color32 = Color32::from_rgb(0x24, 0x21, 0x3A);
pub const CONTROL_BG: Color32 = Color32::from_rgb(0x16, 0x14, 0x26);
pub const CONTROL_HOVER_BG: Color32 = Color32::from_rgb(0x22, 0x1F, 0x38);
pub const SLIDER_RAIL: Color32 = Color32::from_rgb(0x6E, 0x6A, 0x88);
pub const TEXT: Color32 = Color32::from_rgb(0xF4, 0xF3, 0xFA);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0xA9, 0xA6, 0xBD);
pub const ACCENT: Color32 = Color32::from_rgb(0x5B, 0x12, 0xF5);
pub const ACCENT_LIGHT: Color32 = Color32::from_rgb(0xB6, 0x9C, 0xFF);
pub const FOCUS_RING: Color32 = ACCENT_LIGHT;
pub const CANVAS_BG: Color32 = Color32::from_rgb(0xEE, 0xEE, 0xF2);
pub const CANVAS_STRIPE: Color32 = Color32::from_rgb(0xC4, 0xC4, 0xCE);
pub const ARTBOARD_BG: Color32 = Color32::WHITE;
pub const ARTBOARD_BORDER: Color32 = Color32::from_rgb(0x76, 0x76, 0x82);
pub const CANVAS_TEXT: Color32 = Color32::from_rgb(0x0B, 0x0A, 0x14);
pub const INK: Color32 = Color32::from_rgb(0x0B, 0x0A, 0x14);

pub const PANEL_RADIUS: u8 = 10;
pub const CONTROL_RADIUS: u8 = 6;
pub const FOCUS_RING_WIDTH: f32 = 2.5;

pub const SEMIBOLD: &str = "poppins-semibold";

/// Installs Poppins (OFL-1.1) as the only font. egui's default fonts are
/// disabled because one of them uses a license outside our allow list.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::empty();
    fonts.font_data.insert(
        "poppins".into(),
        FontData::from_static(include_bytes!("../assets/fonts/Poppins-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        SEMIBOLD.into(),
        FontData::from_static(include_bytes!("../assets/fonts/Poppins-SemiBold.ttf")).into(),
    );
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["poppins".into()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["poppins".into()]);
    fonts
        .families
        .insert(FontFamily::Name(SEMIBOLD.into()), vec![SEMIBOLD.into()]);
    ctx.set_fonts(fonts);
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn apply_style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Body, FontId::proportional(13.0)),
            (TextStyle::Button, FontId::proportional(13.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
            (TextStyle::Heading, semibold(14.0)),
        ]
        .into();
        // Reduced motion: no animations at all in this proof of concept.
        style.animation_time = 0.0;
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.interact_size.y = 24.0;

        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(TEXT);
        v.panel_fill = PANEL_BG;
        v.window_fill = PANEL_BG;
        v.extreme_bg_color = CONTROL_BG;
        v.faint_bg_color = CONTROL_BG;
        v.window_corner_radius = CornerRadius::same(PANEL_RADIUS);
        v.window_stroke = Stroke::new(1.0, PANEL_BORDER);
        v.selection.bg_fill = ACCENT;
        v.selection.stroke = Stroke::new(1.5, TEXT);
        v.hyperlink_color = ACCENT_LIGHT;
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;

        let radius = CornerRadius::same(CONTROL_RADIUS);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = PANEL_BG;
        w.noninteractive.bg_stroke = Stroke::new(1.0, PANEL_BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        // `bg_fill` paints slider rails and handles; buttons use `weak_bg_fill`.
        w.inactive.bg_fill = SLIDER_RAIL;
        w.inactive.weak_bg_fill = CONTROL_BG;
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.hovered.bg_fill = CONTROL_HOVER_BG;
        w.hovered.weak_bg_fill = CONTROL_HOVER_BG;
        w.hovered.bg_stroke = Stroke::new(1.0, ACCENT_LIGHT);
        w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
        w.active.bg_fill = ACCENT;
        w.active.weak_bg_fill = ACCENT;
        w.active.bg_stroke = Stroke::new(1.0, ACCENT_LIGHT);
        w.active.fg_stroke = Stroke::new(1.5, TEXT);
        w.open.weak_bg_fill = CONTROL_HOVER_BG;
        for state in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            state.corner_radius = radius;
        }
    });
}

/// WCAG relative luminance of an sRGB color.
pub fn relative_luminance(color: Color32) -> f32 {
    fn channel(c: u8) -> f32 {
        let c = f32::from(c) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

/// WCAG contrast ratio between two colors, from 1.0 to 21.0.
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_MIN: f32 = 4.5;
    const NON_TEXT_MIN: f32 = 3.0;

    #[test]
    fn contrast_ratio_extremes() {
        assert!((contrast_ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(ACCENT, ACCENT) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn contrast_ratio_is_symmetric() {
        assert_eq!(contrast_ratio(TEXT, ACCENT), contrast_ratio(ACCENT, TEXT));
    }

    #[test]
    fn text_is_readable_on_every_background() {
        for (fg, bg) in [
            (TEXT, PANEL_BG),
            (TEXT, CONTROL_BG),
            (TEXT, CONTROL_HOVER_BG),
            (TEXT, ACCENT),
            (TEXT_MUTED, PANEL_BG),
            (TEXT_MUTED, CONTROL_BG),
            (CANVAS_TEXT, CANVAS_BG),
            (CANVAS_TEXT, ARTBOARD_BG),
        ] {
            let ratio = contrast_ratio(fg, bg);
            assert!(ratio >= TEXT_MIN, "{fg:?} on {bg:?}: {ratio:.2}");
        }
    }

    #[test]
    fn focus_ring_is_visible_next_to_controls() {
        for bg in [PANEL_BG, CONTROL_BG, ACCENT] {
            let ratio = contrast_ratio(FOCUS_RING, bg);
            assert!(ratio >= NON_TEXT_MIN, "focus ring on {bg:?}: {ratio:.2}");
        }
    }

    #[test]
    fn slider_rail_is_visible_on_panel() {
        assert!(contrast_ratio(SLIDER_RAIL, PANEL_BG) >= NON_TEXT_MIN);
    }

    #[test]
    fn artboard_edge_is_visible_on_canvas() {
        assert!(contrast_ratio(ARTBOARD_BORDER, CANVAS_BG) >= NON_TEXT_MIN);
        assert!(contrast_ratio(ARTBOARD_BORDER, ARTBOARD_BG) >= NON_TEXT_MIN);
    }
}
