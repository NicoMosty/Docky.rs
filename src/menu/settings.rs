use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingId {
    DockScale,
    WidgetScale,
    FontScale,
    /// Letra del widget elegido en el tab "Widgets" (multiplica a `FontScale`).
    WidgetFontScale,
    /// Símbolo del widget elegido (hoy sólo la bocina del botón de volumen).
    WidgetIconScale,
    /// Reloj en 24 h. Sólo aparece cuando el widget elegido es `Clock`.
    WidgetClockFormat,
    IconSize,
    IconGap,
    WidthPadding,
    PosY,
    CornerRadius,
    BorderWidth,
    MenuCornerRadius,
    MenuBorderWidth,
    /// Deslizamiento/fundido de los paneles (overlay y ajustes). Ver
    /// `DockSettings::smooth_transitions`.
    SmoothTransitions,
    /// Aplica el matugen del usuario a las apps (kitty, waybar, rofi, …) con el
    /// "Matugen Style" del dock. Ver `DockSettings::matugen_apps`.
    MatugenApps,
    /// Modo light para matugen (dock y apps). Ver `DockSettings::matugen_light`.
    MatugenLight,
    Transparency,
    BlurEnabled,
    BlurPasses,
    BlurSize,
    BlurVibrancy,
    BlurBrightness,
    BlurContrast,
    BlurXray,
    MediaSmoothScroll,
    MediaWidthScale,
    Autohide,
    AutohideDelay,
    /// Tope de resultados del launcher (`DockSettings::launcher_max_results`).
    LauncherMaxResults,
    /// Orden del launcher por uso en vez de alfabético.
    LauncherSortByUsage,
    /// Tope del historial del portapapeles.
    ClipboardItems,
    /// Tope del historial de avisos.
    NotificationHistory,
}

