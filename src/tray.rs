use std::sync::{Arc, Mutex};
use std::time::Duration;

use zbus::blocking::fdo::PropertiesProxy;
use zbus::blocking::{Connection, Proxy};
const WATCHER_IFACE: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM_IFACE: &str = "org.kde.StatusNotifierItem";

#[derive(Clone, Default)]
pub struct TrayIcon {
    pub service: String,
    pub path: String,
    pub icon_name: String,
    pub icon_pixmap: Option<Arc<tiny_skia::Pixmap>>,
    pub menu_path: Option<String>,
}

#[derive(Clone)]
pub struct TrayMenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub is_separator: bool,
    pub has_submenu: bool,
}

pub type TrayState = Arc<Mutex<Vec<TrayIcon>>>;

/// Una petición de menú del tray. Viaja al hilo de `spawn_menu_worker` en vez de
/// resolverse en el hilo que dibuja: `GetLayout` es una llamada D-Bus bloqueante (el
/// techo es `TRAY_CALL_TIMEOUT`, 500 ms; el default de zbus eran 25 s), y una app del
/// tray colgada no puede congelar el dock (A3 de AUDIT.md).
pub struct MenuRequest {
    pub service: String,
    pub menu_path: String,
    /// Path del item StatusNotifier. Sólo lo usa el menú raíz: es el fallback
    /// `Activate` cuando el menú vuelve vacío (`None` en los submenús, donde ese
    /// fallback no existe).
    pub item_path: Option<String>,
    /// `0` = menú raíz; si no, el id del item cuyo submenú se pidió.
    pub parent_id: i32,
    pub center: Option<(f32, f32)>,
}

/// Lo que devuelve el hilo: el menú y la petición que lo pidió, para que quien
/// aplica pueda descartarlo si el usuario ya cambió de idea.
pub struct MenuResult {
    pub req: MenuRequest,
    pub items: Vec<TrayMenuItem>,
}

/// Hilo único que resuelve los menús del tray fuera del hilo que dibuja. Es uno solo
/// (no un hilo por click) para que las peticiones se atiendan en orden: el último
/// click pisa al anterior en vez de que gane el que conteste primero. Con el
/// `method_timeout` de la conexión, una app colgada no puede tapar la cola.
pub fn spawn_menu_worker(
    rx: std::sync::mpsc::Receiver<MenuRequest>,
    tx: std::sync::mpsc::Sender<crate::ipc::IpcMessage>,
    wl_conn: wayland_client::Connection,
    qh: wayland_client::QueueHandle<crate::app::App>,
) {
    std::thread::spawn(move || {
        while let Ok(req) = rx.recv() {
            let items = fetch_menu(&req.service, &req.menu_path, req.parent_id);
            let result = Box::new(MenuResult { req, items });
            if tx
                .send(crate::ipc::IpcMessage::TrayMenuReady(result))
                .is_ok()
            {
                wl_conn.display().sync(&qh, ());
                let _ = wl_conn.flush();
            }
        }
    });
}

// ----- el tray visible ignora wifi y bluetooth: ya tienen sus widgets propios
// (Network/Bluetooth) y en el tray solo duplican ruido. Se mira icon_name
// ("nm-signal-75", "blueman-tray") y path (".../nm_applet", "/org/blueman/sni")
// porque el service suele ser un nombre único (:1.20) que no dice nada. -----
fn tray_ignored(icon: &TrayIcon) -> bool {
    let name = icon.icon_name.to_lowercase();
    let path = icon.path.to_lowercase();
    // ponytail: heurística por nombre, sin lista de configuración. El techo:
    // cualquier app que use el prefijo "nm-" para OTRA cosa se filtra; si eso
    // pasa, esto es lo primero que hay que tocar (ya hay tests en tray.rs).
    name.starts_with("nm-")
        || name.contains("blueman")
        || name.contains("bluetooth")
        || name.contains("blueberry")
        || path.contains("nm_applet")
        || path.contains("blueman")
        || path.contains("bluetooth")
}

