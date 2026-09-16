use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::time::Duration;
use wayland_client::{Connection, QueueHandle};

#[derive(Clone, Debug)]
pub enum IpcMessage {
    ToggleSearch,
    OsdVolume,
    OsdBrightness,
    MediaChanged,
    VolumeChanged,
    BatteryChanged,
    BluetoothChanged,
    WorkspacesChanged,
    ToggleWallpaper,
    ToggleClipboard,
    ScreenshotFull,
    ScreenshotRegion,
    ToggleDockMenu,
    TestNotification,
    Notify(String, String),
}

const NOTIFY_SEP: char = '\u{1f}';

fn socket_path(profile: &str) -> std::path::PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(dir).join(if profile.is_empty() {
        "dockyrs.sock".to_string()
    } else {
        format!("dockyrs-{profile}.sock")
    })
}

pub fn send_message(text: &str, profile: &str) {
    if let Ok(mut stream) = UnixStream::connect(socket_path(profile)) {
        let _ = stream.write_all(text.as_bytes());
    }
}

// ----- cuánto esperamos a que un cliente escriba su comando. El listener es UN
// solo hilo y `handle_client` hace un `read` bloqueante, así que sin este tope el
// primer cliente que conecta y no escribe deja el loop esperándolo PARA SIEMPRE:
// ningún `--toggle-*`, screenshot ni notify vuelve a funcionar y no hay forma de
// saber por qué. Un `send_message` real escribe inmediatamente después del
// connect, así que 50 ms es holgado; lo único que corta es al cliente roto. -----
const IPC_READ_TIMEOUT: Duration = Duration::from_millis(50);

/// Deja al cliente con el timeout puesto. Separado para poder fijarlo en un test
/// sin fabricar un `App` ni una conexión Wayland.
fn preparar_cliente(stream: &UnixStream) {
    let _ = stream.set_read_timeout(Some(IPC_READ_TIMEOUT));
}

// ----- wakes loop -----
pub fn spawn_listener(
    tx: Sender<IpcMessage>,
    conn: Connection,
    qh: QueueHandle<crate::app::App>,
    profile: String,
) {
    let path = socket_path(&profile);
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(err) => {
            log::warn!("failed to bind dockyrs ipc socket at {path:?}: {err}");
            return;
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            preparar_cliente(&stream);
            handle_client(stream, &tx, &conn, &qh);
        }
    });
}

fn handle_client(
    mut stream: UnixStream,
    tx: &Sender<IpcMessage>,
    conn: &Connection,
    qh: &QueueHandle<crate::app::App>,
) {
    let mut buf = [0u8; 1024];
    let Ok(n) = stream.read(&mut buf) else {
        return;
    };
    let text = String::from_utf8_lossy(&buf[..n]);
    let msg = match text.as_ref() {
        "toggle-search" => Some(IpcMessage::ToggleSearch),
        "osd-volume" => Some(IpcMessage::OsdVolume),
        "osd-brightness" => Some(IpcMessage::OsdBrightness),
        "toggle-wallpaper" => Some(IpcMessage::ToggleWallpaper),
        "toggle-clipboard" => Some(IpcMessage::ToggleClipboard),
        "screenshot-full" => Some(IpcMessage::ScreenshotFull),
        "screenshot-region" => Some(IpcMessage::ScreenshotRegion),
        "toggle-dock-menu" => Some(IpcMessage::ToggleDockMenu),
        "test-notification" => Some(IpcMessage::TestNotification),
        other => other.strip_prefix("notify").and_then(|rest| {
            let mut parts = rest.strip_prefix(NOTIFY_SEP)?.splitn(2, NOTIFY_SEP);
            let title = parts.next()?.to_string();
            let body = parts.next().unwrap_or("").to_string();
            Some(IpcMessage::Notify(title, body))
        }),
    };
    let Some(msg) = msg else {
        return;
    };
    if tx.send(msg).is_ok() {
        conn.display().sync(qh, ());
        let _ = conn.flush();
    }
}

pub fn spawn_brightness_watcher(
    tx: Sender<IpcMessage>,
    conn: Connection,
    qh: QueueHandle<crate::app::App>,
) {
    let Some(dir) = crate::widgets::backlight_dir() else {
        return;
    };
    let path = dir.join("brightness");
    std::thread::spawn(move || {
        let Ok(mut inotify) = inotify::Inotify::init() else {
            return;
        };
        if inotify
            .watches()
            .add(&path, inotify::WatchMask::MODIFY)
            .is_err()
        {
            return;
        }
        let mut buffer = [0u8; 1024];
        loop {
            let Ok(events) = inotify.read_events_blocking(&mut buffer) else {
                return;
            };
            if events.count() == 0 {
                continue;
            }
            if tx.send(IpcMessage::OsdBrightness).is_ok() {
                conn.display().sync(&qh, ());
                let _ = conn.flush();
            }
        }
    });
}

