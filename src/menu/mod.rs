use crate::config::{DockEdge, DockSettings, WidgetKind, WidgetPlacement, WidgetSlot};

mod notifications;
pub use notifications::*;
mod settings;
pub use settings::*;
mod controls;
pub use controls::*;
mod app_search;
pub use app_search::*;
mod dock_menu;
pub use dock_menu::*;
mod edge_picker;
pub use edge_picker::*;
mod palette_controls;
pub use palette_controls::*;
mod widget_chips;
pub use widget_chips::*;
mod theme_picker;
pub use theme_picker::*;
mod scheme_picker;
pub use scheme_picker::*;
mod font_dropdown;
pub use font_dropdown::*;
mod wallpaper_picker;
pub use wallpaper_picker::*;
mod align_picker;
pub use align_picker::*;
mod volume_panel;
pub use volume_panel::*;
mod keynav;
pub use keynav::*;
mod calendar;
pub use calendar::*;

pub const MENU_WIDTH: f32 = 230.0;
pub const MENU_PADDING: f32 = 10.0;
pub const SECTION_LABEL_HEIGHT: f32 = 16.0;
pub const BUTTON_HEIGHT: f32 = 26.0;
pub const SLIDER_ROW_HEIGHT: f32 = 32.0;
pub const TOGGLE_ROW_HEIGHT: f32 = 24.0;
pub const NOTE_ROW_HEIGHT: f32 = 16.0;
pub const ROW_GAP: f32 = 4.0;
pub const STEP_BTN_SIZE: f32 = 16.0;
pub const APP_ENTRY_HEIGHT: f32 = 30.0;
pub const LIST_ROW_GAP: f32 = 2.0;
pub const LIST_MAX_VISIBLE_ROWS: usize = 8;
pub const TAB_SLIDE_DISTANCE: f32 = 28.0;

/// Desplazamiento horizontal del contenido en un cambio de pestaña del overlay:
/// máximo (`TAB_SLIDE_DISTANCE`) al arrancar y 0 al terminar, para que el
/// contenido entre desde el lado hacia el que viajás con Shift+←/→.
///
/// `dir` es la dirección del ciclo (0 = abrir una pestaña directo, por IPC: sin
/// deslizamiento) y `t` el progreso 0..1 de la animación que cada modo **ya**
/// tenía (`anim`), así que no hace falta un temporizador ni un campo nuevos: los
/// frames que antes se gastaban redibujando el mismo cuadro opaco pasan a correr
/// el contenido. Misma fórmula que el panel de ajustes (`dock_menu.rs`).
pub fn overlay_slide_offset(dir: f32, t: f32) -> f32 {
    if dir == 0.0 {
        return 0.0;
    }
    let eased = 0.5 - 0.5 * (std::f32::consts::PI * t.clamp(0.0, 1.0)).cos();
    dir * (1.0 - eased) * TAB_SLIDE_DISTANCE
}
pub const WIDGETS_TAB_FIXED_H: f32 = 484.0;
pub const SEARCH_BOX_HEIGHT: f32 = 26.0;
pub const HEX_FIELD_H: f32 = 26.0;
pub const TRAY_ITEM_HEIGHT: f32 = 26.0;
pub const TRAY_SEPARATOR_HEIGHT: f32 = 9.0;
pub const CUSTOM_HEX_LABELS: [&str; 5] = ["Accent", "Accent 2", "Panel", "Text", "Text Dim"];

/// Ritmo de una animación: la **duración** de un recorrido 0→1, en ms. Reemplaza los
/// pasos por frame (`+= 0.07`), que ataban la velocidad al frame rate: a 144 Hz todo iba
/// 2,4x más rápido que a 60. Las duraciones son las que daban esos pasos a 60 fps
/// (16,67 ms por frame), así el ritmo se conserva.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pace {
    /// Apertura de un panel/overlay/reveal (~238 ms).
    Open,
    /// Cierre de un panel (~333 ms): más lento que la apertura.
    Close,
    /// Cierre del split de la isla (~185 ms): más RÁPIDO que su apertura, porque es un
    /// indicador transitorio y tiene que irse antes de lo que llegó.
    MorphClose,
    /// Fades cortos de contenido (~104 ms).
    Quick,
}