// ----- menú de un item aunque esté filtrado del tray visible: los widgets de
// la izquierda abren por acá los menús de nm-applet/blueman con click derecho.
// Devuelve (service, path, menu_path). -----
pub fn find_menu(matches: impl Fn(&str) -> bool) -> Option<(String, String, String)> {
    let conn = tray_conn()?;
    let proxy = zbus::blocking::proxy::Builder::<Proxy>::new(&conn)
        .destination(WATCHER_IFACE)
        .and_then(|b| b.path(WATCHER_PATH))
        .and_then(|b| b.interface(WATCHER_IFACE))
        .map(|b| b.cache_properties(zbus::proxy::CacheProperties::No))
        .and_then(|b| b.build())
        .ok()?;
    let raw: Vec<String> = match proxy.get_property("RegisteredStatusNotifierItems") {
        Ok(v) => v,
        Err(_) => {
            tray_conn_invalidar();
            return None;
        }
    };
    let raw_svc = raw.iter().find(|s| matches(s))?;
    let icon = resolve_item(&conn, raw_svc)?;
    let menu_path = icon.menu_path?;
    Some((icon.service, icon.path, menu_path))
}

// ----- race winner -----
#[derive(Default)]
struct Watcher {
    /// Items registrados, **compartidos con el hilo del poll**: es el que purga los de
    /// servicios que ya no están en el bus. Sin la purga la lista crece para siempre y
    /// cada vuelta de 2 s intenta resolver items que ya no existen (AUDIT.md D5).
    items: Arc<Mutex<Vec<String>>>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) {
        let entry = if service.starts_with('/') {
            let sender = header.sender().map(|s| s.to_string()).unwrap_or_default();
            format!("{sender}{service}")
        } else if service.contains('/') {
            service.to_string()
        } else {
            format!("{service}/StatusNotifierItem")
        };
        let mut items = self.items.lock().unwrap();
        if !items.iter().any(|s| s == &entry) {
            items.push(entry);
        }
    }

    fn register_status_notifier_host(&self, _service: &str) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().unwrap().clone()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

fn split_service(raw: &str) -> (String, String) {
    match raw.split_once('/') {
        Some((svc, path)) => (svc.to_string(), format!("/{path}")),
        None => (raw.to_string(), "/StatusNotifierItem".to_string()),
    }
}

/// Deja los items cuyo servicio sigue vivo y sin repetidos (el mismo servicio puede
/// registrarse dos veces). `vivo` recibe el nombre del bus ya extraído: separado así del
/// D-Bus para poder probarlo con un predicado falso.
fn items_vivos(raw: &[String], vivo: impl Fn(&str) -> bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    for item in raw {
        if out.iter().any(|s| s == item) {
            continue;
        }
        let (service, _) = split_service(item);
        if vivo(&service) {
            out.push(item.clone());
        }
    }
    out
}

/// ¿El servicio sigue en el bus? Ante error se asume que **sí**: es peor sacar de la lista
/// un item que existe (el ícono desaparece y no vuelve hasta que la app se re-registre)
/// que preguntar de más.
fn nombre_tiene_dueno(conn: &Connection, service: &str) -> bool {
    let Ok(name) = zbus::names::BusName::try_from(service.to_string()) else {
        return false;
    };
    zbus::blocking::fdo::DBusProxy::new(conn)
        .ok()
        .and_then(|p| p.name_has_owner(name).ok())
        .unwrap_or(true)
}

fn item_proxy<'a>(conn: &Connection, service: &str, path: &str) -> zbus::Result<Proxy<'a>> {
    Proxy::new(conn, service.to_string(), path.to_string(), ITEM_IFACE)
}

// ----- premultiplied argb -----
fn argb_to_pixmap(width: i32, height: i32, data: &[u8]) -> Option<tiny_skia::Pixmap> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let (w, h) = (width as u32, height as u32);
    if data.len() < (w as usize) * (h as usize) * 4 {
        return None;
    }
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    let dst = pixmap.data_mut();
    for i in 0..(w as usize * h as usize) {
        let a = data[i * 4] as u16;
        let r = data[i * 4 + 1] as u16;
        let g = data[i * 4 + 2] as u16;
        let b = data[i * 4 + 3] as u16;
        dst[i * 4] = ((r * a) / 255) as u8;
        dst[i * 4 + 1] = ((g * a) / 255) as u8;
        dst[i * 4 + 2] = ((b * a) / 255) as u8;
        dst[i * 4 + 3] = a as u8;
    }
    Some(pixmap)
}

