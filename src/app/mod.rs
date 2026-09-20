use crate::config::PinnedApp;
use crate::desktop::{self, DesktopEntry};
use crate::dock;
use crate::dock::Dock;
use crate::icon_browser;
use crate::menu;
use crate::menu_render;
use crate::render;
use crate::wallpaper::{self, WallpaperEntry};
use crate::widgets;
use dockyrs_canvas::IconCache;
use dockyrs_canvas::{TextCache, ThumbnailCache};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    backend::ObjectId,
    protocol::{wl_callback, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
    zwlr_data_control_source_v1::ZwlrDataControlSourceV1,
};

mod app_search;
mod calendar;
mod click_catcher;
mod clipboard_ui;
mod dock_menu;
mod dock_menu_input;
mod dock_popup;
mod draw;
mod fonts;
mod notification;
mod notifications_ui;
mod osd;
mod pointer;
mod popup_menu;
mod popup_menu_input;
mod screenshot;
mod volume_panel;
mod wallpaper_picker;
mod ws_flash;
use fonts::{apply_kitty_font, apply_system_gtk_font, apply_system_qt_font};
mod handlers;
use self::click_catcher::ClickCatcher;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const MENU_GAP: i32 = 0;
const WALLPAPER_PANEL_H: f32 = menu::OVERLAY_PANEL_VERTICAL_W;
const EMPTY_CUSTOM_HEX: [String; 5] = [
    String::new(),
    String::new(),
    String::new(),
    String::new(),
    String::new(),
];

// ----- el dock se revela al pasar el mouse por su franja superior:
// su input region se mantiene activa incluso oculto (disparador), porque
// niri sólo re-evalúa el foco del puntero con movimiento real -----

// ----- marca de tiempo relativa (solo depuración del hotcorner) -----
pub(crate) fn hdbg_ms() -> u128 {
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    T0.get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis()
}

pub(crate) fn edge_anchor_margin(
    edge: crate::config::DockEdge,
    align: crate::config::DockAlign,
    pos_y: i32,
    extra: i32,
) -> (Anchor, (i32, i32, i32, i32)) {
    use crate::config::{DockAlign, DockEdge};
    match edge {
        DockEdge::Bottom => {
            let a = match align {
                DockAlign::Middle => Anchor::BOTTOM,
                DockAlign::Left => Anchor::BOTTOM | Anchor::LEFT,
                DockAlign::Right => Anchor::BOTTOM | Anchor::RIGHT,
            };
            (a, (0, 0, pos_y + extra, 0))
        }
        DockEdge::Top => {
            let a = match align {
                DockAlign::Middle => Anchor::TOP,
                DockAlign::Left => Anchor::TOP | Anchor::LEFT,
                DockAlign::Right => Anchor::TOP | Anchor::RIGHT,
            };
            (a, (pos_y + extra, 0, 0, 0))
        }
        DockEdge::Left => {
            let a = match align {
                DockAlign::Middle => Anchor::LEFT,
                DockAlign::Left => Anchor::LEFT | Anchor::TOP,
                DockAlign::Right => Anchor::LEFT | Anchor::BOTTOM,
            };
            (a, (0, 0, 0, pos_y + extra))
        }
        DockEdge::Right => {
            let a = match align {
                DockAlign::Middle => Anchor::RIGHT,
                DockAlign::Left => Anchor::RIGHT | Anchor::TOP,
                DockAlign::Right => Anchor::RIGHT | Anchor::BOTTOM,
            };
            (a, (0, pos_y + extra, 0, 0))
        }
    }
}

pub(crate) struct MenuState {
    layer: LayerSurface,
    pool: SlotPool,
    screen: menu::MenuScreen,
    controls: Vec<menu::Control>,
    content_height: f32,
    app_entries: Vec<DesktopEntry>,
    filtered_app_entries: Vec<DesktopEntry>,
    all_icon_names: Vec<String>,
    filtered_icon_names: Vec<String>,
    search_query: String,
    anim: f32,
    target_anim: f32,
    closing: bool,
    awaiting_frame: bool,
    hovered: Option<menu::HitTarget>,
    dragging_slider: Option<menu::SettingId>,
    held_stepper: Option<(menu::SettingId, f32, u32)>,
    list_scroll: usize,
    list_selected: usize,
}