impl Pace {
    pub fn ms(self) -> f32 {
        match self {
            Pace::Open => 238.0,
            Pace::Close => 333.0,
            Pace::MorphClose => 185.0,
            Pace::Quick => 104.0,
        }
    }
}

/// Factor de un lerp exponencial por frame (`x += (target - x) * k`), corregido para
/// que no dependa del frame rate: a 60 fps (16,67 ms) devuelve exactamente `k`. Es el
/// equivalente temporal del `k` por frame que usaban el scroll y el resaltado.
pub fn lerp_factor(k: f32, dt_ms: f32) -> f32 {
    1.0 - (1.0 - k).powf(dt_ms / 16.667)
}

/// Avanza `cur` hacia `target` lo que corresponde a `dt_ms` con el ritmo dado. Es
/// **lineal en el tiempo** —la curva la aplica el dibujo (`ease_out`, `ws_split_eased`),
/// no el avance—, pero ya no depende del frame rate.
pub fn anim_step(cur: f32, target: f32, dt_ms: f32, pace: Pace) -> f32 {
    let step = dt_ms / pace.ms();
    if target > cur {
        (cur + step).min(target)
    } else {
        (cur - step).max(target)
    }
}

/// Un frame de una animación 0..1: avanza `anim` hacia `target` con el ritmo de apertura
/// o de cierre según la dirección y devuelve `true` si todavía hay que dibujar. Es el
/// reemplazo del `if anim < target { … } else if anim > target { … }` repartido en cada
/// tick.
pub fn anim_towards(anim: &mut f32, target: f32, dt_ms: f32, open: Pace, close: Pace) -> bool {
    if *anim == target {
        return false;
    }
    let pace = if *anim < target { open } else { close };
    *anim = anim_step(*anim, target, dt_ms, pace);
    true
}

pub const OSD_TIMEOUT_MS: u64 = 1400;
/// Largo de la píldora del OSD (a lo largo del dock). El grosor es el del dock: el OSD
/// es un estado de la isla, no un panel con superficie propia.
pub const OSD_PILL_LEN: f32 = 220.0;
/// Cuánto tiempo se queda el HUD del indicador de workspaces.
pub const WS_FLASH_TIMEOUT_MS: u64 = 3000;
// ----- fixed size -----
pub const OSD_NOTIFICATION_BASE_LEN: f32 = 240.0;
pub const NOTIFICATION_MIN_PANEL_H: f32 = 52.0;
pub const NOTIFICATION_TIMEOUT_MS: u64 = 4000;
pub const NOTIFICATION_GROWTH_W: f32 = 60.0;
pub const NOTIFICATION_GROWTH_H: f32 = 24.0;
pub const NOTIFICATION_BODY_MAX_LINES: usize = 8;

/// Ancho de los tres paneles del overlay (launcher/ventanas, portapapeles y
/// selector de fondos). Los tres van centrados con el mismo anclaje
/// (`edge_anchor_margin`), así que con el mismo ancho los bordes no se mueven al
/// ciclar con Shift+←/→: antes medían 809 (dock + 180), 440 y 640 y el panel
/// "saltaba" de tamaño entre pestaña y pestaña.
///
/// Sale del dock a propósito: el valor no cambia porque el usuario agregue un
/// widget. 640 está elegido para que la grilla del launcher dé 6 columnas x 2
/// filas (6*92 + 5*10 = 602 contra los 620 disponibles).
pub const OVERLAY_PANEL_W: f32 = 640.0;

/// Largo de esos mismos paneles cuando el dock es vertical (`Left`/`Right`), donde
/// el "largo" es el ALTO. Es el mismo para los cuatro por el mismo motivo que
/// `OVERLAY_PANEL_W` en el horizontal: con el alto común el borde de arriba y el de
/// abajo no se mueven al ciclar con Shift+←/→. Medido antes del cambio: apps 556,
/// portapapeles 434, notifs 434 y fondos 640, así que el panel saltaba 206 px al
/// pasar de una pestaña a otra.
///
/// El ancho (cross) NO se toca: cada panel pide el suyo segun el contenido (los
/// fondos 196 por la miniatura de 150, el resto 356 por las dos columnas de tarjeta
/// y los titulos largos).
///
/// 640 es el que ya usaba el selector de fondos, así que su filmstrip no pierde
/// miniaturas; las listas (portapapeles, notifs) y la grilla del launcher sacan de
/// acá cuantas filas entran (ver `clip_visible_rows`, `notif_visible_rows` y
/// `app_search_strip_h`).
pub const OVERLAY_PANEL_H: f32 = 640.0;