// ----- item gone -----
fn resolve_item(conn: &Connection, raw_svc: &str) -> Option<TrayIcon> {
    let (service, path) = split_service(raw_svc);
    let props = PropertiesProxy::builder(conn)
        .destination(service.clone())
        .ok()?
        .path(path.clone())
        .ok()?
        .build()
        .ok()?
        .get_all(zbus::names::InterfaceName::try_from(ITEM_IFACE).ok()?)
        .ok()?;
    let icon_name = props
        .get("IconName")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();
    let icon_pixmap = props
        .get("IconPixmap")
        .and_then(|v| Vec::<(i32, i32, Vec<u8>)>::try_from(v.clone()).ok())
        .and_then(|raw| raw.into_iter().max_by_key(|(w, h, _)| (*w).max(*h)))
        .and_then(|(w, h, data)| argb_to_pixmap(w, h, &data))
        .map(Arc::new);
    let menu_path = props
        .get("Menu")
        .and_then(|v| zbus::zvariant::OwnedObjectPath::try_from(v.clone()).ok())
        .map(|p| p.to_string());
    Some(TrayIcon {
        service,
        path,
        icon_name,
        icon_pixmap,
        menu_path,
    })
}

// ----- conexión compartida: antes cada llamada hacía Connection::session(),
// o sea el handshake completo de D-Bus, y encima en el hilo que dibuja -----
//
// El `method_timeout` es el techo de CUALQUIER llamada del tray, y es lo que hace
// que una app colgada no pueda frenar nada durante los 25 s del default de zbus.
// Medido en esta máquina (`GetLayout` con gdbus, que incluye el spawn del proceso):
// nm-applet 10,6-13,4 ms y blueman 17,5-20,2 ms, o sea que 500 ms es ~25-50x el
// costo real y no hay riesgo de cortar una respuesta lenta pero legítima.
const TRAY_CALL_TIMEOUT: Duration = Duration::from_millis(500);

/// Caché de una conexión con invalidación explícita. El estado vive acá, sin D-Bus, para
/// poder probarlo: `obtener` construye una sola vez y reusa, `invalidar` obliga a
/// reconstruir en la próxima. Es lo que hace que el tray se recupere cuando el bus de
/// sesión se reinicia (AUDIT.md B5).
#[derive(Default)]
struct CacheConexion<T> {
    valor: Mutex<Option<T>>,
}

impl<T: Clone> CacheConexion<T> {
    fn obtener(&self, construir: impl FnOnce() -> Option<T>) -> Option<T> {
        let mut slot = self.valor.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = slot.as_ref() {
            return Some(v.clone());
        }
        let v = construir()?;
        *slot = Some(v.clone());
        Some(v)
    }

    fn invalidar(&self) {
        *self.valor.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Conexión al bus de sesión para el tray. Antes era un `OnceLock` que se quedaba con la
/// primera conexión **para siempre**: si el bus se reiniciaba (o el socket se cortaba),
/// todas las llamadas del tray fallaban y no se recuperaba hasta reiniciar el dock
/// (AUDIT.md B5). Ahora, cuando una llamada falla, se invalida la caché
/// (`tray_conn_invalidar`) y la próxima reconecta.
fn tray_conn_cache() -> &'static CacheConexion<Arc<Connection>> {
    static CONN: std::sync::OnceLock<CacheConexion<Arc<Connection>>> = std::sync::OnceLock::new();
    CONN.get_or_init(|| CacheConexion {
        valor: Mutex::new(None),
    })
}

fn tray_conn() -> Option<Arc<Connection>> {
    tray_conn_cache().obtener(|| {
        let built = zbus::blocking::connection::Builder::session()
            .map(|b| b.method_timeout(TRAY_CALL_TIMEOUT))
            .and_then(|b| b.build());
        match built {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                log::warn!("tray: no hay bus de sesión: {e}");
                None
            }
        }
    })
}

/// Tira la conexión cacheada: la próxima llamada reconecta. Se llama donde una operación
/// del tray falla, que es el único síntoma de que el bus se reinició.
fn tray_conn_invalidar() {
    log::debug!("tray: la conexión al bus falló; la próxima reconecta");
    tray_conn_cache().invalidar();
}