pub(crate) struct DockPopupMode {
    layer: LayerSurface,
    pool: SlotPool,
    awaiting_frame: bool,
    screen: menu::MenuScreen,
    controls: Vec<menu::Control>,
    content_height: f32,
    hovered: Option<menu::HitTarget>,
    anim: f32,
    target_anim: f32,
    closing: bool,
    tray_items: Vec<crate::tray::TrayMenuItem>,
    tray_service: String,
    tray_menu_path: String,
    tray_stack: Vec<Vec<crate::tray::TrayMenuItem>>,
    /// Id del item cuyo submenú se pidió y todavía no llegó: sin esto, un resultado
    /// que llega tarde (o el de otro item) abriría el menú equivocado (A3).
    pending_submenu: Option<i32>,
    // ----- contenido renderizado, reusado entre frames de la animación -----
    content: Option<tiny_skia::Pixmap>,
    content_dirty: bool,
    center: Option<(f32, f32)>,
    surface_w: f32,
    surface_h: f32,
    box_x: f32,
    box_y: f32,
    // ----- el puntero está dentro del popup: mientras lo esté, salir del dock no
    // lo cierra. Si no, moverse del icono al menú lo cerraría al pasar. -----
    popup_hovered: bool,
    // ----- panel de volumen (`MenuScreen::VolumePanel`): datos que dibuja
    // `menu_render`, más el estado del arrastre de la barra -----
    volume_rows: Vec<crate::widgets::VolumeRow>,
    volume_devices: Vec<crate::widgets::AudioDevice>,
    volume_drag: Option<usize>,
    volume_apply_at: Option<std::time::Instant>,
}

pub(crate) struct DockMenuMode {
    category: menu::MenuCategory,
    controls: Vec<menu::Control>,
    hovered: Option<menu::HitTarget>,
    dragging_slider: Option<menu::SettingId>,
    held_stepper: Option<(menu::SettingId, f32, u32)>,
    dragging_widget: Option<(crate::config::WidgetKind, f32, f32)>,
    anim: f32,
    target_anim: f32,
    closing: bool,
    panel_w: f32,
    panel_h: f32,
    slide_anim: f32,
    slide_dir: f32,
    open_dropdown: menu::OpenDropdown,
    dropdown_anim: f32,
    dropdown_scroll: usize,
    font_query: String,
    dropdown_selected: usize,
    pub(crate) custom_hex: [String; 5],
    custom_light: bool,
    custom_focus: Option<usize>,
    pub(crate) custom_name: String,
    custom_name_focused: bool,
    custom_panel_blend: Option<u8>,
}

pub(crate) struct WallpaperMode {
    wallpapers: Vec<WallpaperEntry>,
    hovered: Option<menu::WallpaperHit>,
    scroll_x: f32,
    scroll_target: f32,
    anim: f32,
    target_anim: f32,
    closing: bool,
    /// Caja del CONTENIDO del panel (ver `AppSearchMode::frame`).
    frame: menu::PanelFrame,
    is_vertical: bool,
    /// Ver `AppSearchMode::slide_dir`.
    slide_dir: f32,
    thumb_w: u32,
    thumb_h: u32,
    thumb_requested: std::collections::HashSet<std::path::PathBuf>,
    /// Cuántas miniaturas se pidieron y todavía no volvieron. El hilo que las
    /// decodifica avisa por el canal, y un canal NO despierta el loop de
    /// Wayland: si el loop se apaga mientras hay pedidos en vuelo, las
    /// miniaturas quedan en la cola hasta que cualquier evento (un scroll, un
    /// click) fuerce un redraw. Por eso `tick_wallpaper_frame` sigue dando
    /// frames mientras esto no llegue a 0.
    thumbs_pending: usize,
    thumb_request_tx: std::sync::mpsc::Sender<(std::path::PathBuf, u32, u32)>,
    thumb_result_rx:
        std::sync::mpsc::Receiver<(std::path::PathBuf, u32, u32, Option<tiny_skia::Pixmap>)>,
}

