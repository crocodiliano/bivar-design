//! Pen input: pressure readout and pressure-sensitive strokes.
//!
//! winit 0.30 (used by eframe 0.37) forwards only the pen pressure on
//! Windows. Tilt and the eraser end are dropped before reaching egui, and
//! reading them ourselves would need `unsafe` Win32 calls, which this proof of
//! concept does not allow. They are shown as "not available".

use eframe::egui::{Pos2, TouchPhase};

pub const MIN_WIDTH: f32 = 1.0;
pub const MAX_WIDTH: f32 = 14.0;
/// Width used for mouse strokes, which have no pressure.
pub const MOUSE_WIDTH: f32 = 3.0;
/// Upper bound on stored points, so a long session cannot exhaust memory.
pub const MAX_POINTS: usize = 200_000;

/// Keeps a pressure value inside 0–1 and rejects values that are not numbers.
pub fn normalize_pressure(raw: Option<f32>) -> Option<f32> {
    raw.filter(|p| p.is_finite()).map(|p| p.clamp(0.0, 1.0))
}

/// Stroke width in points for a given pressure. `None` means a mouse.
pub fn stroke_width(pressure: Option<f32>) -> f32 {
    match normalize_pressure(pressure) {
        Some(p) => MIN_WIDTH + (MAX_WIDTH - MIN_WIDTH) * p,
        None => MOUSE_WIDTH,
    }
}