// ----- one level -----
pub fn fetch_menu(service: &str, menu_path: &str, parent_id: i32) -> Vec<TrayMenuItem> {
    let t0 = std::time::Instant::now();
    let Some(conn) = tray_conn() else {
        return Vec::new();
    };
    let Ok(proxy) = Proxy::new(
        &conn,
        service.to_string(),
        menu_path.to_string(),
        "com.canonical.dbusmenu",
    ) else {
        tray_conn_invalidar();
        return Vec::new();
    };
    let names: Vec<&str> = vec!["type", "label", "enabled", "visible", "children-display"];
    type Layout = (
        i32,
        std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        Vec<zbus::zvariant::OwnedValue>,
    );
    let t_call = std::time::Instant::now();
    let Ok((_, (_, _, children))) =
        proxy.call::<_, _, (u32, Layout)>("GetLayout", &(parent_id, 1i32, names))
    else {
        // ----- si el bus se reinició, esto falla al instante: invalidar la caché es lo que
        // hace que la próxima apertura del menú reconecte (B5) -----
        tray_conn_invalidar();
        return Vec::new();
    };
    // ----- desglose de latencia (visible con RUST_LOG=debug) -----
    log::debug!(
        "tray: {menu_path} setup={}ms getlayout={}ms",
        t_call.duration_since(t0).as_millis(),
        t_call.elapsed().as_millis()
    );
    children
        .into_iter()
        .filter_map(|child| {
            let (id, props, _): Layout = child.try_into().ok()?;
            let visible = props
                .get("visible")
                .and_then(|v| bool::try_from(v.clone()).ok())
                .unwrap_or(true);
            if !visible {
                return None;
            }
            let is_separator = props
                .get("type")
                .and_then(|v| String::try_from(v.clone()).ok())
                .as_deref()
                == Some("separator");
            let enabled = props
                .get("enabled")
                .and_then(|v| bool::try_from(v.clone()).ok())
                .unwrap_or(true);
            let label = props
                .get("label")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default()
                .replace('_', "");
            if !is_separator && label.is_empty() {
                return None;
            }
            let has_submenu = props
                .get("children-display")
                .and_then(|v| String::try_from(v.clone()).ok())
                .as_deref()
                == Some("submenu");
            Some(TrayMenuItem {
                id,
                label,
                enabled,
                is_separator,
                has_submenu,
            })
        })
        .collect()
}

pub fn send_menu_event(service: String, menu_path: String, id: i32) {
    std::thread::spawn(move || {
        let Some(conn) = tray_conn() else {
            return;
        };
        let Ok(proxy) = Proxy::new(&conn, service, menu_path, "com.canonical.dbusmenu") else {
            return;
        };
        let data = zbus::zvariant::Value::from(0i32);
        if proxy
            .call::<_, _, ()>("Event", &(id, "clicked", data, 0u32))
            .is_err()
        {
            tray_conn_invalidar();
        }
    });
}

pub fn activate(service: String, path: String) {
    std::thread::spawn(move || {
        let Some(conn) = tray_conn() else {
            return;
        };
        let Ok(proxy) = item_proxy(&conn, &service, &path) else {
            return;
        };
        if proxy.call::<_, _, ()>("Activate", &(0i32, 0i32)).is_err() {
            tray_conn_invalidar();
        }
    });
}