pub(crate) struct AppSearchMode {
    /// Qué lista muestra: las apps instaladas o las ventanas abiertas.
    list: SearchList,
    query: String,
    all_entries: Vec<DesktopEntry>,
    filtered: Vec<DesktopEntry>,
    controls: Vec<menu::Control>,
    selected: usize,
    scroll_x: f32,
    scroll_target: f32,
    /// Posición de la pastilla del resaltado dentro del strip. Con la grilla de
    /// dos filas hace falta la y: si no, la pastilla se dibuja en la fila de
    /// arriba aunque la tarjeta elegida esté abajo.
    highlight_x: f32,
    highlight_target: f32,
    highlight_y: f32,
    highlight_target_y: f32,
    content_anim: f32,
    anim: f32,
    target_anim: f32,
    closing: bool,
    /// Caja del CONTENIDO del panel: el panel menos la banda de pestañas, que en
    /// el vertical es una columna al costado del dock y en el ancho una fila
    /// arriba (ver `menu::PanelFrame`). El tamaño del panel sale de acá con
    /// `menu::panel_size`, así que no hay dos números que se puedan despegar.
    frame: menu::PanelFrame,
    is_vertical: bool,
    /// Dirección del deslizamiento del contenido al cambiar de pestaña con
    /// Shift+←/→ (+1 = viene de la derecha). 0 = sin deslizamiento (abrir la
    /// pestaña directo, por IPC). El progreso es el `anim` de siempre. Ver
    /// `menu::overlay_slide_offset`.
    slide_dir: f32,
}

/// Qué lista muestra el panel de búsqueda. Es lo que permite que el cambiador de
/// ventanas reuse el mismo panel, el mismo render y el mismo matching que el
/// launcher, en vez de ser otro modo aparte.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SearchList {
    Apps,
    Windows,
}

/// Los modos que se apilan sobre el dock. Shift+←/→ cicla entre ellos: es el
/// equivalente a los modos de rofi, reusando lo que ya existe (el launcher, el
/// portapapeles y el selector de fondos ya eran modos propios).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum OverlayMode {
    Apps,
    Clipboard,
    Notifs,
    Wallpaper,
    Windows,
}

pub(crate) const OVERLAY_ORDER: [OverlayMode; 5] = [
    OverlayMode::Apps,
    OverlayMode::Clipboard,
    OverlayMode::Notifs,
    OverlayMode::Wallpaper,
    OverlayMode::Windows,
];

impl OverlayMode {
    /// Posición en la barra de pestañas: es la misma que en `OVERLAY_ORDER`, que es
    /// lo que recorre Shift+←/→. El índice lo consume el dibujo de la banda.
    pub(crate) fn tab_index(self) -> usize {
        OVERLAY_ORDER.iter().position(|m| *m == self).unwrap_or(0)
    }
}

/// Dirección del ciclo de pestañas del overlay para una flecha con Shift: +1 =
/// siguiente (derecha/abajo), -1 = anterior (izquierda/arriba). En el panel vertical
/// la banda de pestañas es una columna y las flechas que la corren son ↑/↓; en el
/// panel ancho, ←/→.
pub(crate) fn overlay_cycle_dir(keysym: Keysym) -> i32 {
    if matches!(keysym, Keysym::Right | Keysym::Down) {
        1
    } else {
        -1
    }
}

/// ¿Esta flecha, con Shift, cicla las pestañas del overlay? La banda marca el eje:
/// es una fila arriba en el panel ancho (←/→) y una COLUMNA al costado del dock en el
/// vertical, donde además corren ↑/↓. Las cuatro siguen andando, así que el gesto de
/// ←/→ de siempre no se toca.
pub(crate) fn flecha_de_la_banda(keysym: Keysym, is_vertical: bool) -> bool {
    matches!(keysym, Keysym::Left | Keysym::Right)
        || (is_vertical && matches!(keysym, Keysym::Up | Keysym::Down))
}

