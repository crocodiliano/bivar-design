//! The two panels: "Camadas" (layers) and "Propriedades do Objeto".

use eframe::egui::{
    self, Align, Color32, CornerRadius, DragValue, Frame, Layout, Rect, Response, Role, Sense,
    Slider, Stroke, Ui, Vec2, WidgetInfo, pos2, vec2,
};

use crate::dock;
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelKind {
    Layers,
    Properties,
}

impl PanelKind {
    pub const ALL: [Self; 2] = [Self::Layers, Self::Properties];

    pub fn title(self) -> &'static str {
        match self {
            Self::Layers => "Camadas",
            Self::Properties => "Propriedades do Objeto",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Layers => 0,
            Self::Properties => 1,
        }
    }

    pub fn size(self) -> Vec2 {
        match self {
            Self::Layers => vec2(264.0, 420.0),
            Self::Properties => vec2(280.0, 300.0),
        }
    }

    fn header_fill(self) -> Color32 {
        match self {
            Self::Layers => theme::CONTROL_BG,
            Self::Properties => theme::ACCENT,
        }
    }

    pub fn detach_label(self) -> String {
        format!("Destacar o painel {} em janela própria", self.title())
    }

    pub fn dock_label(self) -> String {
        format!(
            "Acoplar o painel {} de volta à janela principal",
            self.title()
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeaderEvent {
    None,
    /// The "Destacar"/"Acoplar" button was pressed.
    TogglePressed,
    /// The header was dragged out of the main window.
    DraggedOut {
        /// Pointer position, local to the main window.
        pointer: egui::Pos2,
        grab_offset: Vec2,
    },
}

pub const HEADER_HEIGHT: f32 = 34.0;

pub fn panel_frame() -> Frame {
    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(Stroke::new(1.0, theme::PANEL_BORDER))
        .corner_radius(CornerRadius::same(theme::PANEL_RADIUS))
}

/// Panel title bar. While docked it can be dragged out of the window.
pub fn header(ui: &mut Ui, kind: PanelKind, floating: bool) -> HeaderEvent {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        vec2(width, HEADER_HEIGHT),
        if floating {
            Sense::hover()
        } else {
            Sense::drag()
        },
    );
    let radius = theme::PANEL_RADIUS;
    ui.painter().rect_filled(
        rect,
        CornerRadius {
            nw: radius,
            ne: radius,
            sw: 0,
            se: 0,
        },
        kind.header_fill(),
    );
    ui.painter().text(
        rect.left_center() + vec2(12.0, 0.0),
        egui::Align2::LEFT_CENTER,
        kind.title(),
        theme::semibold(12.0),
        theme::TEXT,
    );

    let button_rect = Rect::from_min_max(
        pos2(rect.right() - 76.0, rect.top() + 6.0),
        pos2(rect.right() - 6.0, rect.bottom() - 6.0),
    );
    let (text, accessible) = if floating {
        ("Acoplar", kind.dock_label())
    } else {
        ("Destacar", kind.detach_label())
    };
    let button = ui
        .put(
            button_rect,
            egui::Button::new(egui::RichText::new(text).size(11.0)),
        )
        .on_hover_text(&accessible);
    button.widget_info(|| WidgetInfo::labeled(Role::Button, true, &accessible));
    if button.clicked() {
        return HeaderEvent::TogglePressed;
    }

    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let ctx = ui.ctx();
        let window = ctx.content_rect();
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        let origin = ctx.input(|i| i.pointer.press_origin());
        if let (Some(pointer), Some(origin)) = (pointer, origin)
            && dock::pointer_left_window(window, pointer)
        {
            return HeaderEvent::DraggedOut {
                pointer,
                grab_offset: origin - rect.min,
            };
        }
    } else if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    HeaderEvent::None
}

/// A round on/off toggle. The state is shown by shape (filled or hollow
/// circle), not only by color.
fn dot_toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let state = *on;
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, state, label));
    let center = rect.center();
    if state {
        ui.painter().circle_filled(center, 5.0, theme::TEXT);
    } else {
        ui.painter()
            .circle_stroke(center, 4.5, Stroke::new(1.5, theme::TEXT_MUTED));
    }
    response.on_hover_text(label)
}

