//! Visual (golden image), accessibility and keyboard tests for the PoC.
//!
//! A missing reference image makes the test fail and writes `<name>.new.png`
//! next to it. Only the owner approves turning it into the reference.

use bivar_poc_ui_egui::app::PocApp;
use eframe::egui::{Key, Role, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, HarnessBuilder};
use std::sync::{Mutex, MutexGuard};

const WINDOW: Vec2 = Vec2::new(1280.0, 800.0);

/// GPU rendering crashes when several harnesses render at once on the
/// reference machine, so tests that render take turns.
static GPU: Mutex<()> = Mutex::new(());

fn gpu_turn() -> MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn builder(pixels_per_point: f32) -> HarnessBuilder<PocApp> {
    Harness::builder()
        .with_size(WINDOW)
        .with_pixels_per_point(pixels_per_point)
        .with_render_cursor(false)
        .with_context_setup(PocApp::setup)
}

fn harness(pixels_per_point: f32) -> Harness<'static, PocApp> {
    let mut harness = builder(pixels_per_point)
        .wgpu()
        .build_ui_state(|ui, app: &mut PocApp| app.show(ui), PocApp::default());
    harness.run();
    harness
}

fn harness_without_gpu() -> Harness<'static, PocApp> {
    let mut harness =
        builder(1.0).build_ui_state(|ui, app: &mut PocApp| app.show(ui), PocApp::default());
    harness.run();
    harness
}

fn focused_any(harness: &Harness<'_, PocApp>, label: &str) -> bool {
    harness.query_all_by_label(label).any(|n| n.is_focused())
}

#[test]
fn golden_main_window() {
    let _turn = gpu_turn();
    harness(1.0).snapshot("main_window");
}

#[test]
fn golden_main_window_at_150_percent() {
    let _turn = gpu_turn();
    harness(1.5).snapshot("main_window_150");
}

#[test]
fn golden_keyboard_focus_ring() {
    let _turn = gpu_turn();
    let mut harness = harness(1.0);
    harness
        .get_by_role_and_label(Role::CheckBox, "Visível: Texto 1")
        .focus();
    harness.run();
    harness.snapshot("focus_ring");
}

#[test]
fn controls_have_accessible_names_and_roles() {
    let harness = harness_without_gpu();
    for name in ["Protusão", "Peso", "Borrão"] {
        harness.get_by_role_and_label(Role::Slider, name);
        harness.get_by_role_and_label(Role::SpinButton, name);
    }
    for layer in ["Quadrado 1", "Componente 1", "Texto 1", "SVG 1"] {
        harness.get_by_role_and_label(Role::CheckBox, &format!("Visível: {layer}"));
        harness.get_by_role_and_label(Role::CheckBox, &format!("Bloqueada: {layer}"));
    }
    harness.get_by_role_and_label(Role::Button, "Destacar o painel Camadas em janela própria");
    harness.get_by_role_and_label(
        Role::Button,
        "Destacar o painel Propriedades do Objeto em janela própria",
    );
    harness.get_by_role_and_label(Role::Button, "Limpar desenho");
}

#[test]
fn tab_reaches_every_control() {
    let mut harness = harness_without_gpu();
    let expected = [
        "Destacar o painel Camadas em janela própria",
        "Visível: Quadrado 1",
        "Bloqueada: SVG 1",
        "Limpar desenho",
        "Destacar o painel Propriedades do Objeto em janela própria",
        "Protusão",
        "Peso",
        "Borrão",
    ];
    let mut reached = vec![false; expected.len()];
    for _ in 0..60 {
        harness.key_press(Key::Tab);
        harness.run();
        for (i, label) in expected.iter().enumerate() {
            reached[i] |= focused_any(&harness, label);
        }
    }
    let missing: Vec<_> = expected
        .iter()
        .zip(&reached)
        .filter(|(_, r)| !**r)
        .map(|(l, _)| *l)
        .collect();
    assert!(missing.is_empty(), "not reachable with Tab: {missing:?}");
}

#[test]
fn space_toggles_layer_visibility() {
    let mut harness = harness_without_gpu();
    harness
        .get_by_role_and_label(Role::CheckBox, "Visível: Texto 1")
        .focus();
    harness.run();
    harness.key_press(Key::Space);
    harness.run();
    assert!(!harness.state().layers.visible[2]);
}

#[test]
fn arrow_keys_move_a_focused_slider() {
    let mut harness = harness_without_gpu();
    harness
        .get_by_role_and_label(Role::Slider, "Borrão")
        .focus();
    harness.run();
    let before = harness.state().properties.values[2];
    harness.key_press(Key::ArrowRight);
    harness.run();
    assert!(harness.state().properties.values[2] > before);
}

#[test]
fn enter_on_detach_button_floats_and_dock_button_returns() {
    let mut harness = harness_without_gpu();
    harness
        .get_by_label("Destacar o painel Camadas em janela própria")
        .focus();
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    assert!(harness.state().placement[0].is_floating());

    harness
        .get_by_label("Acoplar o painel Camadas de volta à janela principal")
        .click();
    harness.run();
    assert!(!harness.state().placement[0].is_floating());
}