impl App {
    /// Qué modo del overlay está abierto, si hay alguno.
    pub(crate) fn current_overlay(&self) -> Option<OverlayMode> {
        if let Some(m) = self.app_search_mode.as_ref() {
            return Some(match m.list {
                SearchList::Apps => OverlayMode::Apps,
                SearchList::Windows => OverlayMode::Windows,
            });
        }
        if self.clipboard_mode.is_some() {
            return Some(OverlayMode::Clipboard);
        }
        if self.notifications_mode.is_some() {
            return Some(OverlayMode::Notifs);
        }
        if self.wallpaper_mode.is_some() {
            return Some(OverlayMode::Wallpaper);
        }
        None
    }

    /// Pasa al modo siguiente (`dir = 1`) o anterior (`dir = -1`). Cierra el actual
    /// por su propio camino —así restaura el tamaño y libera caché— y abre el que
    /// sigue. Devuelve false si no había ningún modo abierto: en ese caso
    /// Shift+flecha no tiene que hacer nada.
    pub(crate) fn cycle_overlay(&mut self, dir: i32, qh: &QueueHandle<Self>) -> bool {
        let Some(current) = self.current_overlay() else {
            return false;
        };
        self.held_key = None;
        let index = OVERLAY_ORDER
            .iter()
            .position(|m| *m == current)
            .unwrap_or(0) as i32;
        let next = OVERLAY_ORDER[(index + dir).rem_euclid(OVERLAY_ORDER.len() as i32) as usize];
        log::debug!("overlay: {current:?} -> {next:?}");
        match current {
            OverlayMode::Apps | OverlayMode::Windows => self.close_app_search_mode(qh),
            OverlayMode::Clipboard => self.close_clipboard_mode(qh),
            OverlayMode::Notifs => self.close_notifications_mode(qh),
            OverlayMode::Wallpaper => self.close_wallpaper_mode(qh),
        }
        match next {
            OverlayMode::Apps => self.open_app_search(qh),
            OverlayMode::Clipboard => self.open_clipboard(qh),
            OverlayMode::Notifs => self.open_notifications(qh),
            OverlayMode::Wallpaper => self.open_wallpaper_picker(qh),
            OverlayMode::Windows => self.open_windows_mode(qh),
        }
        // ----- la dirección del deslizamiento la sabe recién acá: los `open_*`
        // abren sin dirección (0) y esto la anota en el modo que acaba de abrir. -----
        self.start_overlay_slide(dir as f32);
        true
    }

    /// Con qué arranca el fundido del strip de tarjetas: 0 = se funde (lo anima
    /// `tick_app_search_frame`), 1 = ya está. Con las transiciones apagadas tiene que
    /// arrancar terminado **también acá**: el primer draw es anterior al tick, así que
    /// si no el strip se dibujaría invisible un frame.
    fn initial_content_anim(&self) -> f32 {
        if self.dock.config.settings.smooth_transitions {
            0.0
        } else {
            1.0
        }
    }

    /// Anota la dirección del deslizamiento en el modo abierto y reinicia su
    /// `anim`: es el mismo progreso de la apertura, así que los ~15 frames que
    /// antes se gastaban redibujando el mismo cuadro opaco pasan a correr el
    /// contenido. Abrir una pestaña por IPC deja la dirección en 0 (sin
    /// deslizamiento).
    fn start_overlay_slide(&mut self, dir: f32) {
        // ----- con las transiciones apagadas no hay deslizamiento (y no hace falta
        // reiniciar el `anim`: el panel ya arrancó terminado) -----
        if dir == 0.0 || !self.dock.config.settings.smooth_transitions {
            return;
        }
        if let Some(m) = self.app_search_mode.as_mut() {
            m.slide_dir = dir;
            m.anim = 0.0;
        } else if let Some(m) = self.clipboard_mode.as_mut() {
            m.slide_dir = dir;
            m.anim = 0.0;
        } else if let Some(m) = self.wallpaper_mode.as_mut() {
            m.slide_dir = dir;
            m.anim = 0.0;
        }
    }