fn fingerprint_icons(icons: &[TrayIcon]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for icon in icons {
        icon.service.hash(&mut hasher);
        icon.path.hash(&mut hasher);
        icon.icon_name.hash(&mut hasher);
        icon.menu_path.hash(&mut hasher);
        if let Some(pixmap) = &icon.icon_pixmap {
            pixmap.width().hash(&mut hasher);
            pixmap.height().hash(&mut hasher);
            pixmap.data().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// polls state simpler than signals
/// Cada cuánto reintenta el watcher cuando el bus no está (o se cayó): el dock se relanza
/// cuando el bus se reinicia y puede llegar **antes** que el bus nuevo.
const RECONEXION_MS: u64 = 2000;

/// Corre el watcher del tray: se conecta al bus de sesión, se registra como host de SNI y
/// pollea los items cada 2 s.
///
/// Está en un lazo de reconexión por B5: si el bus de sesión se reinicia, la conexión (y el
/// nombre de `StatusNotifierWatcher`) se pierden, así que hay que rehacer las dos cosas. En
/// ese caso el dock además se relanza (`relanzar_por_bus_caido`), porque las llamadas del
/// tray del resto del dock comparten conexiones cacheadas; y este lazo se encarga de que el
/// relanzado, si llega antes que el bus, se conecte igual.
fn correr_watcher_tray(
    state: TrayState,
    flag: Arc<std::sync::atomic::AtomicBool>,
    wl_conn: wayland_client::Connection,
    qh: wayland_client::QueueHandle<crate::app::App>,
) {
    // ----- el Watcher y el poll comparten la lista de items: el poll purga -----
    let items: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut last_fingerprint = 0u64;
    loop {
        // ----- conexión + registro como host -----
        let owned = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(WATCHER_IFACE))
            .and_then(|b| {
                b.serve_at(
                    WATCHER_PATH,
                    Watcher {
                        items: items.clone(),
                    },
                )
            })
            .and_then(|b| b.build());
        let conn = match owned {
            Ok(c) => c,
            Err(_) => match Connection::session() {
                Ok(c) => c,
                Err(e) => {
                    // ----- sin bus (todavía): se reintenta, no se sale -----
                    log::debug!("tray: no hay bus de sesión: {e}");
                    std::thread::sleep(Duration::from_millis(RECONEXION_MS));
                    continue;
                }
            },
        };
        let watcher_proxy = zbus::blocking::proxy::Builder::<Proxy>::new(&conn)
            .destination(WATCHER_IFACE)
            .and_then(|b| b.path(WATCHER_PATH))
            .and_then(|b| b.interface(WATCHER_IFACE))
            .map(|b| b.cache_properties(zbus::proxy::CacheProperties::No))
            .and_then(|b| b.build())
            .ok();
        if let Some(p) = &watcher_proxy {
            let _: zbus::Result<()> = p.call("RegisterStatusNotifierHost", &("dockyrs",));
        }
        log::debug!("tray: conectado al bus de sesión");

        // ----- poll -----
        let mut fallos_seguidos = 0u32;
        loop {
            let Some(proxy) = watcher_proxy.as_ref() else {
                std::thread::sleep(Duration::from_millis(RECONEXION_MS));
                continue;
            };
            let crudos: Vec<String> =
                match proxy.get_property::<Vec<String>>("RegisteredStatusNotifierItems") {
                    Ok(v) => {
                        fallos_seguidos = 0;
                        v
                    }
                    Err(err) => {
                        fallos_seguidos += 1;
                        log::debug!("tray: no pude leer el bus ({fallos_seguidos}): {err}");
                        // ----- 3 vueltas seguidas sin poder leer = el bus se reinició: se
                        // reconecta Y se relanza el dock (las conexiones cacheadas del
                        // resto del tray quedaron muertas). El tope de `reexec` corta el
                        // bucle si el problema es determinista. -----
                        if fallos_seguidos >= 3 {
                            crate::relanzar_por_bus_caido();
                        }
                        std::thread::sleep(Duration::from_millis(RECONEXION_MS));
                        continue;
                    }
                };
            // ----- purga: un item de un servicio que ya no está en el bus no se puede
            // resolver (`GetAll` falla) y si se queda en la lista se acumula para
            // siempre, costando un round-trip cada 2 s (AUDIT.md D5) -----
            let vivos = items_vivos(&crudos, |svc| nombre_tiene_dueno(&conn, svc));
            if vivos.len() != crudos.len() {
                log::debug!(
                    "tray: purgo {} item(s) de servicios que ya no estan",
                    crudos.len() - vivos.len()
                );
                *items.lock().unwrap() = vivos.clone();
            }
            let icons: Vec<TrayIcon> = vivos
                .iter()
                .filter_map(|raw_svc| resolve_item(&conn, raw_svc))
                .filter(|icon| !tray_ignored(icon))
                .collect();
            let fingerprint = fingerprint_icons(&icons);
            *state.lock().unwrap() = icons;
            if fingerprint != last_fingerprint {
                last_fingerprint = fingerprint;
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                wl_conn.display().sync(&qh, ());
                let _ = wl_conn.flush();
            }
            std::thread::sleep(Duration::from_millis(2000));
        }
    }
}

pub fn spawn(
    state: TrayState,
    flag: Arc<std::sync::atomic::AtomicBool>,
    wl_conn: wayland_client::Connection,
    qh: wayland_client::QueueHandle<crate::app::App>,
) {
    let _ = std::thread::Builder::new()
        .name("tray".into())
        .spawn(move || correr_watcher_tray(state, flag, wl_conn, qh));
}

#[cfg(test)]
mod tray_ignore_tests {
    use super::*;

    fn icon(icon_name: &str, path: &str) -> TrayIcon {
        TrayIcon {
            icon_name: icon_name.into(),
            path: path.into(),
            ..Default::default()
        }
    }

    #[test]
    fn ignorados_los_que_duplican_widgets() {
        // los nombres reales que exponen nm-applet y blueman en este equipo
        assert!(tray_ignored(&icon(
            "nm-signal-75",
            "/org/ayatana/NotificationItem/nm_applet"
        )));
        assert!(tray_ignored(&icon("blueman-tray", "/org/blueman/sni")));
        // por nombre alcanza, aunque el path cambie
        assert!(tray_ignored(&icon("nm-device-wired", "/x")));
        assert!(tray_ignored(&icon("bluetooth-symbolic", "/x")));
    }

    #[test]
    fn no_ignorados_los_demas() {
        assert!(!tray_ignored(&icon(
            "org.remmina.Remmina-status",
            "/org/ayatana/NotificationItem/remmina_icon"
        )));
        assert!(!tray_ignored(&icon("telegram", "/StatusNotifierItem")));
        // ojo: cualquier cosa con "nm-" adelante se filtra (nm-applet usa ese
        // prefijo para todos sus iconos); si un día una app paga esa deuda,
        // esto es lo que falla primero
        assert!(!tray_ignored(&icon("network-wired", "/x")));
    }
}

#[cfg(test)]
mod tray_purga_tests {
    use super::items_vivos;

    /// D5: la lista del watcher se queda sólo con los items que siguen vivos, sin
    /// repetidos. Sin la purga, los de apps que ya cerraron quedan para siempre y cada
    /// vuelta de 2 s les intenta resolver el ícono.
    #[test]
    fn la_lista_pierde_los_items_de_servicios_muertos() {
        let raw = vec![
            ":1.20/StatusNotifierItem".to_string(),
            ":1.30/StatusNotifierItem".to_string(),
            ":1.20/StatusNotifierItem".to_string(), // repetido
            "org.kde.StatusNotifierItem-1234-1/StatusNotifierItem".to_string(),
        ];
        let vivos = items_vivos(&raw, |svc| svc != ":1.30");
        assert_eq!(
            vivos,
            vec![
                ":1.20/StatusNotifierItem".to_string(),
                "org.kde.StatusNotifierItem-1234-1/StatusNotifierItem".to_string(),
            ],
            "saca el muerto y el repetido"
        );
        // ----- y si no hay nada muerto, la lista queda igual -----
        assert_eq!(items_vivos(&raw, |_| true).len(), 3);
        // ----- todos muertos: lista vacía (y el tray no dibuja nada) -----
        assert!(items_vivos(&raw, |_| false).is_empty());
    }
}

#[cfg(test)]
mod tray_conexion_tests {
    use super::CacheConexion;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// B5: la conexión se cachea (se construye una sola vez y se reusa) y `invalidar()`
    /// obliga a reconstruir en la próxima. Es lo que hace que el tray se recupere cuando el
    /// bus de sesión se reinicia: antes era un `OnceLock` con la primera conexión para
    /// siempre, así que todas las llamadas del tray fallaban hasta reiniciar el dock.
    #[test]
    fn la_conexion_se_reusa_y_se_reconstruye_al_invalidar() {
        let cache: CacheConexion<u32> = CacheConexion::default();
        let construcciones = AtomicUsize::new(0);
        let construir = |valor: u32, n: &AtomicUsize| {
            n.fetch_add(1, Ordering::SeqCst);
            Some(valor)
        };
        // ----- primera: construye; segunda: reusa lo cacheado -----
        assert_eq!(cache.obtener(|| construir(1, &construcciones)), Some(1));
        assert_eq!(
            cache.obtener(|| construir(2, &construcciones)),
            Some(1),
            "reusa la conexión que ya estaba"
        );
        assert_eq!(construcciones.load(Ordering::SeqCst), 1);
        // ----- el bus se reinició: se invalida y la próxima reconecta -----
        cache.invalidar();
        assert_eq!(cache.obtener(|| construir(3, &construcciones)), Some(3));
        assert_eq!(construcciones.load(Ordering::SeqCst), 2);
        // ----- si no se puede construir (no hay bus), devuelve None y NO cachea nada -----
        cache.invalidar();
        assert_eq!(cache.obtener(|| None), None);
        assert_eq!(
            construcciones.load(Ordering::SeqCst),
            2,
            "el fallo no cuenta como construcción"
        );
        assert_eq!(
            cache.obtener(|| construir(4, &construcciones)),
            Some(4),
            "y el próximo intento sí construye"
        );
    }
}
