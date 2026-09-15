//! Navegación por teclado de los menús: qué se puede resaltar y cómo se mueve.
//!
//! El resaltado es el MISMO `HitTarget` / `hovered` que dibuja el hover del
//! mouse, así que las flechas no traen un segundo sistema de foco que se pueda
//! desincronizar del dibujo (misma idea que `hit_scale`): cada target de acá
//! abajo ya tiene su render de "hot".

use super::*;

/// Targets que puede resaltar el teclado, en el orden visual del panel.
/// `tray` son los items del menú del tray (vacía en los demás paneles): los
/// deshabilitados se saltean para que las flechas no se frenen en filas grises.
///
/// Los campos de texto (`HexField`, `NameField`) NO son stops: su resaltado
/// sale de `custom_focus` (tomar foco escribiendo), no de `hovered`, así que un
/// stop ahí sería invisible. Se clickean para escribir.
pub fn panel_targets(
    controls: &[Control],
    settings: &DockSettings,
    panel_width: f32,
    tray: &[crate::tray::TrayMenuItem],
) -> Vec<HitTarget> {
    let mut out = Vec::new();
    for c in controls {
        match c.kind {
            ControlKind::Toggle(id) => out.push(HitTarget::Toggle(id)),
            // el slider se resalta como track: Enter no lo activa y ←/→ lo ajusta
            ControlKind::Slider(id) => out.push(HitTarget::SliderTrack(id)),
            ControlKind::Button(b) => out.push(HitTarget::Button(b)),
            ControlKind::AppEntry(i) => out.push(HitTarget::AppEntry(i)),
            ControlKind::IconChoice(i) => out.push(HitTarget::IconChoice(i)),
            ControlKind::TrayItem(i) => {
                if tray.get(i).is_none_or(|item| item.enabled) {
                    out.push(HitTarget::TrayItem(i));
                }
            }
            ControlKind::EdgePicker => {
                out.extend(EDGE_OPTIONS.iter().map(|&e| HitTarget::Edge(e)));
            }
            ControlKind::AlignPicker => {
                out.extend(ALIGN_VALUES.iter().map(|&a| HitTarget::Align(a)));
            }
            ControlKind::PaletteModePicker => {
                out.push(HitTarget::PaletteMode(false));
                out.push(HitTarget::PaletteMode(true));
            }
            ControlKind::PanelBlendPicker => {
                out.extend(
                    PANEL_BLEND_ACCENT_PCT
                        .iter()
                        .map(|&p| HitTarget::PanelBlend(p)),
                );
            }
            ControlKind::ThemePicker => {
                for r in theme_picker_layout(settings, panel_width, c.y).rects {
                    out.push(HitTarget::ThemeOption(r.choice));
                    // el botón de borrar vive dentro del tile de la paleta propia
                    if let Some(i) = r.choice.filter(|i| *i >= THEME_PRESETS.len()) {
                        out.push(HitTarget::DeleteCustomPalette(i - THEME_PRESETS.len()));
                    }
                }
            }
            ControlKind::SchemeDropdown => out.push(HitTarget::SchemeDropdownToggle),
            ControlKind::DockFontDropdown => out.push(HitTarget::DockFontDropdownToggle),
            ControlKind::SystemFontDropdown => out.push(HitTarget::SystemFontDropdownToggle),
            ControlKind::HexField(_) | ControlKind::NameField => {}
            // los chips se arrastran entre slots: no hay nada que "activar"
            ControlKind::WidgetChips => {}
            ControlKind::VolumeRow(i) => out.push(HitTarget::VolumeTrack(i)),
            ControlKind::VolumeDevice(i) => out.push(HitTarget::VolumeDevice(i)),
            // el calendario no tiene stops: ←/→ cambian de mes, no hay resaltado
            ControlKind::Calendar(_) => {}
            ControlKind::SearchBox
            | ControlKind::Section(_)
            | ControlKind::Note(_)
            | ControlKind::IconHeader(_)
            | ControlKind::TraySeparator => {}
        }
    }
    out
}