    /// Con el panel opaco (`transparency = 1.0`, el default) `anim_opacity`
    /// devuelve 1.0 siempre, así que la animación de apertura dibujaría el MISMO
    /// cuadro ~15 veces (110 ms de CPU medidos por cambio de pestaña, ~7 ms por
    /// frame). Si no hay nada que fundir, arranca terminada.
    /// `start_overlay_slide` la reinicia cuando sí hay algo que animar.
    ///
    /// Y con `smooth_transitions` apagado arranca terminada siempre: no hay fundido
    /// ni deslizamiento, así que el panel se dibuja **una** vez.
    fn initial_panel_anim(&self) -> f32 {
        if !self.dock.config.settings.smooth_transitions {
            return 1.0;
        }
        if self.dock.config.settings.transparency >= 0.999 {
            1.0
        } else {
            0.0
        }
    }
}

pub(crate) struct OsdMode {
    kind: menu::OsdKind,
    level: u8,
    muted: bool,
    anim: f32,
    target_anim: f32,
    closing: bool,
    panel_w: f32,
    panel_h: f32,
}

/// Panel de notificaciones: el historial y el scroll. Sin selección ni animación —
/// es el panel más simple del overlay.
pub(crate) struct NotificationsMode {
    scroll: f32,
    hovered: Option<usize>,
    frame: menu::PanelFrame,
    is_vertical: bool,
}

/// HUD que aparece al cambiar de workspace: sólo el indicador de workspaces.
pub(crate) struct WsFlashMode {
    anim: f32,
    target_anim: f32,
    closing: bool,
    panel_w: f32,
    panel_h: f32,
}

pub(crate) struct NotificationMode {
    title: String,
    body: String,
    anim: f32,
    target_anim: f32,
    closing: bool,
    panel_w: f32,
    panel_h: f32,
}

