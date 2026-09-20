use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedApp {
    pub name: String,
    pub icon: String,
    pub exec: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DockEdge {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DockAlign {
    Left,
    #[default]
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WidgetSlot {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WidgetKind {
    Clock,
    Battery,
    Media,
    PowerMenu,
    Bluetooth,
    Tray,
    Workspaces,
    Cpu,
    Ram,
    Network,
    Volume,
    /// Micrófono por defecto (`@DEFAULT_AUDIO_SOURCE@`): icono + MUTE/ON, y el click
    /// togglea el mute.
    Mic,
    /// Grabación de pantalla en curso (`record-toggle.sh` deja la ruta en
    /// `~/.cache/dockyrs-recording-path`): punto rojo + tiempo. En la isla aparece
    /// sola, como actividad viva, aunque no esté colocada en la barra.
    Recording,
    KbdLayout,
    /// Widget con script, direccionado por su posición en
    /// `DockSettings.custom_widgets`. El payload es un índice, no una
    /// identidad: todos los payloads usan la misma entrada genérica de la
    /// tabla y dibujan su salida en caché.
    Custom(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetPlacement {
    pub kind: WidgetKind,
    pub slot: WidgetSlot,
}

fn default_widgets() -> Vec<WidgetPlacement> {
    use WidgetSlot::{Left, Middle, Right};
    // ----- apagado al extremo izquierdo; bateria a su lado -----
    vec![
        WidgetPlacement {
            kind: WidgetKind::PowerMenu,
            slot: Left,
        },
        WidgetPlacement {
            kind: WidgetKind::Battery,
            slot: Left,
        },
        WidgetPlacement {
            kind: WidgetKind::KbdLayout,
            slot: Left,
        },
        WidgetPlacement {
            kind: WidgetKind::Network,
            slot: Left,
        },
        WidgetPlacement {
            kind: WidgetKind::Bluetooth,
            slot: Left,
        },
        WidgetPlacement {
            kind: WidgetKind::Volume,
            slot: Left,
        },
        WidgetPlacement {
            kind: WidgetKind::Workspaces,
            slot: Middle,
        },
        WidgetPlacement {
            kind: WidgetKind::Tray,
            slot: Right,
        },
        WidgetPlacement {
            kind: WidgetKind::Clock,
            slot: Right,
        },
    ]
}

fn default_matugen_scheme() -> String {
    "scheme-neutral".to_string()
}

/// Rango de los multiplicadores de tamaño visual (letra e icono), generales y por
/// widget. Vive acá y no en el panel porque lo usan los dos: `SettingId::range`
/// para el slider y los accessores para no confiar en un JSON editado a mano.
pub const SIZE_SCALE_MIN: f32 = 0.5;
pub const SIZE_SCALE_MAX: f32 = 2.0;

fn default_font_scale() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
}

fn default_hide_delay() -> u64 {
    1200
}

fn default_custom_interval_ms() -> u64 {
    30_000
}

fn default_custom_timeout_ms() -> u64 {
    500
}

fn default_custom_max_chars() -> usize {
    64
}

/// Opciones por widget del tab "Widgets". Cada campo es `None` mientras el
/// widget use el valor general de la barra: el JSON guarda sólo lo que el
/// usuario tocó, no una copia de los defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetOptions {
    /// Multiplicador de letra de ESTE widget, encima del `font_scale` general.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_scale: Option<f32>,
    /// Multiplicador del SÍMBOLO de este widget (hoy sólo la bocina del botón de
    /// volumen), encima de la escala general. No es lo mismo que el tamaño del
    /// widget: el icono crece dentro de su pastilla y la pastilla lo acompaña.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_scale: Option<f32>,
    /// Reloj en 24 h. Ausente o `false` = 12 h con AM/PM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_24h: Option<bool>,
}

impl WidgetOptions {
    /// Sin ninguna opción puesta: la entrada se puede borrar del JSON.
    fn is_empty(&self) -> bool {
        self.font_scale.is_none() && self.icon_scale.is_none() && self.clock_24h.is_none()
    }
}

/// Un par widget + opciones. Es un `Vec` y no un mapa porque `WidgetKind` tiene
/// payload (`Custom(u16)`) y un mapa de serde necesita claves de texto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetOptionEntry {
    pub kind: WidgetKind,
    #[serde(flatten)]
    pub options: WidgetOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomWidgetSource {
    /// `false` (el default) significa que el comando nunca arranca, aunque un
    /// `WidgetKind::Custom` apunte a esta fuente.
    #[serde(default)]
    pub enabled: bool,
    /// Comando de shell cuya primera línea de salida se vuelve el texto del widget.
    #[serde(default)]
    pub command: String,
    /// Ritmo de sondeo. El tick de sistema de dos segundos es la granularidad
    /// más fina disponible, así que un valor menor significa "en cada tick de
    /// sistema".
    #[serde(default = "default_custom_interval_ms")]
    pub interval_ms: u64,
    /// Techo de reloj por ejecución del proceso hijo.
    #[serde(default = "default_custom_timeout_ms")]
    pub timeout_ms: u64,
    /// Presupuesto de texto ya renderizado, contado en caracteres (no en bytes).
    #[serde(default = "default_custom_max_chars")]
    pub max_chars: usize,
}

impl Default for CustomWidgetSource {
    fn default() -> Self {
        Self {
            enabled: false,
            command: String::new(),
            interval_ms: default_custom_interval_ms(),
            timeout_ms: default_custom_timeout_ms(),
            max_chars: default_custom_max_chars(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPalette {
    pub name: String,
    pub accent: (u8, u8, u8),
    pub accent2: (u8, u8, u8),
    pub panel: (u8, u8, u8),
    pub text: (u8, u8, u8),
    pub text_dim: (u8, u8, u8),
    pub is_light: bool,
}

fn default_menu_radius() -> f32 {
    8.40339
}

fn default_menu_border() -> f32 {
    0.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DockSettings {
    pub icon_size: f32,
    pub dock_scale: f32,
    pub widget_scale: f32,
    /// Multiplicador del tamaño de letra de la barra (1.0 = el de fábrica).
    /// Ajuste "Font Size" en Appearance.
    #[serde(default = "default_font_scale")]
    pub font_scale: f32,
    pub media_smooth_scroll: bool,
    pub media_width_scale: f32,
    pub icon_gap: f32,
    pub magnify_scale: f32,
    pub magnify_radius: f32,
    pub width_padding: f32,
    pub corner_radius: f32,
    pub border_width: f32,
    #[serde(default = "default_menu_radius")]
    pub menu_corner_radius: f32,
    #[serde(default = "default_menu_border")]
    pub menu_border_width: f32,
    /// Transiciones suaves de los paneles: el deslizamiento al cambiar de pestaña
    /// (overlay y panel de ajustes) y el fundido de apertura. Apagado, cada panel se
    /// dibuja **una** vez y sin deslizar: menos CPU y menos movimiento en pantalla.
    /// No toca el OSD, las notificaciones ni el revelado del autohide.
    pub smooth_transitions: bool,
    pub pos_y: i32,
    pub dock_align: DockAlign,
    pub transparency: f32,
    pub blur_enabled: bool,
    pub blur_passes: i32,
    pub blur_size: i32,
    pub blur_vibrancy: f32,
    pub blur_brightness: f32,
    pub blur_contrast: f32,
    pub blur_xray: bool,
    pub accent_r: u8,
    pub accent_g: u8,
    pub accent_b: u8,
    pub accent2_r: u8,
    pub accent2_g: u8,
    pub accent2_b: u8,
    pub on_accent_r: u8,
    pub on_accent_g: u8,
    pub on_accent_b: u8,
    pub panel_r: u8,
    pub panel_g: u8,
    pub panel_b: u8,
    pub text_r: u8,
    pub text_g: u8,
    pub text_b: u8,
    pub text_dim_r: u8,
    pub text_dim_g: u8,
    pub text_dim_b: u8,
    pub accent_from_wallpaper: bool,
    pub last_wallpaper: String,
    /// Carpeta que lista el selector de fondos. Vacío = la de fábrica
    /// (`~/Pictures/Wallpapers`). Se elige desde el panel, en Appearance.
    #[serde(default)]
    pub wallpaper_dir: String,
    #[serde(default)]
    pub custom_theme: bool,
    #[serde(default = "default_matugen_scheme")]
    pub matugen_scheme: String,
    /// Corre el matugen del usuario (`~/.config/matugen/config.toml` → sus
    /// templates: kitty, waybar, rofi, niri, gtk, …) con el "Matugen Style" del
    /// dock, así las apps siguen al dock. Ajuste "Matugen Apps" en Colors.
    #[serde(default)]
    pub matugen_apps: bool,
    /// Modo para matugen (`--mode`): apagado = `dark`. Ajuste "Light Mode" en
    /// Colors. Aplica a los colores del dock (con `accent_from_wallpaper`) y a
    /// las apps (con "Matugen Apps"). Ver `DockSettings::matugen_mode`.
    #[serde(default)]
    pub matugen_light: bool,
    #[serde(default)]
    pub dock_font: String,
    #[serde(default)]
    pub system_font: String,
    pub dock_edge: DockEdge,
    #[serde(default = "default_true")]
    pub autohide: bool,
    #[serde(default = "default_hide_delay")]
    pub autohide_delay_ms: u64,
    #[serde(default = "default_widgets")]
    pub widgets: Vec<WidgetPlacement>,
    #[serde(default)]
    pub custom_palettes: Vec<CustomPalette>,
    /// Fuentes de widgets con script, direccionadas por posición como
    /// `WidgetKind::Custom(index)`. Vacío por defecto; a propósito no tiene
    /// control en el panel de Ajustes en esta prueba mínima del framework.
    #[serde(default)]
    pub custom_widgets: Vec<CustomWidgetSource>,
    /// Opciones por widget del tab "Widgets", direccionadas por `kind`.
    #[serde(default)]
    pub widget_options: Vec<WidgetOptionEntry>,
    /// Widget elegido en el tab "Widgets". Es estado de UI y por eso no se
    /// guarda (`skip`), pero vive acá porque `SettingId::get/set` reciben sólo
    /// `&DockSettings`: así las filas del editor son `SettingId` comunes y
    /// heredan gratis el slider, el teclado y el dibujo del panel.
    ///
    /// ponytail: estado de UI dentro de la config. Si aparece una segunda cosa
    /// que el panel necesite saber, conviene un `MenuCtx` (settings + selección)
    /// en vez de seguir sumando campos acá.
    #[serde(skip)]
    pub selected_widget: Option<WidgetKind>,
}

impl Default for DockSettings {
    fn default() -> Self {
        Self {
            icon_size: 35.688725,
            dock_scale: 0.335,
            widget_scale: 1.0,
            font_scale: default_font_scale(),
            media_smooth_scroll: false,
            media_width_scale: 1.2026273,
            icon_gap: 2.858622,
            magnify_scale: 1.35,
            magnify_radius: 80.0,
            width_padding: 67.51996,
            corner_radius: 8.40339,
            border_width: 0.0,
            menu_corner_radius: default_menu_radius(),
            menu_border_width: default_menu_border(),
            smooth_transitions: true,
            pos_y: 0,
            dock_align: DockAlign::Middle,
            transparency: 1.0,
            blur_enabled: false,
            blur_passes: 3,
            blur_size: 6,
            blur_vibrancy: 0.2,
            blur_brightness: 0.85,
            blur_contrast: 1.0,
            blur_xray: false,
            accent_r: 203,
            accent_g: 166,
            accent_b: 247,
            accent2_r: 137,
            accent2_g: 180,
            accent2_b: 250,
            on_accent_r: 20,
            on_accent_g: 20,
            on_accent_b: 24,
            panel_r: 37,
            panel_g: 33,
            panel_b: 43,
            text_r: 235,
            text_g: 235,
            text_b: 240,
            text_dim_r: 150,
            text_dim_g: 150,
            text_dim_b: 158,
            accent_from_wallpaper: false,
            custom_theme: false,
            last_wallpaper: String::new(),
            wallpaper_dir: String::new(),
            matugen_scheme: default_matugen_scheme(),
            matugen_apps: false,
            matugen_light: false,
            dock_font: "Adwaita Sans".to_string(),
            system_font: String::new(),
            dock_edge: DockEdge::Top,
            autohide: true,
            autohide_delay_ms: default_hide_delay(),
            widgets: default_widgets(),
            custom_palettes: Vec::new(),
            custom_widgets: Vec::new(),
            widget_options: Vec::new(),
            selected_widget: None,
        }
    }
}

impl DockSettings {
    /// Modo para matugen: `"light"` con el ajuste prendido, si no `"dark"`.
    pub fn matugen_mode(&self) -> &'static str {
        if self.matugen_light { "light" } else { "dark" }
    }

    /// Si `kind` está colocado en la barra (en cualquier slot). Una sola
    /// definición para todos: el reparto, los `refresh_*` que se saltean si el
    /// widget no está, y el gate del watcher de media (`App::publish_watcher_wants`).
    pub fn has_widget(&self, kind: WidgetKind) -> bool {
        self.widgets.iter().any(|w| w.kind == kind)
    }

    /// Opciones puestas de `kind`. `None` = no tiene ninguna.
    pub fn widget_options(&self, kind: WidgetKind) -> Option<&WidgetOptions> {
        self.widget_options
            .iter()
            .find(|e| e.kind == kind)
            .map(|e| &e.options)
    }

    /// Escala de letra de `kind`: la general por la suya. El valor sale
    /// recortado a `SIZE_SCALE_MIN..=SIZE_SCALE_MAX` para que un JSON editado a
    /// mano no deforme la barra.
    pub fn widget_font_scale(&self, kind: WidgetKind) -> f32 {
        let per_widget = self
            .widget_options(kind)
            .and_then(|o| o.font_scale)
            .unwrap_or(1.0)
            .clamp(SIZE_SCALE_MIN, SIZE_SCALE_MAX);
        self.font_scale * per_widget
    }

    /// Escala del símbolo de `kind`, con el mismo recorte que la letra. Un widget
    /// que no dibuja símbolo propio la ignora.
    pub fn widget_icon_scale(&self, kind: WidgetKind) -> f32 {
        self.widget_options(kind)
            .and_then(|o| o.icon_scale)
            .unwrap_or(1.0)
            .clamp(SIZE_SCALE_MIN, SIZE_SCALE_MAX)
    }

    /// Reloj en 24 h. Sólo el widget `Clock` lo mira.
    pub fn widget_clock_24h(&self, kind: WidgetKind) -> bool {
        self.widget_options(kind)
            .and_then(|o| o.clock_24h)
            .unwrap_or(false)
    }

    /// Guarda las opciones de `kind`. Un juego vacío borra la entrada, así el
    /// JSON no acumula widgets con todos los campos en el default.
    pub fn set_widget_options(&mut self, kind: WidgetKind, options: WidgetOptions) {
        self.widget_options.retain(|e| e.kind != kind);
        if !options.is_empty() {
            self.widget_options
                .push(WidgetOptionEntry { kind, options });
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub apps: Vec<PinnedApp>,
    pub settings: DockSettings,
    #[serde(skip)]
    pub profile: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            apps: default_apps(),
            settings: DockSettings::default(),
            profile: String::new(),
        }
    }
}

fn default_apps() -> Vec<PinnedApp> {
    Vec::new()
}

fn config_path(profile: &str) -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("dockyrs");
    dir.push(if profile.is_empty() {
        "config.json".to_string()
    } else {
        format!("config-{profile}.json")
    });
    dir
}

/// Copia al lado el config que no parsea (`config.json.bak-<ts>`) y devuelve su ruta.
/// `None` si tampoco se pudo escribir la copia. Separado de `load` para poder probarlo
/// con un archivo temporal sin tocar la config real del usuario.
fn respaldar_ilegible(path: &std::path::Path, raw: &str) -> Option<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let bak = path.with_extension(format!("json.bak-{ts}"));
    std::fs::write(&bak, raw).ok()?;
    Some(bak)
}

impl Config {
    pub fn load(profile: &str) -> Self {
        let path = config_path(profile);
        let mut cfg: Self = match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str(&raw) {
                Ok(cfg) => cfg,
                Err(err) => {
                    // ----- el archivo EXISTE pero no parsea: se arranca con los de
                    // fábrica, pero antes se guarda una copia al lado. Sin esto el
                    // primer `save()` (cualquier ajuste que toque el usuario) pisaba
                    // el archivo roto y sus ajustes se perdían sin rastro
                    // (AUDIT.md C3). -----
                    match respaldar_ilegible(&path, &raw) {
                        Some(bak) => log::error!(
                            "el config {path:?} no parsea ({err}): arranco con los de fábrica y dejo copia en {bak:?}"
                        ),
                        None => log::error!(
                            "el config {path:?} no parsea ({err}): arranco con los de fábrica y NO pude guardar la copia"
                        ),
                    }
                    Config::default()
                }
            },
            Err(_) => Config::default(),
        };
        cfg.profile = profile.to_string();
        cfg
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path(&self.profile);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }
}

#[cfg(test)]
mod config_backup_tests {
    use super::*;

    /// C3: un config que existe pero no parsea se respalda (`config.json.bak-<ts>`) y
    /// el original NO se pisa. Antes `load()` caía a los defaults y el primer `save()`
    /// borraba los ajustes del usuario sin dejar rastro.
    #[test]
    fn el_config_ilegible_se_respalda_sin_pisar_el_original() {
        let dir = std::env::temp_dir().join(format!("dockyrs-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir temporal");
        let path = dir.join("config.json");
        let roto = "{ esto no es json";
        std::fs::write(&path, roto).expect("escribir el roto");

        let bak = respaldar_ilegible(&path, roto).expect("el backup tiene que salir");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), roto, "la copia es igual");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            roto,
            "el original queda como estaba"
        );
        assert!(
            bak.to_string_lossy().contains(".json.bak-"),
            "el backup va al lado, con el timestamp: {bak:?}"
        );
        // ----- y un JSON válido sigue cargando normal -----
        let bueno = serde_json::to_string(&Config::default()).unwrap();
        std::fs::write(&path, &bueno).unwrap();
        assert!(serde_json::from_str::<Config>(&bueno).is_ok());
        assert!(respaldar_ilegible(&path, &bueno).is_some());
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod widget_options_tests {
    use super::*;

    fn con_opciones(kind: WidgetKind, options: WidgetOptions) -> DockSettings {
        let mut s = DockSettings::default();
        s.set_widget_options(kind, options);
        s
    }

    /// Sin nada puesto todo vale lo general: ninguna opción por widget cambia
    /// el look de fábrica.
    #[test]
    fn arrancan_vacias_y_con_el_valor_general() {
        let s = DockSettings::default();
        assert!(s.widget_options.is_empty());
        assert!(s.selected_widget.is_none());
        for kind in [WidgetKind::Clock, WidgetKind::Custom(0)] {
            assert_eq!(s.widget_font_scale(kind), 1.0, "{kind:?}");
            assert_eq!(s.widget_icon_scale(kind), 1.0, "{kind:?}");
            assert!(!s.widget_clock_24h(kind), "{kind:?}");
        }
    }

    /// Sola la escala del widget: la del vecino no se toca. Es todo el punto del
    /// editor.
    #[test]
    fn la_escala_de_un_widget_no_toca_al_resto() {
        let s = con_opciones(
            WidgetKind::Clock,
            WidgetOptions {
                font_scale: Some(1.5),
                ..Default::default()
            },
        );
        assert_eq!(s.widget_font_scale(WidgetKind::Clock), 1.5);
        assert_eq!(s.widget_font_scale(WidgetKind::Volume), 1.0);
    }

    /// La escala del widget multiplica a la general; el global sigue moviendo
    /// todo.
    #[test]
    fn la_escala_del_widget_multiplica_a_la_general() {
        let mut s = con_opciones(
            WidgetKind::Clock,
            WidgetOptions {
                font_scale: Some(1.5),
                ..Default::default()
            },
        );
        s.font_scale = 1.2;
        assert!((s.widget_font_scale(WidgetKind::Clock) - 1.8).abs() < 0.001);
        assert!((s.widget_font_scale(WidgetKind::Volume) - 1.2).abs() < 0.001);
    }

    /// Un JSON editado a mano no puede deformar la barra: el factor sale
    /// recortado a `SIZE_SCALE_MIN..=SIZE_SCALE_MAX`.
    #[test]
    fn un_factor_fuera_de_rango_se_recorta() {
        for (puesto, esperado) in [(99.0, SIZE_SCALE_MAX), (0.01, SIZE_SCALE_MIN)] {
            let s = con_opciones(
                WidgetKind::Clock,
                WidgetOptions {
                    font_scale: Some(puesto),
                    ..Default::default()
                },
            );
            assert_eq!(s.widget_font_scale(WidgetKind::Clock), esperado);
        }
    }

    /// Un juego vacío no deja entrada: el JSON guarda lo que el usuario tocó y
    /// no una lista de widgets con todos los campos en el default.
    #[test]
    fn volver_al_default_borra_la_entrada() {
        let mut s = con_opciones(
            WidgetKind::Clock,
            WidgetOptions {
                font_scale: Some(1.4),
                clock_24h: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(s.widget_options.len(), 1);
        assert!(s.widget_clock_24h(WidgetKind::Clock));

        s.set_widget_options(
            WidgetKind::Clock,
            WidgetOptions {
                font_scale: Some(1.4),
                ..Default::default()
            },
        );
        assert_eq!(s.widget_options.len(), 1, "quedaba la escala de letra");

        // ----- y tampoco se va con sólo el símbolo puesto: `is_empty` tiene que
        // mirar los tres campos, no dos -----
        s.set_widget_options(
            WidgetKind::Clock,
            WidgetOptions {
                icon_scale: Some(1.2),
                ..Default::default()
            },
        );
        assert_eq!(s.widget_options.len(), 1, "quedaba la escala del símbolo");

        s.set_widget_options(WidgetKind::Clock, WidgetOptions::default());
        assert!(
            s.widget_options.is_empty(),
            "la entrada vacía tenía que irse"
        );
    }

    /// La escala del símbolo es un knob aparte de la letra: son dos tamaños
    /// distintos del mismo widget y ninguno arrastra al otro.
    #[test]
    fn la_escala_del_simbolo_es_independiente_de_la_letra() {
        let s = con_opciones(
            WidgetKind::Volume,
            WidgetOptions {
                icon_scale: Some(1.6),
                ..Default::default()
            },
        );
        assert_eq!(s.widget_icon_scale(WidgetKind::Volume), 1.6);
        assert_eq!(
            s.widget_font_scale(WidgetKind::Volume),
            1.0,
            "la letra del volumen no se movió"
        );
        assert_eq!(
            s.widget_icon_scale(WidgetKind::Clock),
            1.0,
            "el símbolo del vecino no se movió"
        );
    }

    /// El símbolo se recorta con el mismo rango que la letra.
    #[test]
    fn el_factor_del_simbolo_se_recorta() {
        for (puesto, esperado) in [(99.0, SIZE_SCALE_MAX), (0.01, SIZE_SCALE_MIN)] {
            let s = con_opciones(
                WidgetKind::Volume,
                WidgetOptions {
                    icon_scale: Some(puesto),
                    ..Default::default()
                },
            );
            assert_eq!(s.widget_icon_scale(WidgetKind::Volume), esperado);
        }
    }

    /// Las opciones viajan al JSON y vuelven (el par va como `Vec` porque
    /// `WidgetKind::Custom` tiene payload y no sirve de clave de mapa).
    #[test]
    fn las_opciones_sobreviven_al_json() {
        let s = con_opciones(
            WidgetKind::Custom(3),
            WidgetOptions {
                font_scale: Some(1.25),
                ..Default::default()
            },
        );
        let json = serde_json::to_string(&s).expect("serializa");
        let vuelta: DockSettings = serde_json::from_str(&json).expect("deserializa");
        assert_eq!(vuelta.widget_font_scale(WidgetKind::Custom(3)), 1.25);
        // ----- sólo lo puesto: los otros campos no viajan como `null` -----
        assert!(
            !json.contains("null"),
            "el JSON guarda campos que el usuario no tocó: {json}"
        );
    }

    /// La selección es estado de UI: no se guarda, así que un panel abierto no
    /// ensucia el config con la última pestaña que se miró.
    #[test]
    fn la_seleccion_del_editor_no_se_guarda() {
        let s = DockSettings {
            selected_widget: Some(WidgetKind::Clock),
            ..Default::default()
        };
        let json = serde_json::to_string(&s).expect("serializa");
        assert!(
            !json.contains("selected_widget"),
            "la selección no puede viajar al JSON: {json}"
        );
        let vuelta: DockSettings = serde_json::from_str(&json).expect("deserializa");
        assert!(vuelta.selected_widget.is_none());
    }
}

#[cfg(test)]
mod has_widget_tests {
    use super::*;

    /// La única definición de "el widget está colocado": de acá cuelgan el reparto,
    /// los `refresh_*` que se saltean si no está y el gate del watcher de media (un
    /// `playerctl` residente de ~7 MB que no tiene sentido sin un `Media`).
    #[test]
    fn has_widget_mira_todos_los_slots() {
        let mut s = DockSettings {
            widgets: Vec::new(),
            ..Default::default()
        };
        assert!(!s.has_widget(WidgetKind::Media));
        s.widgets = vec![
            crate::config::WidgetPlacement {
                kind: WidgetKind::Clock,
                slot: WidgetSlot::Right,
            },
            crate::config::WidgetPlacement {
                kind: WidgetKind::Media,
                slot: WidgetSlot::Middle,
            },
        ];
        assert!(s.has_widget(WidgetKind::Media));
        assert!(s.has_widget(WidgetKind::Clock));
        assert!(!s.has_widget(WidgetKind::Battery));
    }
}