pub fn spawn_battery_watcher(
    tx: Sender<IpcMessage>,
    conn: Connection,
    qh: QueueHandle<crate::app::App>,
) {
    let Some(dir) = crate::widgets::battery_dir() else {
        return;
    };
    std::thread::spawn(move || {
        use std::os::unix::io::AsRawFd;
        loop {
            let mut files: Vec<std::fs::File> = ["capacity", "status", "energy_now"]
                .iter()
                .filter_map(|name| std::fs::File::open(dir.join(name)).ok())
                .collect();
            for f in &mut files {
                let _ = f.read(&mut [0u8; 64]);
            }

            let mut pfds: Vec<libc::pollfd> = files
                .iter()
                .map(|f| libc::pollfd {
                    fd: f.as_raw_fd(),
                    events: libc::POLLPRI | libc::POLLERR,
                    revents: 0,
                })
                .collect();
            // ----- `poll` con POLLPRI es lo correcto (los sysfs de batería avisan
            // cuando el driver los soporta), pero hay laptops donde NO avisan nunca:
            // medido en la de desarrollo, `poll(POLLPRI)` sobre `capacity`/`status`/
            // `energy_now` vuelve a los 30 s con 0 eventos. Con un timeout de 30 s
            // eso dejaba el `BatteryChanged` cada ~30,5 s, y como el widget no dibuja
            // NADA mientras no tenga dato, era un hueco de hasta 30 s al arrancar y
            // al colocar el widget. Leer cuesta 3 ms (medido), así que el timeout
            // baja a 3 s: en las que sí notifican sigue siendo instantáneo, y en
            // estas el dato queda a lo sumo 3 s viejo. -----
            if pfds.is_empty() {
                std::thread::sleep(std::time::Duration::from_secs(3));
            } else {
                unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, 3_000) };
            }

            if tx.send(IpcMessage::BatteryChanged).is_ok() {
                conn.display().sync(&qh, ());
                let _ = conn.flush();
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

// ----- parent death signal -----
fn die_with_parent(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    cmd
}

pub fn spawn_media_watcher(
    tx: Sender<IpcMessage>,
    conn: Connection,
    qh: QueueHandle<crate::app::App>,
) {
    std::thread::spawn(move || {
        loop {
            let mut cmd = std::process::Command::new("playerctl");
            cmd.args([
                "-a",
                "metadata",
                "--follow",
                "--format",
                "{{status}}\t{{title}}\t{{artist}}",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
            let child = die_with_parent(&mut cmd).spawn();
            let Ok(mut child) = child else {
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            };
            if let Some(stdout) = child.stdout.take() {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let _ = line;
                    if tx.send(IpcMessage::MediaChanged).is_ok() {
                        conn.display().sync(&qh, ());
                        let _ = conn.flush();
                    }
                }
            }
            let _ = child.wait();
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    });
}

/// Escucha los cambios de volumen de PipeWire/PulseAudio en vez de esperar al
/// tick de sistema. Las teclas de volumen del sistema corren `wpctl` **por fuera
/// del dock**, así que el widget sólo se enteraba en el próximo sondeo: hasta 2 s
/// de atraso y salteándose los pasos intermedios de una ráfaga. `pactl subscribe`
/// imprime una línea por evento; interesan los `sink` (cambió el volumen o el
/// mute de una salida) y el `server` (cambió la salida por defecto).
///
/// Ojo con el filtro: `on sink #N` y `on sink-input #N` comparten el prefijo, y
/// los `sink-input` son los streams de las apps (eso lo relee el panel de volumen
/// cada 2 s, no acá: hacerlo en cada evento lo volvería lento al arrastrar).
pub fn spawn_volume_watcher(
    tx: Sender<IpcMessage>,
    conn: Connection,
    qh: QueueHandle<crate::app::App>,
) {
    std::thread::spawn(move || {
        loop {
            let mut cmd = std::process::Command::new("pactl");
            cmd.arg("subscribe")
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            let Ok(mut child) = die_with_parent(&mut cmd).spawn() else {
                // ----- sin pactl (o sin audio) queda el sondeo de 2 s -----
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            };
            if let Some(stdout) = child.stdout.take() {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if !(line.contains("on sink #") || line.contains("on server")) {
                        continue;
                    }
                    if tx.send(IpcMessage::VolumeChanged).is_ok() {
                        conn.display().sync(&qh, ());
                        let _ = conn.flush();
                    }
                }
            }
            let _ = child.wait();
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    });
}

pub fn spawn_bluetooth_watcher(
    tx: Sender<IpcMessage>,
    conn: Connection,
    qh: QueueHandle<crate::app::App>,
) {
    std::thread::spawn(move || {
        let _ = watch_bluetooth(&tx, &conn, &qh);
    });
}

fn watch_bluetooth(
    tx: &Sender<IpcMessage>,
    conn: &Connection,
    qh: &QueueHandle<crate::app::App>,
) -> zbus::Result<()> {
    let system = zbus::blocking::Connection::system()?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .path_namespace("/org/bluez")?
        .build();
    let iter = zbus::blocking::MessageIterator::for_match_rule(rule, &system, Some(8))?;
    for msg in iter.flatten() {
        type Changed = (
            String,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
            Vec<String>,
        );
        let Ok((_iface, changed, invalidated)) = msg.body().deserialize::<Changed>() else {
            continue;
        };
        // ----- ignore unrelated churn -----
        let relevant = |k: &str| k == "Powered" || k == "Connected";
        if !changed.keys().any(|k| relevant(k)) && !invalidated.iter().any(|k| relevant(k)) {
            continue;
        }
        if tx.send(IpcMessage::BluetoothChanged).is_ok() {
            conn.display().sync(qh, ());
            let _ = conn.flush();
        }
    }
    Ok(())
}

pub fn spawn_workspace_watcher(
    tx: Sender<IpcMessage>,
    conn: Connection,
    qh: QueueHandle<crate::app::App>,
) {
    match crate::compositor::Compositor::detect() {
        crate::compositor::Compositor::Niri => spawn_niri_workspace_watcher(tx, conn, qh),
        _ => spawn_hypr_workspace_watcher(tx, conn, qh),
    }
}

fn spawn_niri_workspace_watcher(
    tx: Sender<IpcMessage>,
    conn: Connection,
    qh: QueueHandle<crate::app::App>,
) {
    std::thread::spawn(move || {
        loop {
            let mut cmd = std::process::Command::new("niri");
            cmd.args(["msg", "--json", "event-stream"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null());
            let Ok(mut child) = cmd.spawn() else {
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            };
            if let Some(stdout) = child.stdout.take() {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    // ponytail: filtrado por substring, sin parsear JSON por evento.
                    // `Window*` incluye abrir/cerrar ventana: hace falta para que el
                    // workspace vacío (dock fijo) se reevalúe al abrir la primera.
                    if (line.contains("Workspace")
                        || line.contains("workspace")
                        || line.contains("Window"))
                        && tx.send(IpcMessage::WorkspacesChanged).is_ok()
                    {
                        conn.display().sync(&qh, ());
                        let _ = conn.flush();
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    });
}

fn spawn_hypr_workspace_watcher(
    tx: Sender<IpcMessage>,
    conn: Connection,
    qh: QueueHandle<crate::app::App>,
) {
    std::thread::spawn(move || {
        loop {
            let Ok(sig) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE") else {
                return;
            };
            let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
            let path = std::path::PathBuf::from(runtime_dir)
                .join("hypr")
                .join(&sig)
                .join(".socket2.sock");
            let Ok(stream) = UnixStream::connect(&path) else {
                std::thread::sleep(std::time::Duration::from_secs(3));
                continue;
            };
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if (line.starts_with("workspace")
                    || line.starts_with("focusedmon")
                    || line.starts_with("openwindow")
                    || line.starts_with("closewindow"))
                    && tx.send(IpcMessage::WorkspacesChanged).is_ok()
                {
                    conn.display().sync(&qh, ());
                    let _ = conn.flush();
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

#[cfg(test)]
mod ipc_timeout_tests {
    use super::*;

    #[test]
    fn el_timeout_del_cliente_es_corto() {
        // ----- el listener es UN solo hilo con un `read` bloqueante: si este
        // número se agranda, un cliente que conecta y no escribe vuelve a dejar
        // todo el IPC sin responder durante ese rato. -----
        assert!(
            IPC_READ_TIMEOUT <= Duration::from_millis(200),
            "el timeout de lectura del IPC quedó en {IPC_READ_TIMEOUT:?}"
        );
    }

    #[test]
    fn un_cliente_que_no_escribe_no_cuelga_el_read() {
        // ----- el mecanismo del que depende b3: con el timeout puesto, el `read`
        // de `handle_client` vuelve en vez de esperar para siempre. -----
        let (mut servidor, _cliente_que_nunca_escribe) =
            UnixStream::pair().expect("socketpair AF_UNIX");
        preparar_cliente(&servidor);
        let mut buf = [0u8; 1024];
        let start = std::time::Instant::now();
        let res = servidor.read(&mut buf);
        assert!(
            res.is_err(),
            "sin datos el read tiene que volver con error, no quedarse esperando"
        );
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "tardó {:?} en volver",
            start.elapsed()
        );
    }
}
