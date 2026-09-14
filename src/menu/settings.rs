use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingId {
    DockScale,
    WidgetScale,
    IconSize,
    IconGap,
    WidthPadding,
    PosY,
    CornerRadius,
    BorderWidth,
    MenuCornerRadius,
    MenuBorderWidth,
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
}

impl SettingId {
    pub fn label(self) -> &'static str {
        match self {
            SettingId::DockScale => "Dock Size",
            SettingId::WidgetScale => "Widget Size",
            SettingId::IconSize => "Icon Size",
            SettingId::IconGap => "Icon Gaps",
            SettingId::WidthPadding => "Dock Width",
            SettingId::PosY => "Position Y",
            SettingId::CornerRadius => "Dock Roundness",
            SettingId::BorderWidth => "Dock Border",
            SettingId::MenuCornerRadius => "Menu Roundness",
            SettingId::MenuBorderWidth => "Menu Border",
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
        }
    }

    pub fn range(self) -> (f32, f32, f32) {
        match self {
            SettingId::DockScale => (0.3, 2.0, 0.01),
            SettingId::WidgetScale => (0.3, 2.0, 0.01),
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
            SettingId::Autohide => (0.0, 1.0, 1.0),
            SettingId::MediaWidthScale => (0.3, 2.0, 0.01),
            SettingId::AutohideDelay => (100.0, 2000.0, 50.0),
        }
    }

    pub fn is_toggle(self) -> bool {
        matches!(
            self,
            SettingId::BlurEnabled
                | SettingId::BlurXray
                | SettingId::MediaSmoothScroll
                | SettingId::Autohide
        )
    }

    pub fn get(self, s: &DockSettings) -> f32 {
        match self {
            SettingId::DockScale => s.dock_scale,
            SettingId::WidgetScale => s.widget_scale,
            SettingId::IconSize => s.icon_size,
            SettingId::IconGap => s.icon_gap,
            SettingId::WidthPadding => s.width_padding,
            SettingId::PosY => s.pos_y as f32,
            SettingId::CornerRadius => s.corner_radius,
            SettingId::BorderWidth => s.border_width,
            SettingId::MenuCornerRadius => s.menu_corner_radius,
            SettingId::MenuBorderWidth => s.menu_border_width,
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
        }
    }

    pub fn set(self, s: &mut DockSettings, value: f32) {
        let (min, max, _) = self.range();
        let clamped = value.clamp(min, max);
        match self {
            SettingId::DockScale => s.dock_scale = clamped,
            SettingId::WidgetScale => s.widget_scale = clamped,
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
            SettingId::DockScale | SettingId::WidgetScale | SettingId::MediaWidthScale => {
                format!("{}%", (self.get(s) * 100.0).round() as i32)
            }
            SettingId::Transparency | SettingId::BlurVibrancy => {
                format!("{}%", (self.get(s) * 100.0).round() as i32)
            }
            SettingId::BlurBrightness | SettingId::BlurContrast => format!("{:.2}", self.get(s)),
            SettingId::BorderWidth | SettingId::MenuBorderWidth => format!("{:.2}px", self.get(s)),
            SettingId::AutohideDelay => format!("{}ms", self.get(s).round() as i32),
            SettingId::BlurEnabled
            | SettingId::BlurXray
            | SettingId::MediaSmoothScroll
            | SettingId::Autohide => String::new(),
        }
    }

    pub fn affects_layout(self) -> bool {
        matches!(
            self,
            SettingId::DockScale
                | SettingId::WidgetScale
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

    /// Defaults que preservan el look actual: mismo radio que el dock, sin borde.
    #[test]
    fn defaults_iguales_al_dock() {
        let s = DockSettings::default();
        assert_eq!(s.menu_corner_radius, s.corner_radius);
        assert_eq!(s.menu_border_width, 0.0);
    }
}