pub const LAYER_NAMES: [&str; 4] = ["Quadrado 1", "Componente 1", "Texto 1", "SVG 1"];

#[derive(Clone, Debug)]
pub struct LayersState {
    pub selected: usize,
    pub visible: [bool; LAYER_NAMES.len()],
    pub locked: [bool; LAYER_NAMES.len()],
}

impl Default for LayersState {
    fn default() -> Self {
        Self {
            selected: 0,
            visible: [true; LAYER_NAMES.len()],
            locked: [false; LAYER_NAMES.len()],
        }
    }
}

pub fn visibility_label(name: &str) -> String {
    format!("Visível: {name}")
}

pub fn lock_label(name: &str) -> String {
    format!("Bloqueada: {name}")
}

pub fn layers_ui(ui: &mut Ui, state: &mut LayersState) {
    Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 10))
        .show(ui, |ui| {
            egui::CollapsingHeader::new(
                egui::RichText::new("Artboard 1").font(theme::semibold(12.0)),
            )
            .id_salt("artboard-1")
            .default_open(true)
            .show(ui, |ui| {
                for (i, name) in LAYER_NAMES.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (thumb, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        ui.painter()
                            .rect_filled(thumb, CornerRadius::same(3), theme::ARTBOARD_BG);
                        if ui
                            .add(egui::Button::selectable(state.selected == i, *name))
                            .clicked()
                        {
                            state.selected = i;
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            dot_toggle(ui, &mut state.locked[i], &lock_label(name));
                            dot_toggle(ui, &mut state.visible[i], &visibility_label(name));
                        });
                    });
                }
            });
        });
}

pub const PROPERTY_NAMES: [&str; 3] = ["Protusão", "Peso", "Borrão"];

#[derive(Clone, Debug)]
pub struct PropertiesState {
    pub values: [f32; PROPERTY_NAMES.len()],
}

impl Default for PropertiesState {
    fn default() -> Self {
        Self {
            values: [0.0, 100.0, 50.0],
        }
    }
}

pub fn properties_ui(ui: &mut Ui, state: &mut PropertiesState) {
    Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            for (name, value) in PROPERTY_NAMES.iter().zip(state.values.iter_mut()) {
                let label = ui
                    .horizontal(|ui| {
                        let label = ui.label(egui::RichText::new(*name).size(12.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add(DragValue::new(value).range(0.0..=100.0).speed(0.5))
                                .labelled_by(label.id);
                        });
                        label
                    })
                    .inner;
                ui.spacing_mut().slider_width = ui.available_width();
                ui.add(Slider::new(value, 0.0..=100.0).show_value(false))
                    .labelled_by(label.id);
                ui.add_space(6.0);
            }
        });
}

/// Panel title bar plus contents. Returns what happened in the title bar.
pub fn panel_ui(
    ui: &mut Ui,
    kind: PanelKind,
    floating: bool,
    layers: &mut LayersState,
    properties: &mut PropertiesState,
) -> HeaderEvent {
    let event = header(ui, kind, floating);
    match kind {
        PanelKind::Layers => layers_ui(ui, layers),
        PanelKind::Properties => properties_ui(ui, properties),
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_panel_has_distinct_title_and_index() {
        assert_ne!(PanelKind::Layers.title(), PanelKind::Properties.title());
        let indices: Vec<usize> = PanelKind::ALL.iter().map(|k| k.index()).collect();
        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn accessible_labels_name_the_panel() {
        for kind in PanelKind::ALL {
            assert!(kind.detach_label().contains(kind.title()));
            assert!(kind.dock_label().contains(kind.title()));
        }
    }

    #[test]
    fn layer_toggle_labels_name_the_layer() {
        for name in LAYER_NAMES {
            assert!(visibility_label(name).ends_with(name));
            assert!(lock_label(name).ends_with(name));
            assert_ne!(visibility_label(name), lock_label(name));
        }
    }

    #[test]
    fn default_property_values_are_in_range() {
        for v in PropertiesState::default().values {
            assert!((0.0..=100.0).contains(&v));
        }
    }
}