pub struct App {
    pub registry_state: RegistryState,
    pub output_state: OutputState,
    pub seat_state: SeatState,
    pub shm: Shm,
    pub pool: SlotPool,
    pub compositor: CompositorState,
    pub layer_shell: LayerShell,
    pub layer: LayerSurface,
    pub dock_visible: bool,
    /// Animación de aparición estilo isla: 0 = colapsado (invisible), 1 = el dock
    /// entero. El destino lo fija `set_dock_visible`; el paso por frame lo da
    /// `tick_reveal_frame` desde el callback de frame de la superficie del dock.
    /// Split de la isla (0 = cerrada, 1 = abierta en dos con el indicador de
    /// workspaces en el medio). El destino lo fija `show_ws_flash` (al cambiar de
    /// workspace) y el paso por frame `tick_island_split_frame`.
    pub island_ws_split: f32,
    pub island_ws_target: f32,
    pub reveal_anim: f32,
    pub reveal_target: f32,
    /// El Overview de niri está abierto. Mientras dure, el dock se queda visible
    /// (`dock_stays_visible`). Lo mantiene el event-stream de niri.
    pub overview_open: bool,
    pub autohide_armed: bool,
    pub applied_geom: Option<(Anchor, (i32, i32, i32, i32))>,
    /// Última input region aplicada del dock: `None` afuera = todavía no se aplicó,
    /// `Some(None)` = superficie entera, `Some(Some(rect))` = sólo el blob de la isla.
    pub applied_input: Option<Option<(i32, i32, i32, i32)>>,
    pub applied_size: Option<(u32, u32)>,
    pub last_ptr_event: Option<std::time::Instant>,
    /// Instante en que el puntero salió de la franja. Sirve para ocultar el dock
    /// al salir sin esperar el delay largo, pero sin ocultarlo por un `Leave`
    /// espurio: lo limpia cualquier Enter/Motion, así que el plazo (LEAVE_HIDE_MS)
    /// sólo vence si el puntero se fue de verdad.
    pub ptr_left_at: Option<std::time::Instant>,
    /// Instante en que el puntero llegó al widget del reloj y se quedó: pasada la
    /// ventana de `CALENDAR_HOVER_MS` el calendario se abre. Lo limpia cualquier
    /// Enter/Motion que caiga fuera del reloj.
    pub calendar_hover_at: Option<std::time::Instant>,
    pub autohide_hide_tx: std::sync::mpsc::Sender<u64>,
    /// Peticiones de menú del tray: las resuelve el hilo de `tray::spawn_menu_worker`
    /// porque `GetLayout` es bloqueante (A3 de AUDIT.md). El resultado vuelve por el
    /// canal de IPC como `IpcMessage::TrayMenuReady`.
    pub tray_menu_tx: std::sync::mpsc::Sender<crate::tray::MenuRequest>,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    pub dock: Dock,
    pub icon_cache: IconCache,
    pub text_cache: TextCache,
    pub frame_pixmap: Option<tiny_skia::Pixmap>,
    pub thumbnail_cache: ThumbnailCache,
    /// Superficie transparente a pantalla completa con la input region recortada
    /// alrededor del dock, mientras hay un panel abierto. Ver `click_catcher.rs`.
    pub click_catcher: Option<ClickCatcher>,
    pub available_fonts: std::rc::Rc<Vec<String>>,
    pub output_scale: i32,
    pub pinned_output: Option<wl_output::WlOutput>,
    pub awaiting_frame: bool,
    pub exit: bool,
    pub first_configure: bool,
    pub pointer_down: bool,
    pub press_pos: Option<(f64, f64)>,
    pub press_icon_index: Option<usize>,
    pub menu: Option<MenuState>,
    pub popup_mode: Option<DockPopupMode>,
    pub wallpaper_mode: Option<WallpaperMode>,
    pub dock_menu_mode: Option<DockMenuMode>,
    pub app_search_mode: Option<AppSearchMode>,
    pub osd_mode: Option<OsdMode>,
    pub osd_reset_tx: std::sync::mpsc::Sender<()>,
    pub ws_flash_mode: Option<WsFlashMode>,
    pub ws_reset_tx: std::sync::mpsc::Sender<()>,
    pub notification_mode: Option<NotificationMode>,
    /// Superficie PROPIA del toast de notificaciones (arriba a la derecha). Se crea al
    /// primer aviso y se suelta al cerrarse, como el popup: no tiene por qué vivir en
    /// la superficie del dock, que es una franja de 26 px pegada al borde izquierdo.
    pub toast_layer: Option<LayerSurface>,
    pub notification_reset_tx: std::sync::mpsc::Sender<u64>,
    /// El watcher de media (`playerctl --follow`) sólo corre con un widget Media
    /// colocado. Ver `App::publish_watcher_wants`.
    pub media_wanted: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub widgets: crate::widgets::WidgetSnapshot,
    pub tray: crate::tray::TrayState,
    pub last_tray_count: usize,
    pub last_hyprctl_send: Option<std::time::Instant>,
    pub last_empty_click: Option<(std::time::Instant, f64, f64)>,
    /// Si en el frame anterior había un modo que usa la superficie compartida.
    /// Sirve para detectar el cierre del último modo y refrescar el estado del
    /// puntero: el modo se quedó con los eventos, así que `pointer_pos` queda viejo
    /// y con eso en `Some` el dock no se oculta nunca más. Se actualiza en `draw_ex`.
    pub mode_was_open: bool,
    /// Interactividad de teclado aplicada a la superficie (0=none, 1=on-demand,
    /// 2=exclusive). Cachearla evita re-setearla en cada frame.
    pub keyboard_state: u8,
    pub marquee: render::MarqueeState,
    pub marquee_tick_tx: std::sync::mpsc::Sender<u64>,
    pub marquee_rate: u64,
    pub modifiers: Modifiers,
    pub held_key: Option<(Keysym, u32, f32)>,
    pub seat: Option<wl_seat::WlSeat>,
    pub conn: Connection,
    pub qh: QueueHandle<App>,
    pub screenshot: Option<crate::screenshot::ScreenshotState>,
    pub clipboard_manager: Option<ZwlrDataControlManagerV1>,
    pub clipboard_device: Option<ZwlrDataControlDeviceV1>,
    pub clipboard_offer: Option<ZwlrDataControlOfferV1>,
    pub clipboard_offers: std::collections::HashMap<ObjectId, Vec<String>>,
    pub clipboard_source: Option<ZwlrDataControlSourceV1>,
    pub clipboard_copy_bytes: std::sync::Arc<Vec<u8>>,
    pub clipboard_history: crate::clipboard::ClipboardHistory,
    pub clipboard_ready_at: std::time::Instant,
    pub clipboard_mode: Option<ClipboardMode>,
    /// Historial de avisos (el más nuevo primero, tope `NOTIF_HISTORY_CAP`).
    pub notifications: Vec<menu::NotifyEntry>,
    pub notifications_mode: Option<NotificationsMode>,
    pub clip_tx: std::sync::mpsc::Sender<(String, Vec<u8>)>,
    pub paste_tx: std::sync::mpsc::Sender<(crate::clipboard::PasteTarget, String)>,
}