/// Formats a value from 0 to 1 with two decimals and a decimal comma.
pub fn format_unit(value: f32) -> String {
    format!("{value:.2}").replace('.', ",")
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrokePoint {
    /// Position relative to the artboard's top-left corner.
    pub pos: Pos2,
    pub width: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stroke {
    pub points: Vec<StrokePoint>,
}

#[derive(Clone, Debug, Default)]
pub struct PenState {
    /// Last pressure reported by the pen, in 0–1.
    pub pressure: Option<f32>,
    pub in_contact: bool,
    pub strokes: Vec<Stroke>,
    drawing: bool,
    total_points: usize,
}

impl PenState {
    /// Handles one pen (touch) event. `pos` is relative to the artboard and
    /// `inside` says whether it lies within it.
    pub fn on_pen(&mut self, phase: TouchPhase, pos: Pos2, inside: bool, force: Option<f32>) {
        let pressure = normalize_pressure(force);
        match phase {
            TouchPhase::Start => {
                self.in_contact = true;
                self.pressure = pressure;
                if inside {
                    self.begin_stroke();
                    self.push_point(pos, pressure);
                }
            }
            TouchPhase::Move => {
                if pressure.is_some() {
                    self.pressure = pressure;
                }
                if self.drawing {
                    self.push_point(pos, pressure);
                }
            }
            TouchPhase::End | TouchPhase::Cancel => {
                self.in_contact = false;
                self.pressure = Some(0.0);
                self.drawing = false;
            }
        }
    }

    pub fn is_drawing(&self) -> bool {
        self.drawing
    }

    pub fn begin_stroke(&mut self) {
        self.strokes.push(Stroke::default());
        self.drawing = true;
    }

    pub fn end_stroke(&mut self) {
        self.drawing = false;
    }

    pub fn push_point(&mut self, pos: Pos2, pressure: Option<f32>) {
        if !self.drawing || self.total_points >= MAX_POINTS {
            return;
        }
        if let Some(stroke) = self.strokes.last_mut() {
            stroke.points.push(StrokePoint {
                pos,
                width: stroke_width(pressure),
            });
            self.total_points += 1;
        }
    }

    pub fn clear(&mut self) {
        self.strokes.clear();
        self.drawing = false;
        self.total_points = 0;
    }

    pub fn pressure_text(&self) -> String {
        match self.pressure {
            Some(p) => format!("Pressão: {}", format_unit(p)),
            None => "Pressão: —".to_owned(),
        }
    }

    pub fn contact_text(&self) -> &'static str {
        if self.in_contact {
            "Ponta: encostada"
        } else {
            "Ponta: no ar"
        }
    }
}

pub const TILT_TEXT: &str = "Inclinação: não disponível";
pub const ERASER_TEXT: &str = "Borracha: não disponível";

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn pressure_is_clamped_to_unit_range() {
        assert_eq!(normalize_pressure(Some(-0.5)), Some(0.0));
        assert_eq!(normalize_pressure(Some(0.4)), Some(0.4));
        assert_eq!(normalize_pressure(Some(7.0)), Some(1.0));
        assert_eq!(normalize_pressure(None), None);
    }

    #[test]
    fn non_numeric_pressure_is_rejected() {
        assert_eq!(normalize_pressure(Some(f32::NAN)), None);
        assert_eq!(normalize_pressure(Some(f32::INFINITY)), None);
    }

    #[test]
    fn width_grows_with_pressure() {
        assert_eq!(stroke_width(Some(0.0)), MIN_WIDTH);
        assert_eq!(stroke_width(Some(1.0)), MAX_WIDTH);
        assert!(stroke_width(Some(0.3)) < stroke_width(Some(0.6)));
        assert_eq!(stroke_width(None), MOUSE_WIDTH);
        assert_eq!(stroke_width(Some(f32::NAN)), MOUSE_WIDTH);
    }

    #[test]
    fn unit_values_use_decimal_comma() {
        assert_eq!(format_unit(0.634), "0,63");
        assert_eq!(format_unit(1.0), "1,00");
    }

    #[test]
    fn pen_stroke_inside_artboard_is_recorded() {
        let mut pen = PenState::default();
        pen.on_pen(TouchPhase::Start, pos2(1.0, 1.0), true, Some(0.2));
        pen.on_pen(TouchPhase::Move, pos2(2.0, 2.0), true, Some(0.8));
        pen.on_pen(TouchPhase::End, pos2(2.0, 2.0), true, None);
        assert_eq!(pen.strokes.len(), 1);
        let widths: Vec<f32> = pen.strokes[0].points.iter().map(|p| p.width).collect();
        assert_eq!(
            widths,
            vec![stroke_width(Some(0.2)), stroke_width(Some(0.8))]
        );
        assert!(!pen.in_contact);
        assert!(!pen.is_drawing());
    }

    #[test]
    fn pen_down_outside_artboard_only_updates_readout() {
        let mut pen = PenState::default();
        pen.on_pen(TouchPhase::Start, pos2(-5.0, 1.0), false, Some(0.5));
        pen.on_pen(TouchPhase::Move, pos2(3.0, 1.0), true, Some(0.6));
        assert!(pen.strokes.is_empty());
        assert_eq!(pen.pressure, Some(0.6));
        assert!(pen.in_contact);
    }

    #[test]
    fn hovering_keeps_last_pressure() {
        let mut pen = PenState::default();
        pen.on_pen(TouchPhase::Start, pos2(0.0, 0.0), true, Some(0.5));
        pen.on_pen(TouchPhase::Move, pos2(1.0, 0.0), true, None);
        assert_eq!(pen.pressure, Some(0.5));
    }

    #[test]
    fn readout_texts() {
        let mut pen = PenState::default();
        assert_eq!(pen.pressure_text(), "Pressão: —");
        assert_eq!(pen.contact_text(), "Ponta: no ar");
        pen.on_pen(TouchPhase::Start, pos2(0.0, 0.0), true, Some(0.25));
        assert_eq!(pen.pressure_text(), "Pressão: 0,25");
        assert_eq!(pen.contact_text(), "Ponta: encostada");
    }

    #[test]
    fn point_budget_is_enforced() {
        let mut pen = PenState::default();
        pen.begin_stroke();
        for i in 0..MAX_POINTS + 10 {
            pen.push_point(pos2(i as f32, 0.0), None);
        }
        assert_eq!(pen.strokes[0].points.len(), MAX_POINTS);
        pen.clear();
        assert!(pen.strokes.is_empty());
    }
}