/// Mueve el resaltado `dir` posiciones dentro de `targets`, recortando en los
/// extremos (igual que el launcher). `None` si no hay nada que resaltar.
pub fn nav_step(targets: &[HitTarget], cur: Option<HitTarget>, dir: isize) -> Option<HitTarget> {
    if targets.is_empty() {
        return None;
    }
    let last = targets.len() as isize - 1;
    let next = match cur.and_then(|c| targets.iter().position(|t| *t == c)) {
        Some(i) => (i as isize + dir).clamp(0, last),
        // primera flecha: desde arriba entra por el primero, desde abajo por el último
        None if dir > 0 => 0,
        None => last,
    };
    targets.get(next as usize).copied()
}

#[cfg(test)]
mod keynav_tests {
    use super::*;

    fn button(b: ButtonKind) -> Control {
        Control {
            kind: ControlKind::Button(b),
            y: 0.0,
            height: BUTTON_HEIGHT,
        }
    }

    fn targets() -> Vec<HitTarget> {
        vec![
            HitTarget::Toggle(SettingId::Autohide),
            HitTarget::SliderTrack(SettingId::DockScale),
            HitTarget::Button(ButtonKind::Back),
        ]
    }

    #[test]
    fn la_primera_flecha_entra_por_el_extremo_que_corresponde() {
        let t = targets();
        assert_eq!(nav_step(&t, None, 1), Some(t[0]));
        assert_eq!(nav_step(&t, None, -1), Some(t[2]));
        assert_eq!(nav_step(&[], None, 1), None);
    }

    #[test]
    fn recorta_en_los_extremos_y_no_da_la_vuelta() {
        let t = targets();
        assert_eq!(nav_step(&t, Some(t[0]), -1), Some(t[0]));
        assert_eq!(nav_step(&t, Some(t[2]), 1), Some(t[2]));
        assert_eq!(nav_step(&t, Some(t[0]), 1), Some(t[1]));
        // un resaltado que ya no está en la lista (cambió de pestaña) no rompe
        assert_eq!(
            nav_step(&t, Some(HitTarget::Button(ButtonKind::QuitDock)), 1),
            Some(t[0])
        );
    }

    #[test]
    fn los_deshabilitados_del_tray_no_son_stop() {
        let controls = vec![
            Control {
                kind: ControlKind::TrayItem(0),
                y: 0.0,
                height: TRAY_ITEM_HEIGHT,
            },
            Control {
                kind: ControlKind::TrayItem(1),
                y: 0.0,
                height: TRAY_ITEM_HEIGHT,
            },
        ];
        let items = vec![
            crate::tray::TrayMenuItem {
                id: 0,
                label: "off".into(),
                enabled: false,
                is_separator: false,
                has_submenu: false,
            },
            crate::tray::TrayMenuItem {
                id: 1,
                label: "on".into(),
                enabled: true,
                is_separator: false,
                has_submenu: false,
            },
        ];
        let settings = DockSettings::default();
        let t = panel_targets(&controls, &settings, MENU_WIDTH, &items);
        assert_eq!(t, vec![HitTarget::TrayItem(1)]);
    }

    #[test]
    fn el_panel_de_ajustes_incluye_pestanas_antes_que_controles() {
        let settings = DockSettings::default();
        let controls = vec![
            button(ButtonKind::AddApp),
            Control {
                kind: ControlKind::Slider(SettingId::DockScale),
                y: 0.0,
                height: SLIDER_ROW_HEIGHT,
            },
        ];
        let mut t: Vec<HitTarget> = MENU_CATEGORIES.iter().map(|&c| HitTarget::Tab(c)).collect();
        assert_eq!(t.len(), MENU_CATEGORIES.len());
        t.extend(panel_targets(
            &controls,
            &settings,
            DOCK_MENU_RIGHT_COL_W,
            &[],
        ));
        assert_eq!(
            t[MENU_CATEGORIES.len()],
            HitTarget::Button(ButtonKind::AddApp)
        );
        assert_eq!(
            t[MENU_CATEGORIES.len() + 1],
            HitTarget::SliderTrack(SettingId::DockScale)
        );
    }
}
