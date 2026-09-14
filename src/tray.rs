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
    let proxy = zbus::blocking::proxy::Builder::<Proxy>::new(conn)
        .destination(WATCHER_IFACE)
        .and_then(|b| b.path(WATCHER_PATH))
        .and_then(|b| b.interface(WATCHER_IFACE))
        .map(|b| b.cache_properties(zbus::proxy::CacheProperties::No))
        .and_then(|b| b.build())
        .ok()?;
    let raw: Vec<String> = proxy.get_property("RegisteredStatusNotifierItems").ok()?;
    let raw_svc = raw.iter().find(|s| matches(s))?;
    let icon = resolve_item(conn, raw_svc)?;
    let menu_path = icon.menu_path?;
    Some((icon.service, icon.path, menu_path))
}

// ----- race winner -----
#[derive(Default)]
struct Watcher {
    items: Mutex<Vec<String>>,
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
fn tray_conn() -> Option<&'static Connection> {
    static CONN: std::sync::OnceLock<Option<Connection>> = std::sync::OnceLock::new();
    CONN.get_or_init(|| match Connection::session() {
        Ok(c) => Some(c),
        Err(e) => {
            log::warn!("tray: no hay bus de sesión: {e}");
            None
        }
    })
    .as_ref()
}

// ----- one level -----
pub fn fetch_menu(service: &str, menu_path: &str, parent_id: i32) -> Vec<TrayMenuItem> {
    let t0 = std::time::Instant::now();
    let Some(conn) = tray_conn() else {
        return Vec::new();
    };
    let Ok(proxy) = Proxy::new(
        conn,
        service.to_string(),
        menu_path.to_string(),
        "com.canonical.dbusmenu",
    ) else {
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
        if let Some(conn) = tray_conn()
            && let Ok(proxy) = Proxy::new(conn, service, menu_path, "com.canonical.dbusmenu")
        {
            let data = zbus::zvariant::Value::from(0i32);
            let _: zbus::Result<()> = proxy.call("Event", &(id, "clicked", data, 0u32));
        }
    });
}

pub fn activate(service: String, path: String) {
    std::thread::spawn(move || {
        if let Some(conn) = tray_conn()
            && let Ok(proxy) = item_proxy(conn, &service, &path)
        {
            let _: zbus::Result<()> = proxy.call("Activate", &(0i32, 0i32));
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
pub fn spawn(
    state: TrayState,
    flag: Arc<std::sync::atomic::AtomicBool>,
    wl_conn: wayland_client::Connection,
    qh: wayland_client::QueueHandle<crate::app::App>,
) {
    let _ = std::thread::Builder::new()
        .name("tray".into())
        .spawn(move || {
            let owned = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name(WATCHER_IFACE))
                .and_then(|b| b.serve_at(WATCHER_PATH, Watcher::default()))
                .and_then(|b| b.build());
            let conn = match owned {
                Ok(c) => c,
                Err(_) => match Connection::session() {
                    Ok(c) => c,
                    Err(e) => {
                        log::warn!("tray: session bus unavailable: {e}");
                        return;
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

            let mut last_fingerprint = 0u64;
            loop {
                let raw: Vec<String> = watcher_proxy
                    .as_ref()
                    .and_then(|p| {
                        p.get_property::<Vec<String>>("RegisteredStatusNotifierItems")
                            .ok()
                    })
                    .unwrap_or_default();
                let icons: Vec<TrayIcon> = raw
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
        });
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
