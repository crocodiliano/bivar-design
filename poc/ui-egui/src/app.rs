//! Main window: layers panel, canvas, properties panel and detached windows.

use eframe::egui::{
    self, Align2, CentralPanel, Context, CornerRadius, Event, Frame, Id, LayerId, Order, Panel,
    Pos2, Rect, Stroke, StrokeKind, Ui, UiBuilder, ViewportBuilder, ViewportId, vec2,
};

use crate::dock::{self, Placement};
use crate::panels::{self, HeaderEvent, LayersState, PanelKind, PropertiesState};
use crate::pen::{self, PenState};
use crate::theme;

/// Where a panel opened with the keyboard appears, from the main window's corner.
const KEYBOARD_DETACH_OFFSET: egui::Vec2 = vec2(80.0, 80.0);
const STRIPE_SPACING: f32 = 14.0;
const ARTBOARD_ASPECT: f32 = 16.0 / 10.0;

#[derive(Default)]
pub struct PocApp {
    pub layers: LayersState,
    pub properties: PropertiesState,
    pub pen: PenState,
    pub placement: [Placement; 2],
    spawn_pos: [Option<Pos2>; 2],
}

impl PocApp {
    pub fn new(ctx: &Context) -> Self {
        Self::setup(ctx);
        Self::default()
    }

    /// Installs fonts and style on a context.
    pub fn setup(ctx: &Context) {
        theme::install_fonts(ctx);
        theme::apply_style(ctx);
    }

    fn handle_header(&mut self, kind: PanelKind, event: HeaderEvent, main_window: Option<Rect>) {
        let i = kind.index();
        let origin = main_window.map_or(Pos2::ZERO, |r| r.min);
        match event {
            HeaderEvent::None => {}
            HeaderEvent::TogglePressed if self.placement[i].is_floating() => self.dock(kind),
            HeaderEvent::TogglePressed => {
                self.float(kind, origin + KEYBOARD_DETACH_OFFSET);
            }
            HeaderEvent::DraggedOut {
                pointer,
                grab_offset,
            } => {
                let on_monitor = origin + pointer.to_vec2();
                self.float(kind, dock::detached_position(on_monitor, grab_offset));
            }
        }
    }

    fn float(&mut self, kind: PanelKind, at: Pos2) {
        self.placement[kind.index()] = Placement::floating();
        self.spawn_pos[kind.index()] = Some(at);
    }

    fn dock(&mut self, kind: PanelKind) {
        self.placement[kind.index()] = Placement::Docked;
        self.spawn_pos[kind.index()] = None;
    }

    fn docked_panel(&mut self, ui: &mut Ui, kind: PanelKind, main_window: Option<Rect>) {
        let event = panels::panel_frame()
            .show(ui, |ui| {
                ui.set_width(kind.size().x);
                panels::panel_ui(ui, kind, false, &mut self.layers, &mut self.properties)
            })
            .inner;
        self.handle_header(kind, event, main_window);
    }

    fn floating_panel(&mut self, ctx: &Context, kind: PanelKind, main_window: Option<Rect>) {
        let i = kind.index();
        let mut builder = ViewportBuilder::default()
            .with_title(format!("{} — Bivar", kind.title()))
            .with_inner_size(kind.size())
            .with_min_inner_size(vec2(200.0, 160.0));
        if let Some(pos) = self.spawn_pos[i] {
            builder = builder.with_position(pos);
        }
        ctx.show_viewport_immediate(ViewportId::from_hash_of(kind.title()), builder, |ui, _| {
            let event = CentralPanel::default()
                .frame(Frame::new().fill(theme::PANEL_BG))
                .show(ui, |ui| {
                    panels::panel_ui(ui, kind, true, &mut self.layers, &mut self.properties)
                })
                .inner;
            paint_focus_ring(ui.ctx());

            let (close, outer) = ui.ctx().input(|input| {
                (
                    input.viewport().close_requested(),
                    input.viewport().outer_rect,
                )
            });
            // Closing the window docks the panel: nothing is lost.
            if close || event == HeaderEvent::TogglePressed {
                self.dock(kind);
                return;
            }
            let Some(outer) = outer else { return };
            match self.placement[i] {
                Placement::Floating { first_seen: None } => {
                    self.placement[i] = Placement::Floating {
                        first_seen: Some(outer.min),
                    };
                }
                Placement::Floating {
                    first_seen: Some(first),
                } => {
                    if main_window.is_some_and(|main| dock::should_dock(main, outer, first)) {
                        self.dock(kind);
                    }
                }
                Placement::Docked => {}
            }
        });
    }