/// Cross de esos mismos tres paneles cuando el dock es vertical (`Left`/`Right`):
/// una columna al lado del dock. Es el MISMO para los tres, por la misma razón
/// que `OVERLAY_PANEL_W` en el horizontal: el borde no se mueve al ciclar con
/// Shift+←/→. 170 es el que ya usaba el selector de fondos (miniatura de 150 más
/// los dos `MENU_PADDING`), que es el panel que el usuario puso de referencia.
/// Es el ancho del CONTENIDO: la banda de pestañas le suma `OVERLAY_TABS_W`.
pub const OVERLAY_PANEL_VERTICAL_W: f32 = 170.0;

/// Cross ANCHO del panel vertical: el launcher (dos columnas de tarjeta de 150 más
/// el gap y los insets) y el portapapeles, que tiene títulos largos y en 170 se
/// elidía a ~16 caracteres. Da 330 por construcción, así que el launcher no tiene
/// su propio número (lo fija el test `el_ancho_del_launcher_es_el_compartido`).
pub const OVERLAY_PANEL_VERTICAL_WIDE: f32 = 330.0;

/// Cross del contenido del panel vertical, con el piso del grosor del dock: con
/// una barra muy gruesa el panel quedaría más angosto que ella.
pub fn overlay_vertical_cross(dock_thickness: f32) -> f32 {
    dock_thickness.max(OVERLAY_PANEL_VERTICAL_W)
}

/// Barra de pestañas compartida por los modos del overlay. El orden es el de
/// `OVERLAY_ORDER` (Shift+←/→ cicla) y cada panel la dibuja: es la única señal de
/// en qué mini-app estás.
pub const OVERLAY_TABS: [&str; 6] = [
    "Apps",
    "Clipboard",
    "Keybinds",
    "Notifs",
    "Wallpapers",
    "Windows",
];
/// Largo de la banda cuando es una FILA (panel ancho, dock arriba/abajo).
pub const OVERLAY_TABS_H: f32 = 26.0;
/// Ancho de la banda cuando es una COLUMNA (panel vertical): va pegada al lado del
/// dock, con las etiquetas rotadas 90° como los widgets de la barra vertical.
/// Antes iba apilada arriba y le comía 104 de alto a un panel que ya es una
/// columna (y el panel quedaba más alto que la pantalla útil).
pub const OVERLAY_TABS_W: f32 = 26.0;

/// Caja del CONTENIDO de un panel del overlay (`x`, `y` son su origen dentro del
/// panel; `w`, `h` su tamaño). La banda de pestañas vive fuera de la caja: es una
/// fila de 26 arriba en el panel ancho y una columna de 26 al costado en el
/// vertical. Todo el reparto de los tres paneles (buscador, filas, tiles,
/// filmstrip) y sus hit tests salen de acá: la banda se cuenta UNA vez y el
/// dibujo y el click no se pueden desincronizar (trampa 10).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelFrame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Frame a partir del contenido que pide el panel. `band_left` es de qué lado cae
/// la columna de pestañas en el panel vertical (el lado del dock: `Left`).
pub fn frame_for(content_w: f32, content_h: f32, is_vertical: bool, band_left: bool) -> PanelFrame {
    if is_vertical {
        let x = if band_left { OVERLAY_TABS_W } else { 0.0 };
        PanelFrame {
            x,
            y: 0.0,
            w: content_w,
            h: content_h,
        }
    } else {
        PanelFrame {
            x: 0.0,
            y: OVERLAY_TABS_H,
            w: content_w,
            h: content_h,
        }
    }
}

/// Tamaño del panel que hay que pedirle a la layer: el frame más la banda. Es el
/// inverso de `frame_for` (lo fija el test `el_frame_y_el_panel_son_inversos`).
pub fn panel_size(frame: PanelFrame, is_vertical: bool) -> (f32, f32) {
    if is_vertical {
        (frame.w + OVERLAY_TABS_W, frame.h)
    } else {
        (frame.w, frame.h + OVERLAY_TABS_H)
    }
}