pub(crate) struct ClipboardMode {
    query: String,
    filtered: Vec<usize>,
    selected: usize,
    scroll_y: f32,
    scroll_target: f32,
    hovered: Option<usize>,
    anim: f32,
    target_anim: f32,
    closing: bool,
    /// Caja del CONTENIDO del panel (ver `AppSearchMode::frame`).
    frame: menu::PanelFrame,
    is_vertical: bool,
    /// Ver `AppSearchMode::slide_dir`.
    slide_dir: f32,
    previews: std::collections::HashMap<usize, crate::clipboard::ScaledPreview>,
}

fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("{r:02x}{g:02x}{b:02x}")
}

// ----- skip blur -----
pub(super) fn anim_opacity(transparency: f32, eased: f32) -> f32 {
    if transparency >= 0.999 { 1.0 } else { eased }
}

fn bgra_from_rgba(src: &[u8], dst: &mut [u8]) {
    for (s, d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
        d[0] = s[2];
        d[1] = s[1];
        d[2] = s[0];
        d[3] = s[3];
    }
}

fn launch_app(exec: &str) {
    let exec = exec.to_string();
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{} &", exec))
        .spawn();
}

fn repo_dir() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent()?.parent()?.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

pub(crate) fn trim_heap() {
    unsafe { libc::malloc_trim(0) };
}

#[cfg(test)]
mod overlay_tabs_tests {
    use super::{OVERLAY_ORDER, flecha_de_la_banda, overlay_cycle_dir};
    use crate::menu::OVERLAY_TABS;
    use smithay_client_toolkit::seat::keyboard::Keysym;

    /// La banda dibuja la pestaña `tab_index()` con el nombre de
    /// `OVERLAY_TABS[tab_index()]`: si se reordena `OVERLAY_ORDER` sin tocar las
    /// etiquetas, queda mintiendo (pestaña "Clipboard" con el launcher abierto).
    #[test]
    fn la_barra_sigue_el_orden_de_los_modos() {
        assert_eq!(OVERLAY_ORDER.map(|m| m.tab_index()), [0, 1, 2, 3, 4]);
        assert_eq!(
            OVERLAY_TABS,
            ["Apps", "Clipboard", "Notifs", "Wallpapers", "Windows"]
        );
    }

    /// En el panel vertical la banda es una columna, así que Shift+↓ tiene que ir a
    /// la pestaña SIGUIENTE (y Shift+↑ a la anterior): con el signo al revés las
    /// flechas verticales irían para atrás.
    #[test]
    fn la_flecha_vertical_da_la_direccion_del_ciclo() {
        assert_eq!(overlay_cycle_dir(Keysym::Down), 1);
        assert_eq!(overlay_cycle_dir(Keysym::Right), 1);
        assert_eq!(overlay_cycle_dir(Keysym::Up), -1);
        assert_eq!(overlay_cycle_dir(Keysym::Left), -1);
    }

    /// Qué flechas ciclan las pestañas según la orientación de la banda: en el panel
    /// ancho (fila) sólo ←/→, en el vertical (columna) también ↑/↓, que es lo que
    /// pidió el usuario. Si alguien saca la rama vertical, este test lo avisa.
    #[test]
    fn las_flechas_de_la_banda_siguen_la_orientacion() {
        assert!(flecha_de_la_banda(Keysym::Left, false));
        assert!(flecha_de_la_banda(Keysym::Right, false));
        assert!(!flecha_de_la_banda(Keysym::Up, false));
        assert!(!flecha_de_la_banda(Keysym::Down, false));
        for k in [Keysym::Left, Keysym::Right, Keysym::Up, Keysym::Down] {
            assert!(flecha_de_la_banda(k, true), "{k:?} en vertical");
        }
    }
}