    fn canvas_ui(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::CANVAS_BG);
        paint_stripes(&painter, rect);

        // Leave room for the docked properties panel (left) and the pen readout (bottom).
        let space = Rect::from_min_max(
            rect.min + vec2(PanelKind::Properties.size().x + 48.0, 56.0),
            rect.max - vec2(40.0, 96.0),
        );
        let artboard = artboard_rect(space);
        painter.text(
            artboard.left_top() - vec2(0.0, 6.0),
            Align2::LEFT_BOTTOM,
            "Artboard 1",
            theme::semibold(13.0),
            theme::CANVAS_TEXT,
        );
        painter.rect_filled(artboard, 0.0, theme::ARTBOARD_BG);
        painter.rect_stroke(
            artboard,
            0.0,
            Stroke::new(1.0, theme::ARTBOARD_BORDER),
            StrokeKind::Outside,
        );

        let response = ui.interact(artboard, Id::unique("artboard"), egui::Sense::drag());
        let canvas_layer = ui.layer_id();
        let events = ui.input(|i| i.events.clone());
        let mut pen_event = false;
        for event in events {
            if let Event::Touch {
                phase, pos, force, ..
            } = event
            {
                pen_event = true;
                let inside =
                    artboard.contains(pos) && ui.ctx().layer_id_at(pos) == Some(canvas_layer);
                self.pen
                    .on_pen(phase, pos - artboard.min.to_vec2(), inside, force);
            }
        }
        // Mouse fallback. A pen also moves the mouse pointer, so skip while it
        // is in use to avoid drawing twice.
        if !pen_event && !self.pen.in_contact {
            if response.drag_started() {
                self.pen.begin_stroke();
            }
            if response.dragged()
                && let Some(p) = response.interact_pointer_pos()
            {
                self.pen.push_point(p - artboard.min.to_vec2(), None);
            }
            if response.drag_stopped() {
                self.pen.end_stroke();
            }
        }

        let ink = ui.painter_at(artboard);
        for stroke in &self.pen.strokes {
            for pair in stroke.points.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                ink.line_segment(
                    [
                        artboard.min + a.pos.to_vec2(),
                        artboard.min + b.pos.to_vec2(),
                    ],
                    Stroke::new(b.width, theme::INK),
                );
            }
            for p in &stroke.points {
                ink.circle_filled(artboard.min + p.pos.to_vec2(), p.width / 2.0, theme::INK);
            }
        }

        self.pen_readout(ui, rect);
    }

    fn pen_readout(&mut self, ui: &mut Ui, canvas: Rect) {
        let area = Rect::from_min_size(
            canvas.left_bottom() + vec2(16.0, -60.0),
            vec2(canvas.width() - 32.0, 44.0),
        );
        ui.scope_builder(UiBuilder::new().max_rect(area), |ui| {
            Frame::new()
                .fill(theme::PANEL_BG)
                .corner_radius(CornerRadius::same(theme::PANEL_RADIUS))
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Caneta").font(theme::semibold(12.0)));
                        ui.label(self.pen.pressure_text());
                        ui.label(self.pen.contact_text());
                        ui.label(egui::RichText::new(pen::TILT_TEXT).color(theme::TEXT_MUTED));
                        ui.label(egui::RichText::new(pen::ERASER_TEXT).color(theme::TEXT_MUTED));
                        if ui.button("Limpar desenho").clicked() {
                            self.pen.clear();
                        }
                    });
                });
        });
    }

    /// Draws the whole main window.
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let main_window = ctx.input(|i| i.viewport().inner_rect);

        if !self.placement[PanelKind::Layers.index()].is_floating() {
            Panel::left("layers-dock")
                .resizable(false)
                .exact_size(PanelKind::Layers.size().x + 16.0)
                .frame(
                    Frame::new()
                        .fill(theme::APP_BG)
                        .inner_margin(egui::Margin::same(8)),
                )
                .show(ui, |ui| {
                    self.docked_panel(ui, PanelKind::Layers, main_window)
                });
        }

        let canvas = CentralPanel::default()
            .frame(Frame::new().fill(theme::CANVAS_BG))
            .show(ui, |ui| {
                self.canvas_ui(ui);
                ui.max_rect()
            })
            .inner;

        if !self.placement[PanelKind::Properties.index()].is_floating() {
            egui::Area::new(Id::unique("properties-dock"))
                .fixed_pos(canvas.min + vec2(16.0, 16.0))
                .show(&ctx, |ui| {
                    self.docked_panel(ui, PanelKind::Properties, main_window)
                });
        }

        for kind in PanelKind::ALL {
            if self.placement[kind.index()].is_floating() {
                self.floating_panel(&ctx, kind, main_window);
            }
        }

        paint_focus_ring(&ctx);
    }
}