impl SettingId {
    pub fn label(self) -> &'static str {
        match self {
            SettingId::DockScale => "Dock Size",
            SettingId::WidgetScale => "Widget Size",
            SettingId::FontScale => "Font Size",
            SettingId::WidgetFontScale => "Widget Font",
            SettingId::WidgetIconScale => "Widget Icon",
            SettingId::WidgetClockFormat => "24-Hour Clock",
            SettingId::IconSize => "Icon Size",
            SettingId::IconGap => "Icon Gaps",
            SettingId::WidthPadding => "Dock Width",
            SettingId::PosY => "Position Y",
            SettingId::CornerRadius => "Dock Roundness",
            SettingId::BorderWidth => "Dock Border",
            SettingId::MenuCornerRadius => "Menu Roundness",
            SettingId::MenuBorderWidth => "Menu Border",
            SettingId::SmoothTransitions => "Smooth Transitions",
            SettingId::MatugenApps => "Matugen Apps",
            SettingId::MatugenLight => "Light Mode",
            SettingId::Transparency => "Transparency",
            SettingId::BlurEnabled => "Blur",
            SettingId::BlurPasses => "Blur Passes",
            SettingId::BlurSize => "Blur Size",
            SettingId::BlurVibrancy => "Blur Vibrancy",
            SettingId::BlurBrightness => "Blur Brightness",
            SettingId::BlurContrast => "Blur Contrast",
            SettingId::BlurXray => "X-Ray",
            SettingId::MediaSmoothScroll => "Media Smooth Scroll",
            SettingId::MediaWidthScale => "Media Widget Width",
            SettingId::Autohide => "Autohide",
            SettingId::AutohideDelay => "Hide Delay",
            SettingId::LauncherMaxResults => "Max Results",
            SettingId::LauncherSortByUsage => "Sort by Usage",
            SettingId::ClipboardItems => "Clipboard Items",
            SettingId::NotificationHistory => "Notification History",
        }
    }

    pub fn range(self) -> (f32, f32, f32) {
        match self {
            SettingId::DockScale => (0.3, 2.0, 0.01),
            SettingId::WidgetScale => (0.3, 2.0, 0.01),
            SettingId::FontScale => (0.5, 2.0, 0.05),
            SettingId::WidgetFontScale => (
                crate::config::SIZE_SCALE_MIN,
                crate::config::SIZE_SCALE_MAX,
                0.05,
            ),
            SettingId::WidgetIconScale => (
                crate::config::SIZE_SCALE_MIN,
                crate::config::SIZE_SCALE_MAX,
                0.1,
            ),
            SettingId::WidgetClockFormat => (0.0, 1.0, 1.0),
            SettingId::IconSize => (28.0, 96.0, 1.0),
            SettingId::IconGap => (0.0, 40.0, 1.0),
            SettingId::WidthPadding => (0.0, 2400.0, 1.0),
            SettingId::PosY => (0.0, 80.0, 1.0),
            SettingId::CornerRadius | SettingId::MenuCornerRadius => (0.0, 40.0, 1.0),
            SettingId::BorderWidth | SettingId::MenuBorderWidth => (0.0, 5.0, 0.01),
            SettingId::Transparency => (0.0, 1.0, 0.01),
            SettingId::BlurPasses => (1.0, 8.0, 1.0),
            SettingId::BlurSize => (1.0, 20.0, 1.0),
            SettingId::BlurVibrancy => (0.0, 1.0, 0.01),
            SettingId::BlurBrightness => (0.0, 2.0, 0.01),
            SettingId::BlurContrast => (0.0, 2.0, 0.01),
            SettingId::BlurEnabled | SettingId::BlurXray | SettingId::MediaSmoothScroll => {
                (0.0, 1.0, 1.0)
            }
            SettingId::SmoothTransitions | SettingId::MatugenApps | SettingId::MatugenLight => {
                (0.0, 1.0, 1.0)
            }
            SettingId::Autohide => (0.0, 1.0, 1.0),
            SettingId::MediaWidthScale => (0.3, 2.0, 0.01),
            SettingId::AutohideDelay => (100.0, 2000.0, 50.0),
            SettingId::LauncherMaxResults => (10.0, 200.0, 10.0),
            SettingId::ClipboardItems => (10.0, 200.0, 10.0),
            SettingId::NotificationHistory => (10.0, 200.0, 10.0),
            SettingId::LauncherSortByUsage => (0.0, 1.0, 1.0),
        }
    }

    pub fn is_toggle(self) -> bool {
        matches!(
            self,
            SettingId::BlurEnabled
                | SettingId::BlurXray
                | SettingId::MediaSmoothScroll
                | SettingId::Autohide
                | SettingId::SmoothTransitions
                | SettingId::MatugenApps
                | SettingId::MatugenLight
                | SettingId::WidgetClockFormat
                | SettingId::LauncherSortByUsage
        )
    }

    pub fn get(self, s: &DockSettings) -> f32 {
        match self {
            SettingId::DockScale => s.dock_scale,
            SettingId::WidgetScale => s.widget_scale,
            SettingId::FontScale => s.font_scale,
            // ----- las dos filas del editor leen del widget elegido: sin
            // selección muestran el valor neutro -----
            SettingId::WidgetFontScale => s
                .selected_widget
                .and_then(|kind| s.widget_options(kind))
                .and_then(|o| o.font_scale)
                .unwrap_or(1.0),
            SettingId::WidgetIconScale => s
                .selected_widget
                .and_then(|kind| s.widget_options(kind))
                .and_then(|o| o.icon_scale)
                .unwrap_or(1.0),
            SettingId::WidgetClockFormat => bool_f(
                s.selected_widget
                    .is_some_and(|kind| s.widget_clock_24h(kind)),
            ),
            SettingId::IconSize => s.icon_size,
            SettingId::IconGap => s.icon_gap,
            SettingId::WidthPadding => s.width_padding,
            SettingId::PosY => s.pos_y as f32,
            SettingId::CornerRadius => s.corner_radius,
            SettingId::BorderWidth => s.border_width,
            SettingId::MenuCornerRadius => s.menu_corner_radius,
            SettingId::MenuBorderWidth => s.menu_border_width,
            SettingId::SmoothTransitions => bool_f(s.smooth_transitions),
            SettingId::MatugenApps => bool_f(s.matugen_apps),
            SettingId::MatugenLight => bool_f(s.matugen_light),
            SettingId::Transparency => s.transparency,
            SettingId::BlurEnabled => bool_f(s.blur_enabled),
            SettingId::BlurPasses => s.blur_passes as f32,
            SettingId::BlurSize => s.blur_size as f32,
            SettingId::BlurVibrancy => s.blur_vibrancy,
            SettingId::BlurBrightness => s.blur_brightness,
            SettingId::BlurContrast => s.blur_contrast,
            SettingId::BlurXray => bool_f(s.blur_xray),
            SettingId::MediaSmoothScroll => bool_f(s.media_smooth_scroll),
            SettingId::MediaWidthScale => s.media_width_scale,
            SettingId::Autohide => bool_f(s.autohide),
            SettingId::AutohideDelay => s.autohide_delay_ms as f32,
            SettingId::LauncherMaxResults => s.launcher_max_results as f32,
            SettingId::LauncherSortByUsage => bool_f(s.launcher_sort_by_usage),
            SettingId::ClipboardItems => s.clipboard_items as f32,
            SettingId::NotificationHistory => s.notif_history as f32,
        }
    }

    pub fn set(self, s: &mut DockSettings, value: f32) {
        let (min, max, _) = self.range();
        let clamped = value.clamp(min, max);
        match self {
            SettingId::DockScale => s.dock_scale = clamped,
            SettingId::WidgetScale => s.widget_scale = clamped,
            SettingId::FontScale => s.font_scale = clamped,
            SettingId::WidgetFontScale => {
                if let Some(kind) = s.selected_widget {
                    let mut options = s.widget_options(kind).cloned().unwrap_or_default();
                    options.font_scale = Some(clamped);
                    s.set_widget_options(kind, options);
                }
            }
            SettingId::WidgetIconScale => {
                if let Some(kind) = s.selected_widget {
                    let mut options = s.widget_options(kind).cloned().unwrap_or_default();
                    options.icon_scale = Some(clamped);
                    s.set_widget_options(kind, options);
                }
            }
            SettingId::WidgetClockFormat => {
                if let Some(kind) = s.selected_widget {
                    let mut options = s.widget_options(kind).cloned().unwrap_or_default();
                    options.clock_24h = Some(clamped >= 0.5);
                    s.set_widget_options(kind, options);
                }
            }
            SettingId::IconSize => s.icon_size = clamped,
            SettingId::IconGap => s.icon_gap = clamped,
            SettingId::WidthPadding => s.width_padding = clamped,
            SettingId::PosY => s.pos_y = clamped.round() as i32,
            SettingId::CornerRadius => s.corner_radius = clamped,
            SettingId::BorderWidth => s.border_width = clamped,
            SettingId::MenuCornerRadius => s.menu_corner_radius = clamped,
            SettingId::MenuBorderWidth => s.menu_border_width = clamped,
            SettingId::Transparency => s.transparency = clamped,
            SettingId::BlurEnabled => s.blur_enabled = clamped >= 0.5,
            SettingId::SmoothTransitions => s.smooth_transitions = clamped >= 0.5,
            SettingId::MatugenApps => s.matugen_apps = clamped >= 0.5,
            SettingId::MatugenLight => s.matugen_light = clamped >= 0.5,
            SettingId::BlurPasses => s.blur_passes = clamped.round() as i32,
            SettingId::BlurSize => s.blur_size = clamped.round() as i32,
            SettingId::BlurVibrancy => s.blur_vibrancy = clamped,
            SettingId::BlurBrightness => s.blur_brightness = clamped,
            SettingId::BlurContrast => s.blur_contrast = clamped,
            SettingId::BlurXray => s.blur_xray = clamped >= 0.5,
            SettingId::MediaSmoothScroll => s.media_smooth_scroll = clamped >= 0.5,
            SettingId::MediaWidthScale => s.media_width_scale = clamped,
            SettingId::Autohide => s.autohide = clamped >= 0.5,
            SettingId::AutohideDelay => s.autohide_delay_ms = clamped.round() as u64,
            SettingId::LauncherMaxResults => s.launcher_max_results = clamped.round() as usize,
            SettingId::LauncherSortByUsage => s.launcher_sort_by_usage = clamped >= 0.5,
            SettingId::ClipboardItems => s.clipboard_items = clamped.round() as usize,
            SettingId::NotificationHistory => s.notif_history = clamped.round() as usize,
        }
    }

    pub fn toggle(self, s: &mut DockSettings) {
        let cur = self.get(s);
        self.set(s, if cur >= 0.5 { 0.0 } else { 1.0 });
    }

    pub fn display_value(self, s: &DockSettings) -> String {
        match self {
            SettingId::IconSize
            | SettingId::IconGap
            | SettingId::WidthPadding
            | SettingId::PosY
            | SettingId::CornerRadius
            | SettingId::MenuCornerRadius
            | SettingId::BlurSize => format!("{}px", self.get(s).round() as i32),
            SettingId::BlurPasses => format!("{}", self.get(s).round() as i32),
            SettingId::DockScale
            | SettingId::WidgetScale
            | SettingId::FontScale
            | SettingId::WidgetFontScale
            | SettingId::WidgetIconScale
            | SettingId::MediaWidthScale => {
                format!("{}%", (self.get(s) * 100.0).round() as i32)
            }
            SettingId::Transparency | SettingId::BlurVibrancy => {
                format!("{}%", (self.get(s) * 100.0).round() as i32)
            }
            SettingId::BlurBrightness | SettingId::BlurContrast => format!("{:.2}", self.get(s)),
            SettingId::BorderWidth | SettingId::MenuBorderWidth => format!("{:.2}px", self.get(s)),
            SettingId::AutohideDelay => format!("{}ms", self.get(s).round() as i32),
            SettingId::LauncherMaxResults => format!("{} apps", self.get(s).round() as i32),
            SettingId::ClipboardItems | SettingId::NotificationHistory => {
                format!("{} items", self.get(s).round() as i32)
            }
            SettingId::BlurEnabled
            | SettingId::BlurXray
            | SettingId::MediaSmoothScroll
            | SettingId::Autohide
            | SettingId::SmoothTransitions
            | SettingId::MatugenApps
            | SettingId::MatugenLight
            | SettingId::WidgetClockFormat
            | SettingId::LauncherSortByUsage => String::new(),
        }
    }

    pub fn affects_layout(self) -> bool {
        matches!(
            self,
            SettingId::DockScale
                | SettingId::WidgetScale
                | SettingId::FontScale
                | SettingId::WidgetFontScale
                | SettingId::WidgetIconScale
                | SettingId::WidgetClockFormat
                | SettingId::IconSize
                | SettingId::IconGap
                | SettingId::WidthPadding
                | SettingId::PosY
                | SettingId::MediaWidthScale
                | SettingId::Autohide
        )
    }

    pub fn lua_blur_field(self) -> Option<&'static str> {
        match self {
            SettingId::BlurEnabled => Some("enabled"),
            SettingId::BlurPasses => Some("passes"),
            SettingId::BlurSize => Some("size"),
            SettingId::BlurVibrancy => Some("vibrancy"),
            SettingId::BlurBrightness => Some("brightness"),
            SettingId::BlurContrast => Some("contrast"),
            SettingId::BlurXray => Some("xray"),
            _ => None,
        }
    }

    pub fn hyprctl_value(self, s: &DockSettings) -> String {
        match self {
            SettingId::BlurEnabled | SettingId::BlurXray => {
                if self.get(s) >= 0.5 {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            SettingId::BlurPasses | SettingId::BlurSize => {
                format!("{}", self.get(s).round() as i32)
            }
            _ => format!("{:.3}", self.get(s)),
        }
    }
}

fn bool_f(b: bool) -> f32 {
    if b { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod menu_border_tests {
    use super::*;
    use crate::menu::controls::APPEARANCE_SETTINGS;

    /// Los ajustes de borde de menús tienen que estar expuestos en el panel:
    /// sin esto el setting existe pero el usuario no lo puede cambiar (pasó
    /// con Menu Roundness, que estaba en SettingId pero fuera de la lista).
    #[test]
    fn los_ajustes_de_menu_estan_en_appearance() {
        assert!(APPEARANCE_SETTINGS.contains(&SettingId::MenuCornerRadius));
        assert!(APPEARANCE_SETTINGS.contains(&SettingId::MenuBorderWidth));
    }

    /// Roundtrip con clamp: el radio recorta a [0, 40], el borde a [0, 5].
    #[test]
    fn menu_roundness_y_border_con_clamp() {
        let mut s = DockSettings::default();
        SettingId::MenuCornerRadius.set(&mut s, 4.0);
        assert_eq!(SettingId::MenuCornerRadius.get(&s), 4.0);
        SettingId::MenuCornerRadius.set(&mut s, 99.0);
        assert_eq!(SettingId::MenuCornerRadius.get(&s), 40.0);
        SettingId::MenuBorderWidth.set(&mut s, 2.5);
        assert_eq!(SettingId::MenuBorderWidth.get(&s), 2.5);
        SettingId::MenuBorderWidth.set(&mut s, -1.0);
        assert_eq!(SettingId::MenuBorderWidth.get(&s), 0.0);
    }

    /// El tamaño de letra existe, recorta a [0.5, 2.0], sale en Appearance y
    /// fuerza relayout (los anchos de las pastillas dependen de la letra).
    #[test]
    fn el_tamano_de_letra_es_ajuste_de_layout() {
        let mut s = DockSettings::default();
        assert_eq!(s.font_scale, 1.0);
        assert_eq!(SettingId::FontScale.label(), "Font Size");
        SettingId::FontScale.set(&mut s, 1.5);
        assert_eq!(SettingId::FontScale.get(&s), 1.5);
        SettingId::FontScale.set(&mut s, 99.0);
        assert_eq!(SettingId::FontScale.get(&s), 2.0);
        SettingId::FontScale.set(&mut s, 0.0);
        assert_eq!(SettingId::FontScale.get(&s), 0.5);
        assert!(APPEARANCE_SETTINGS.contains(&SettingId::FontScale));
        assert!(SettingId::FontScale.affects_layout());
    }

    /// Las transiciones suaves se apagan desde el panel: si el ajuste existe pero
    /// no está en la lista, el usuario no lo puede tocar (el mismo caso que Menu
    /// Roundness).
    #[test]
    fn las_transiciones_estan_en_appearance_y_son_toggle() {
        assert!(APPEARANCE_SETTINGS.contains(&SettingId::SmoothTransitions));
        assert!(SettingId::SmoothTransitions.is_toggle());
        // ----- no pide relayout: no cambia ninguna medida del dock -----
        assert!(!SettingId::SmoothTransitions.affects_layout());
        // ----- y el default es prendidas (el comportamiento de siempre) -----
        let mut s = DockSettings::default();
        assert!(s.smooth_transitions);
        assert_eq!(SettingId::SmoothTransitions.get(&s), 1.0);
        SettingId::SmoothTransitions.set(&mut s, 0.0);
        assert!(!s.smooth_transitions);
        assert_eq!(SettingId::SmoothTransitions.get(&s), 0.0);
        SettingId::SmoothTransitions.toggle(&mut s);
        assert!(s.smooth_transitions);
        // ----- y el panel la arma como fila de TOGGLE, no de slider -----
        let controls = crate::menu::build_category_controls(
            crate::menu::MenuCategory::Appearance,
            &DockSettings::default(),
        );
        assert!(
            controls.iter().any(|c| matches!(
                c.kind,
                crate::menu::ControlKind::Toggle(SettingId::SmoothTransitions)
            )),
            "tiene que estar en el panel de Appearance"
        );
    }

    /// La opción de matugen para las apps vive en Colors, es toggle y no pide
    /// relayout. Apagada por default: prenderla reescribe configs de otras apps.
    #[test]
    fn matugen_apps_esta_en_colors_y_arranca_apagado() {
        assert!(SettingId::MatugenApps.is_toggle());
        assert!(!SettingId::MatugenApps.affects_layout());
        let mut s = DockSettings::default();
        assert!(!s.matugen_apps);
        assert_eq!(SettingId::MatugenApps.get(&s), 0.0);
        SettingId::MatugenApps.toggle(&mut s);
        assert!(s.matugen_apps);
        assert_eq!(SettingId::MatugenApps.get(&s), 1.0);
        assert!(SettingId::MatugenApps.display_value(&s).is_empty());
        let controls = crate::menu::build_category_controls(
            crate::menu::MenuCategory::Colors,
            &DockSettings::default(),
        );
        assert!(
            controls.iter().any(|c| matches!(
                c.kind,
                crate::menu::ControlKind::Toggle(SettingId::MatugenApps)
            )),
            "tiene que estar en el panel de Colors"
        );
    }

    /// El modo light de matugen: toggle en Colors, apagado por default, y
    /// `matugen_mode()` traduce el bool a lo que espera el CLI.
    #[test]
    fn matugen_light_esta_en_colors_y_es_toggle() {
        assert!(SettingId::MatugenLight.is_toggle());
        assert!(!SettingId::MatugenLight.affects_layout());
        let mut s = DockSettings::default();
        assert!(!s.matugen_light);
        assert_eq!(s.matugen_mode(), "dark");
        SettingId::MatugenLight.toggle(&mut s);
        assert_eq!(SettingId::MatugenLight.get(&s), 1.0);
        assert_eq!(s.matugen_mode(), "light");
        assert!(SettingId::MatugenLight.display_value(&s).is_empty());
        let controls = crate::menu::build_category_controls(
            crate::menu::MenuCategory::Colors,
            &DockSettings::default(),
        );
        assert!(
            controls.iter().any(|c| matches!(
                c.kind,
                crate::menu::ControlKind::Toggle(SettingId::MatugenLight)
            )),
            "tiene que estar en el panel de Colors"
        );
    }

    /// Defaults que preservan el look actual: mismo radio que el dock, sin borde.
    #[test]
    fn defaults_iguales_al_dock() {
        let s = DockSettings::default();
        assert_eq!(s.menu_corner_radius, s.corner_radius);
        assert_eq!(s.menu_border_width, 0.0);
    }
}

#[cfg(test)]
mod launcher_settings_tests {
    use super::*;

    fn control_kinds() -> Vec<crate::menu::ControlKind> {
        crate::menu::build_category_controls(
            crate::menu::MenuCategory::Launcher,
            &DockSettings::default(),
        )
        .into_iter()
        .map(|c| c.kind)
        .collect()
    }

    /// La categoría existe y expone los cuatro: los topes del launcher/portapapeles/
    /// avisos y el orden por uso. Si falta uno, el setting existe pero nadie lo puede
    /// tocar (el mismo caso que Menu Roundness).
    #[test]
    fn la_categoria_launcher_expone_sus_ajustes() {
        assert!(crate::menu::MENU_CATEGORIES.contains(&crate::menu::MenuCategory::Launcher));
        let kinds = control_kinds();
        for id in [
            SettingId::LauncherMaxResults,
            SettingId::LauncherSortByUsage,
            SettingId::ClipboardItems,
            SettingId::NotificationHistory,
        ] {
            assert!(
                kinds.iter().any(|k| matches!(
                    k,
                    crate::menu::ControlKind::Slider(x) | crate::menu::ControlKind::Toggle(x) if *x == id
                )),
                "falta {id:?} en la categoría Launcher"
            );
        }
    }

    /// El orden por uso es un toggle, arranca prendido (el comportamiento de
    /// siempre) y no pide relayout: no cambia ninguna medida del dock.
    #[test]
    fn el_orden_por_uso_es_toggle_y_arranca_prendido() {
        assert!(SettingId::LauncherSortByUsage.is_toggle());
        assert!(!SettingId::LauncherSortByUsage.affects_layout());
        let mut s = DockSettings::default();
        assert!(s.launcher_sort_by_usage);
        assert_eq!(SettingId::LauncherSortByUsage.get(&s), 1.0);
        SettingId::LauncherSortByUsage.toggle(&mut s);
        assert!(!s.launcher_sort_by_usage);
        assert_eq!(SettingId::LauncherSortByUsage.get(&s), 0.0);
        assert!(SettingId::LauncherSortByUsage.display_value(&s).is_empty());
    }

    /// Los topes arrancan en el valor histórico (las constantes de cada panel) y el
    /// slider recorta al rango: si no, un valor fuera de rango en el JSON dejaría el
    /// panel pidiendo más de lo que soporta.
    #[test]
    fn los_topes_recortan_y_arrancan_en_el_valor_de_siempre() {
        let mut s = DockSettings::default();
        assert_eq!(s.launcher_max_results, crate::menu::SEARCH_MAX_RESULTS);
        assert_eq!(s.clipboard_items, crate::clipboard::MAX_ITEMS);
        assert_eq!(s.notif_history, crate::menu::NOTIF_HISTORY_CAP);

        SettingId::LauncherMaxResults.set(&mut s, 999.0);
        assert_eq!(s.launcher_max_results, 200);
        SettingId::LauncherMaxResults.set(&mut s, 0.0);
        assert_eq!(s.launcher_max_results, 10);
        SettingId::ClipboardItems.set(&mut s, 75.0);
        assert_eq!(s.clipboard_items, 75);
        SettingId::NotificationHistory.set(&mut s, 130.0);
        assert_eq!(s.notif_history, 130);
    }
}