/// Rect de la banda dentro del panel (x, y, w, h): la fila de arriba o la columna
/// del lado del dock. La usan el dibujo de las pestañas y nada más.
pub fn band_rect(
    panel_w: f32,
    panel_h: f32,
    is_vertical: bool,
    band_left: bool,
) -> (f32, f32, f32, f32) {
    if is_vertical {
        let x = if band_left {
            0.0
        } else {
            panel_w - OVERLAY_TABS_W
        };
        (x, 0.0, OVERLAY_TABS_W, panel_h)
    } else {
        (0.0, 0.0, panel_w, OVERLAY_TABS_H)
    }
}

/// Radio de todo lo que va DENTRO de un panel del overlay: la caja de búsqueda,
/// la pastilla de la tarjeta de app, la fila del portapapeles, la pestaña de la
/// banda y la miniatura de fondo. Antes eran 6/8/7/11 según el panel (y el 11
/// estaba repetido en tres lugares del filmstrip), así que los paneles no
/// parecían la misma familia. El inset lateral de esos mismos paneles es
/// `MENU_PADDING`.
///
/// El radio de la miniatura se hornea en el pixmap (`open_wallpaper_picker`),
/// así que el anillo del hover tiene que salir de acá también o no coincide con
/// la esquina de la imagen.
pub const OVERLAY_RADIUS: f32 = 8.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OsdKind {
    Volume,
    Brightness,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonKind {
    AddApp,
    OpenAppLauncher,
    OpenWindowSwitcher,
    OpenClipboard,
    OpenKeybinds,
    OpenNotifications,
    OpenWallpapers,
    QuitDock,
    ChangeIcon,
    RemoveApp,
    Back,
    Suspend,
    Logout,
    Reboot,
    Shutdown,
    CreatePalette,
    SavePalette,
    WallpaperDir,
}
impl ButtonKind {
    pub fn label(self) -> &'static str {
        match self {
            ButtonKind::AddApp => "Add App…",
            ButtonKind::OpenAppLauncher => "App Launcher",
            ButtonKind::OpenWindowSwitcher => "Window Switcher",
            ButtonKind::OpenClipboard => "Clipboard",
            ButtonKind::OpenKeybinds => "Keybindings",
            ButtonKind::OpenNotifications => "Notifications",
            ButtonKind::OpenWallpapers => "Wallpapers",
            ButtonKind::QuitDock => "Quit Dock",
            ButtonKind::ChangeIcon => "Change Icon…",
            ButtonKind::RemoveApp => "Remove App from Dock",
            ButtonKind::Back => "‹ Back",
            ButtonKind::Suspend => "Suspend",
            ButtonKind::Logout => "Log Out",
            ButtonKind::Reboot => "Reboot",
            ButtonKind::Shutdown => "Shut Down",
            ButtonKind::CreatePalette => "Create Your Own Palette!",
            ButtonKind::SavePalette => "Save Palette",
            // ----- el label real lo pone el render: muestra la carpeta elegida -----
            ButtonKind::WallpaperDir => "Wallpaper Folder",
        }
    }

    pub fn is_destructive(self) -> bool {
        matches!(
            self,
            ButtonKind::QuitDock
                | ButtonKind::RemoveApp
                | ButtonKind::Reboot
                | ButtonKind::Shutdown
        )
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuScreen {
    AddApp,
    IconMenu(usize),
    IconPicker(usize),
    WallpaperPicker,
    PowerMenu,
    TrayMenu,
    /// Panel de volumen: salida + un stream por app + selector de salida.
    VolumePanel,
    /// Calendario del reloj: se abre con el puntero encima del widget y ←/→
    /// cambian de mes.
    Calendar,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuCategory {
    Layout,
    Appearance,
    Colors,
    Widgets,
    /// Opciones del launcher y del contenido de las pestañas del overlay
    /// (portapapeles, avisos). Ver `LAUNCHER_SETTINGS`.
    Launcher,
    System,
    // ----- palette button only -----
    CustomPalette,
}
pub const MENU_CATEGORIES: [MenuCategory; 6] = [
    MenuCategory::Layout,
    MenuCategory::Appearance,
    MenuCategory::Colors,
    MenuCategory::Widgets,
    MenuCategory::Launcher,
    MenuCategory::System,
];
pub fn category_order_index(c: MenuCategory) -> i32 {
    match c {
        MenuCategory::Layout => 0,
        MenuCategory::Appearance => 1,
        MenuCategory::Colors => 2,
        MenuCategory::Widgets => 3,
        MenuCategory::Launcher => 4,
        MenuCategory::System => 5,
        MenuCategory::CustomPalette => 6,
    }
}
impl MenuCategory {
    pub fn label(self) -> &'static str {
        match self {
            MenuCategory::Layout => "Layout",
            MenuCategory::Appearance => "Appearance",
            MenuCategory::Colors => "Themes",
            MenuCategory::Widgets => "Widgets",
            MenuCategory::Launcher => "Launcher",
            MenuCategory::System => "System",
            MenuCategory::CustomPalette => "Themes",
        }
    }
}
#[derive(Clone, Copy)]
pub enum ControlKind {
    Section(&'static str),
    Note(&'static str),
    IconHeader(usize),
    Slider(SettingId),
    Toggle(SettingId),
    Button(ButtonKind),
    AppEntry(usize),
    SearchBox,
    IconChoice(usize),
    EdgePicker,
    WidgetChips,
    ThemePicker,
    SchemeDropdown,
    DockFontDropdown,
    SystemFontDropdown,
    HexField(usize),
    NameField,
    PaletteModePicker,
    PanelBlendPicker,
    TrayItem(usize),
    TraySeparator,
    AlignPicker,
    // ----- panel de volumen: fila de la salida o de un stream, y dispositivo de
    // salida del selector (los datos vienen en `DrawArgs`, no de los ajustes) -----
    VolumeRow(usize),
    VolumeDevice(usize),
    // ----- calendario: el mes mostrado viaja en el propio control, no en un
    // campo aparte (ver `menu::CalendarMonth`) -----
    Calendar(CalendarMonth),
}
#[derive(Clone, Copy)]
pub struct Control {
    pub kind: ControlKind,
    pub y: f32,
    pub height: f32,
}
pub fn slider_geometry(control_y: f32) -> (f32, f32, f32, f32, f32) {
    let track_row_y = control_y + 14.0;
    let minus_x = MENU_PADDING;
    let plus_x = MENU_WIDTH - MENU_PADDING - STEP_BTN_SIZE;
    let track_x0 = minus_x + STEP_BTN_SIZE + 6.0;
    let track_x1 = plus_x - 6.0;
    (minus_x, track_x0, track_x1, plus_x, track_row_y)
}
pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() == 3 {
        let mut chars = s.chars();
        let r = chars.next()?;
        let g = chars.next()?;
        let b = chars.next()?;
        let r = u8::from_str_radix(&r.to_string().repeat(2), 16).ok()?;
        let g = u8::from_str_radix(&g.to_string().repeat(2), 16).ok()?;
        let b = u8::from_str_radix(&b.to_string().repeat(2), 16).ok()?;
        return Some((r, g, b));
    }
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}
pub fn slider_value_from_x(id: SettingId, x: f32, control_y: f32) -> f32 {
    let (_, track_x0, track_x1, _, _) = slider_geometry(control_y);
    let (min, max, _) = id.range();
    let t = ((x - track_x0) / (track_x1 - track_x0).max(1.0)).clamp(0.0, 1.0);
    min + t * (max - min)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitTarget {
    Toggle(SettingId),
    SliderTrack(SettingId),
    SliderMinus(SettingId),
    SliderPlus(SettingId),
    Button(ButtonKind),
    AppEntry(usize),
    SearchBox,
    IconChoice(usize),
    Tab(MenuCategory),
    Edge(DockEdge),
    WidgetChip(WidgetKind),
    ThemeOption(Option<usize>),
    SchemeOption(usize),
    SchemeDropdownToggle,
    DockFontDropdownToggle,
    SystemFontDropdownToggle,
    FontOption(usize),
    HexFieldFocus(usize),
    NameFieldFocus,
    PaletteMode(bool),
    PanelBlend(u8),
    DeleteCustomPalette(usize),
    TrayItem(usize),
    Align(crate::config::DockAlign),
    VolumeMute(usize),
    VolumeTrack(usize),
    VolumeDevice(usize),
}
pub const DOCK_MENU_LEFT_COL_W: f32 = 120.0;
pub const DOCK_MENU_DIVIDER_W: f32 = 1.0;
pub const DOCK_MENU_TAB_H: f32 = 32.0;
pub const DOCK_MENU_RIGHT_COL_W: f32 = 400.0;
pub fn dock_menu_tab_hit_test(x: f32, y: f32) -> Option<HitTarget> {
    if x >= DOCK_MENU_LEFT_COL_W {
        return None;
    }
    let index = ((y - MENU_PADDING) / DOCK_MENU_TAB_H) as usize;
    MENU_CATEGORIES.get(index).map(|&c| HitTarget::Tab(c))
}
pub fn hit_test(
    controls: &[Control],
    settings: &DockSettings,
    panel_width: f32,
    x: f32,
    y: f32,
) -> Option<HitTarget> {
    for c in controls {
        if y < c.y || y > c.y + c.height {
            continue;
        }
        return match c.kind {
            ControlKind::Toggle(id) => Some(HitTarget::Toggle(id)),
            ControlKind::Slider(id) => {
                let (minus_x, _, _, plus_x, _) = slider_geometry(c.y);
                if x >= minus_x && x <= minus_x + STEP_BTN_SIZE {
                    Some(HitTarget::SliderMinus(id))
                } else if x >= plus_x && x <= plus_x + STEP_BTN_SIZE {
                    Some(HitTarget::SliderPlus(id))
                } else {
                    Some(HitTarget::SliderTrack(id))
                }
            }
            ControlKind::Button(b) => Some(HitTarget::Button(b)),
            ControlKind::AppEntry(i) => Some(HitTarget::AppEntry(i)),
            ControlKind::IconChoice(i) => Some(HitTarget::IconChoice(i)),
            ControlKind::SearchBox => Some(HitTarget::SearchBox),
            ControlKind::EdgePicker => {
                edge_picker_hit_test(c.y, panel_width, x, y).map(HitTarget::Edge)
            }
            ControlKind::WidgetChips => {
                widget_chip_hit_test(settings, c.y, panel_width, x, y).map(HitTarget::WidgetChip)
            }
            ControlKind::ThemePicker => theme_picker_hit_test(settings, c.y, panel_width, x, y)
                .map(|hit| match hit {
                    ThemeGridHit::Select(choice) => HitTarget::ThemeOption(choice),
                    ThemeGridHit::Delete(i) => HitTarget::DeleteCustomPalette(i),
                }),
            ControlKind::SchemeDropdown => Some(HitTarget::SchemeDropdownToggle),
            ControlKind::DockFontDropdown => Some(HitTarget::DockFontDropdownToggle),
            ControlKind::SystemFontDropdown => Some(HitTarget::SystemFontDropdownToggle),
            ControlKind::HexField(i) => Some(HitTarget::HexFieldFocus(i)),
            ControlKind::NameField => Some(HitTarget::NameFieldFocus),
            ControlKind::PaletteModePicker => {
                palette_mode_hit_test(c.y, panel_width, x, y).map(HitTarget::PaletteMode)
            }
            ControlKind::PanelBlendPicker => {
                panel_blend_hit_test(c.y, panel_width, x, y).map(HitTarget::PanelBlend)
            }
            ControlKind::TrayItem(i) => Some(HitTarget::TrayItem(i)),
            ControlKind::AlignPicker => {
                align_hit_test(c.y, panel_width, x, y).map(HitTarget::Align)
            }
            ControlKind::VolumeRow(_) | ControlKind::VolumeDevice(_) => volume_hit_test(c, x, y),
            ControlKind::Section(_)
            | ControlKind::Note(_)
            | ControlKind::IconHeader(_)
            // el calendario no tiene nada clickeable: se cambia de mes con ←/→
            | ControlKind::Calendar(_)
            | ControlKind::TraySeparator => None,
        };
    }
    None
}
pub fn stepper_speed(hold_frames: u32) -> f32 {
    match hold_frames {
        0..=14 => 1.0,
        15..=44 => 3.0,
        45..=89 => 8.0,
        _ => 18.0,
    }
}

#[cfg(test)]
mod slide_tests {
    use super::*;

    /// El contrato del deslizamiento: arranca corrido al máximo hacia el lado del
    /// viaje, llega a 0 (si no, el panel quedaría desalineado para siempre) y con
    /// `dir == 0` no se mueve nada (abrir una pestaña directo).
    #[test]
    fn el_deslizamiento_arranca_corrido_y_termina_en_cero() {
        assert_eq!(overlay_slide_offset(0.0, 0.0), 0.0);
        assert_eq!(overlay_slide_offset(0.0, 0.5), 0.0);
        assert_eq!(overlay_slide_offset(0.0, 1.0), 0.0);
        assert_eq!(overlay_slide_offset(1.0, 0.0), TAB_SLIDE_DISTANCE);
        assert_eq!(overlay_slide_offset(-1.0, 0.0), -TAB_SLIDE_DISTANCE);
        assert_eq!(overlay_slide_offset(1.0, 1.0), 0.0);
        assert_eq!(overlay_slide_offset(-1.0, 1.0), 0.0);
        // ----- y baja monótono: un valor que volviera a subir se vería como un
        // rebote en medio del cambio de pestaña -----
        let mut prev = overlay_slide_offset(1.0, 0.0);
        for i in 1..=10 {
            let v = overlay_slide_offset(1.0, i as f32 / 10.0);
            assert!(v <= prev, "t={i}: {v} > {prev}");
            prev = v;
        }
    }
}

#[cfg(test)]
mod anim_tests {
    use super::*;

    /// El avance es por TIEMPO, no por frame: la misma duración da el mismo recorrido a
    /// 120 Hz (4 frames de 8,33 ms) que a 60 Hz (uno de 33,33 ms). Era el punto de F: los
    /// `+= 0.07` por frame hacían que a 144 Hz todo fuera 2,4x más rápido.
    #[test]
    fn el_avance_no_depende_del_frame_rate() {
        let mut a = 0.0;
        for _ in 0..4 {
            a = anim_step(a, 1.0, 8.333, Pace::Open);
        }
        let b = anim_step(0.0, 1.0, 33.333, Pace::Open);
        assert!((a - b).abs() < 0.005, "120 Hz: {a} contra 60 Hz: {b}");
        // ----- y a 60 fps el primer paso reproduce el `+= 0.07` de antes -----
        assert!((anim_step(0.0, 1.0, 16.667, Pace::Open) - 0.07).abs() < 0.005);
    }

    /// El paso no se pasa del target (ni rebota) y llega exacto.
    #[test]
    fn el_avance_no_se_pasa_del_target() {
        assert_eq!(anim_step(0.9, 1.0, 1000.0, Pace::Open), 1.0);
        assert_eq!(anim_step(0.1, 0.0, 1000.0, Pace::Open), 0.0);
        assert_eq!(anim_step(1.0, 1.0, 16.667, Pace::Open), 1.0);
    }

    /// `anim_towards` elige el ritmo por dirección y avisa si hay que seguir dibujando.
    #[test]
    fn el_ritmo_sale_de_la_direccion() {
        let mut v = 0.0;
        assert!(anim_towards(
            &mut v,
            1.0,
            16.667,
            Pace::Open,
            Pace::MorphClose
        ));
        let subiendo = v;
        // ----- bajando con MorphClose (185 ms) va MÁS rápido que subir con Open (238) -----
        let mut w = 1.0;
        assert!(anim_towards(
            &mut w,
            0.0,
            16.667,
            Pace::Open,
            Pace::MorphClose
        ));
        assert!(
            (1.0 - w) > subiendo,
            "el cierre va más rápido que la apertura"
        );
        // ----- llegado: false, así el tick se apaga -----
        let mut done = 1.0;
        assert!(!anim_towards(
            &mut done,
            1.0,
            16.667,
            Pace::Open,
            Pace::Close
        ));
    }

    /// El lerp corregido: a 60 fps devuelve el `k` de antes, y dos frames de 120 fps dan
    /// el mismo factor combinado que uno de 60.
    #[test]
    fn el_lerp_no_depende_del_frame_rate() {
        assert!((lerp_factor(0.28, 16.667) - 0.28).abs() < 0.001);
        let k120 = lerp_factor(0.28, 8.333);
        let combinado = 1.0 - (1.0 - k120) * (1.0 - k120);
        assert!((combinado - 0.28).abs() < 0.005, "combinado: {combinado}");
    }
}