impl eframe::App for PocApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::from(theme::APP_BG).to_array()
    }
}

/// Largest rectangle with the artboard's aspect ratio, centered in `space`.
pub fn artboard_rect(space: Rect) -> Rect {
    let width = space.width().min(space.height() * ARTBOARD_ASPECT).max(1.0);
    Rect::from_center_size(space.center(), vec2(width, width / ARTBOARD_ASPECT))
}

fn paint_stripes(painter: &egui::Painter, rect: Rect) {
    let stroke = Stroke::new(1.0, theme::CANVAS_STRIPE);
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [
                egui::pos2(x, rect.bottom()),
                egui::pos2(x + rect.height(), rect.top()),
            ],
            stroke,
        );
        x += STRIPE_SPACING;
    }
}

/// Thick outline around the focused control, so focus never relies on color.
pub fn paint_focus_ring(ctx: &Context) {
    let Some(id) = ctx.memory(|m| m.focused()) else {
        return;
    };
    let Some(response) = ctx.read_response(id) else {
        return;
    };
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("focus-ring")));
    painter.rect_stroke(
        response.rect.expand(2.0),
        CornerRadius::same(theme::CONTROL_RADIUS + 2),
        Stroke::new(theme::FOCUS_RING_WIDTH, theme::FOCUS_RING),
        StrokeKind::Outside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn artboard_keeps_aspect_and_fits() {
        for space in [
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 400.0)),
            Rect::from_min_size(pos2(50.0, 20.0), vec2(300.0, 900.0)),
        ] {
            let a = artboard_rect(space);
            assert!((a.width() / a.height() - ARTBOARD_ASPECT).abs() < 1e-3);
            assert!(space.expand(0.01).contains_rect(a));
            assert!((a.center() - space.center()).length() < 1e-3);
        }
    }

    #[test]
    fn artboard_never_collapses() {
        let a = artboard_rect(Rect::from_min_size(pos2(0.0, 0.0), vec2(-10.0, -10.0)));
        assert!(a.width() > 0.0 && a.height() > 0.0);
    }

    #[test]
    fn keyboard_detach_then_dock_round_trip() {
        let ctx = Context::default();
        let mut app = PocApp::new(&ctx);
        let main = Some(Rect::from_min_size(pos2(100.0, 100.0), vec2(1200.0, 800.0)));
        app.handle_header(PanelKind::Layers, HeaderEvent::TogglePressed, main);
        assert!(app.placement[0].is_floating());
        assert_eq!(app.spawn_pos[0], Some(pos2(180.0, 180.0)));
        app.handle_header(PanelKind::Layers, HeaderEvent::TogglePressed, main);
        assert_eq!(app.placement[0], Placement::Docked);
        assert_eq!(app.placement[1], Placement::Docked);
    }

    #[test]
    fn dragging_out_opens_window_under_pointer() {
        let ctx = Context::default();
        let mut app = PocApp::new(&ctx);
        let main = Some(Rect::from_min_size(pos2(100.0, 100.0), vec2(1200.0, 800.0)));
        app.handle_header(
            PanelKind::Properties,
            HeaderEvent::DraggedOut {
                pointer: pos2(1300.0, 50.0),
                grab_offset: vec2(20.0, 10.0),
            },
            main,
        );
        assert!(app.placement[1].is_floating());
        assert_eq!(
            app.spawn_pos[1],
            Some(dock::detached_position(
                pos2(1400.0, 150.0),
                vec2(20.0, 10.0)
            ))
        );
    }
}
