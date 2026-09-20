use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::time::Duration;
use wayland_client::{Connection, QueueHandle};

// ----- sin derives: no se clona ni se imprime, y `DeferredWidgets` trae tipos
// (`MediaInfo`, `BluetoothInfo`) que tampoco. -----
pub enum IpcMessage {
    ToggleSearch,
    OsdVolume,
    OsdBrightness,
    MediaChanged,
    VolumeChanged,
    BatteryChanged,
    BluetoothChanged,
    /// `WorkspacesChanged` de niri: el evento **ya trae la lista entera**, así que el
    /// dock no tiene que lanzar `niri msg --json workspaces` (que era el costo del
    /// hallazgo B4). Se filtra por el output anclado y se ordena por id, igual que la
    /// lectura por CLI (`workspaces_de_json` es la única cuenta de ese mapeo).
    WorkspacesList(Box<[crate::widgets::WorkspaceInfo]>),
    /// Algo cambió en los workspaces o en las ventanas y hay que **releer** (esos
    /// eventos no traen la lista). Va con throttle en `App::refresh_workspaces`.
    WorkspacesChanged,
    /// Lo que el arranque leyó en un hilo aparte (batería, media, bluetooth,
    /// volumen): son las lecturas caras y el primer frame no las espera. Trae el
    /// dato ya leído a propósito — los `refresh_*` releen, y acá el punto es no
    /// volver a esperar.
    DeferredWidgets(Box<crate::widgets::DeferredWidgets>),
    /// El layout de teclado cambió (niri lo manda como `KeyboardLayoutsChanged`):
    /// llega por el event-stream, no por el tick de 2 s.
    KbdLayoutChanged,
    /// El Overview de niri se abrió (`true`) o se cerró (`false`). Niri lo manda
    /// también con el estado inicial al conectar al event-stream, así que el dock
    /// arranca sabiendo si estaba abierto.
    OverviewChanged(bool),
    ToggleWallpaper,
    ToggleClipboard,
    ToggleNotifications,
    ScreenshotFull,
    ScreenshotRegion,
    ToggleDockMenu,
    TestNotification,
    Notify(String, String),
    /// El menú de un item del tray ya resuelto. NO viene del socket: lo manda el hilo
    /// de `tray::spawn_menu_worker`, porque `GetLayout` es bloqueante y no puede
    /// correr en el hilo que dibuja (A3 de AUDIT.md).
    TrayMenuReady(Box<crate::tray::MenuResult>),
}

const NOTIFY_SEP: char = '\u{1f}';

/// Tope de un comando de IPC. 1024 quedaba corto para un `notify` con cuerpo largo: se
/// truncaba en silencio y llegaba a medias (AUDIT.md C6). 16 KB es holgado para un
/// título + cuerpo y sigue siendo un tope duro contra un cliente que manda basura.
const IPC_MAX_BYTES: usize = 16 * 1024;

fn socket_path(profile: &str) -> std::path::PathBuf {
    // ----- con `XDG_RUNTIME_DIR` (que en una sesión Wayland siempre está) el socket ya
    // es del usuario. Si falta NO se cae a `/tmp` pelado: un socket con nombre
    // predecible ahí lo puede abrir cualquier usuario de la máquina y mandarle comandos
    // al dock (AUDIT.md B2). Se usa un subdirectorio por uid con 0700. -----
    let dir = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            use std::os::unix::fs::PermissionsExt;
            let d = std::env::temp_dir().join(format!("dockyrs-{}", unsafe { libc::getuid() }));
            if let Err(err) = std::fs::create_dir_all(&d) {
                log::warn!("no pude crear {d:?} para el socket: {err}");
            } else {
                let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
            }
            d
        });
    dir.join(if profile.is_empty() {
        "dockyrs.sock".to_string()
    } else {
        format!("dockyrs-{profile}.sock")
    })
}

/// Escribe el comando en el socket del dock. Si no se puede (el dock no corre o su
/// IPC está trabada) lo dice por stderr: un `--toggle-search` que falla en silencio
/// se ve como "el keybind no anda" y se pierde una tarde buscando en el lado equivocado.
pub fn send_message(text: &str, profile: &str) {
    match UnixStream::connect(socket_path(profile)) {
        Ok(mut stream) => {
            if let Err(err) = stream.write_all(text.as_bytes()) {
                log::warn!("no pude mandarle {text:?} al dock: {err}");
            }
        }
        Err(err) => log::warn!("no pude conectar con el dock (¿está corriendo?): {err}"),
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
        // ----- UN HILO POR CLIENTE. Antes `handle_client` corría en línea en este
        // mismo hilo: un cliente queda mudo (o el read se cuelga por lo que sea) y el
        // accept no vuelve a correr, así que la cola del socket se llena y TODOS los
        // `--toggle-*` / notify empiezan a dar `ConnectionRefused` — con la app viva,
        // el socket en LISTENING y cero logs. Visto en vivo. Con un hilo por cliente,
        // uno trabado cuesta un hilo y nada más. -----
        for stream in listener.incoming().flatten() {
            preparar_cliente(&stream);
            let (tx, conn, qh) = (tx.clone(), conn.clone(), qh.clone());
            std::thread::spawn(move || handle_client(stream, &tx, &conn, &qh));
        }
    });
}

/// Comando de un cliente: el texto (recortado al tope) y si hubo que recortarlo.
/// Separado de `handle_client` para poder probarlo con un `UnixStream::pair()` real,
/// sin fabricar un `App` ni una conexión Wayland.
fn leer_comando(stream: &UnixStream) -> Option<(String, bool)> {
    let mut buf = Vec::with_capacity(256);
    let mut rd = stream.take(IPC_MAX_BYTES as u64 + 1);
    rd.read_to_end(&mut buf).ok()?;
    if buf.is_empty() {
        return None;
    }
    let truncado = buf.len() > IPC_MAX_BYTES;
    if truncado {
        buf.truncate(IPC_MAX_BYTES);
        log::warn!("comando de IPC de más de {IPC_MAX_BYTES} bytes: lo corto");
    }
    Some((String::from_utf8_lossy(&buf).to_string(), truncado))
}

fn handle_client(
    stream: UnixStream,
    tx: &Sender<IpcMessage>,
    conn: &Connection,
    qh: &QueueHandle<crate::app::App>,
) {
    let Some((text, _)) = leer_comando(&stream) else {
        return;
    };
    let msg = match text.as_ref() {
        "toggle-search" => Some(IpcMessage::ToggleSearch),
        "osd-volume" => Some(IpcMessage::OsdVolume),
        "osd-brightness" => Some(IpcMessage::OsdBrightness),
        "toggle-wallpaper" => Some(IpcMessage::ToggleWallpaper),
        "toggle-clipboard" => Some(IpcMessage::ToggleClipboard),
        "toggle-notifications" => Some(IpcMessage::ToggleNotifications),
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
    wanted: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    std::thread::spawn(move || {
        loop {
            // ----- sin un widget Media colocado, `playerctl --follow` es un hijo
            // residente de ~7 MB que no dibuja nada: se consulta el flag ANTES de
            // crearlo. Ojo: quitar el widget con el proceso andando no lo mata hasta
            // que salga o el dock reinicie; colocarlo arranca dentro de los 3 s. -----
            if !wanted.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_secs(3));
                continue;
            }
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
                    // ----- la caratula remota se baja ACÁ, en el hilo del watcher: es
                    // el único lugar donde `curl` puede esperar sin que el dock deje
                    // de dibujar. El título ya se avisó arriba; si apareció una
                    // carátula nueva, un segundo aviso la hace entrar sin que el hilo
                    // que dibuja haya tocado la red (A4 de AUDIT.md). -----
                    if crate::widgets::warm_media_art() && tx.send(IpcMessage::MediaChanged).is_ok()
                    {
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
            // ----- hablar el socket de niri DIRECTO en vez de lanzar el CLI
            // `niri msg --json event-stream`: ese hijo residente costaba 15 MB
            // medidos (el ítem más grande del stack) y el protocolo es el mismo
            // (pedido JSON + `\n`, respuestas JSON por línea; verificado contra
            // niri 26.04). Si no hay `NIRI_SOCKET` o el connect falla, se cae al CLI
            // de siempre, así que el peor caso es el comportamiento viejo. -----
            let stream = std::env::var("NIRI_SOCKET")
                .ok()
                .and_then(|path| UnixStream::connect(path).ok());
            match stream {
                Some(stream) => pump_niri_events(&stream, &tx, &conn, &qh),
                None => pump_niri_cli(&tx, &conn, &qh),
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    });
}

/// Qué mensaje pide una línea del event-stream de niri (`None` = no cambia nada del
/// dock). Mismo filtro por substring que el CLI: sin parsear JSON por evento;
/// `Window*` incluye abrir/cerrar ventana, que hace falta para que el workspace vacío
/// (dock fijo) se reevalúe al abrir la primera.
///
/// `KeyboardLayoutsChanged` viaja por el MISMO stream, así que el widget se actualiza
/// al instante y el tick de 2 s ya no tiene que lanzar `niri msg -j keyboard-layouts`
/// (~14 ms medidos por spawn).
fn niri_mensaje(line: &str) -> Option<IpcMessage> {
    // ----- el filtro es por NOMBRE de evento, no por substring. Con "contiene
    // Window/Workspace" pasaban cosas que el dock no dibuja: por UNA ventana que abre y
    // cierra niri manda 10 eventos (medido) y `WindowFocusChanged`,
    // `WindowFocusTimestampChanged` y `WindowLayoutsChanged` no cambian ni el workspace
    // activo, ni cual está vacío, ni la urgencia: costaban 3 de los 10 `niri msg --json
    // workspaces` de cada ciclo. -----
    let ev: serde_json::Value = serde_json::from_str(line).ok()?;
    let (nombre, payload) = ev.as_object()?.iter().next()?;
    match nombre.as_str() {
        "KeyboardLayoutsChanged" | "KeyboardLayoutSwitched" => Some(IpcMessage::KbdLayoutChanged),
        // ----- se busca el booleano DENTRO del evento, no en la línea suelta -----
        "OverviewOpenedOrClosed" => Some(IpcMessage::OverviewChanged(
            payload
                .get("is_open")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        )),
        // ----- trae la lista: no se lanza nada -----
        "WorkspacesChanged" => {
            let vacio = Vec::new();
            let list = payload
                .get("workspaces")
                .and_then(|v| v.as_array())
                .unwrap_or(&vacio);
            Some(IpcMessage::WorkspacesList(
                crate::widgets::workspaces_de_json(list).into_boxed_slice(),
            ))
        }
        // ----- cambian el estado que el dock dibuja (punto activo, workspace vacío,
        // urgencia) pero NO traen la lista completa: hay que releer -----
        "WorkspaceActivated"
        | "WorkspaceActiveWindowChanged"
        | "WorkspaceUrgencyChanged"
        | "WindowsChanged"
        | "WindowOpenedOrChanged"
        | "WindowClosed"
        | "WindowUrgencyChanged" => Some(IpcMessage::WorkspacesChanged),
        // ----- todo lo demás se ignora: `WindowLayoutsChanged`, `WindowFocusChanged`,
        // `WindowFocusTimestampChanged`, `ConfigLoaded`, `CastsChanged`, el
        // `{"Ok":"Handled"}` del acuse de recibo, … -----
        _ => None,
    }
}

fn pump_niri_events(
    stream: &UnixStream,
    tx: &Sender<IpcMessage>,
    conn: &Connection,
    qh: &QueueHandle<crate::app::App>,
) {
    let mut stream = stream;
    if stream.write_all(b"\"EventStream\"\n").is_err() {
        return;
    }
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        if let Some(msg) = niri_mensaje(&line)
            && tx.send(msg).is_ok()
        {
            conn.display().sync(qh, ());
            let _ = conn.flush();
        }
    }
}

fn pump_niri_cli(tx: &Sender<IpcMessage>, conn: &Connection, qh: &QueueHandle<crate::app::App>) {
    let mut cmd = std::process::Command::new("niri");
    cmd.args(["msg", "--json", "event-stream"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return;
    };
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(msg) = niri_mensaje(&line)
                && tx.send(msg).is_ok()
            {
                conn.display().sync(qh, ());
                let _ = conn.flush();
            }
        }
    }
    let _ = child.wait();
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
mod niri_event_tests {
    use super::{IpcMessage, niri_mensaje};

    /// El mapa línea → mensaje del event-stream. `Workspace`/`Window` mueven los
    /// workspaces (sin `Window*` el workspace vacío no se reevalúa y el dock dejaría
    /// de fijarse), `KeyboardLayout` el layout y `OverviewOpenedOrClosed` el estado
    /// del Overview (que fija el dock). Tiene que quedar afuera lo que viaja
    /// por el mismo stream sin cambiar nada del dock — incluido el `{"Ok":...}` con el
    /// que niri acusa recibo del pedido.
    #[test]
    fn el_filtro_del_event_stream() {
        // ----- estos NO traen la lista, así que piden relectura -----
        for linea in [
            r#"{"WindowsChanged":{}}"#,
            r#"{"WindowOpenedOrChanged":{}}"#,
            r#"{"WindowClosed":{"id":4}}"#,
            r#"{"WorkspaceActivated":{"id":1,"focused":true}}"#,
            r#"{"WorkspaceActiveWindowChanged":{"workspace_id":1,"active_window_id":null}}"#,
            r#"{"WorkspaceUrgencyChanged":{"id":3,"urgent":true}}"#,
            r#"{"WindowUrgencyChanged":{"id":4,"urgent":true}}"#,
        ] {
            assert!(
                matches!(niri_mensaje(linea), Some(IpcMessage::WorkspacesChanged)),
                "{linea}"
            );
        }
        // ----- y estos no cambian NADA de lo que el dock dibuja: con el filtro viejo
        // (por substring "Window") costaban un `niri msg` cada uno, y por cada ventana
        // que abre y cierra niri manda 10 eventos (medido) con 3 de estos adentro -----
        for linea in [
            r#"{"WindowFocusChanged":{"id":4}}"#,
            r#"{"WindowFocusTimestampChanged":{"id":4,"timestamp":123}}"#,
            r#"{"WindowLayoutsChanged":{"changes":[]}}"#,
            r#"{"CastsChanged":{"casts":[]}}"#,
            r#"{"ConfigLoaded":{"path":"/tmp/x"}}"#,
        ] {
            assert!(
                niri_mensaje(linea).is_none(),
                "{linea} no tendría que pedir nada"
            );
        }
        // ----- `WorkspacesChanged` SÍ trae la lista entera: se usa el payload y no se
        // lanza `niri msg --json workspaces` (era el costo de B4). La línea es una real
        // capturada del event-stream de niri 26.04. -----
        let payload = r#"{"WorkspacesChanged":{"workspaces": [{"id": 3, "idx": 2, "name": null, "output": "eDP-1", "is_urgent": false, "is_active": false, "is_focused": false, "active_window_id": null}, {"id": 1, "idx": 1, "name": null, "output": "eDP-1", "is_urgent": true, "is_active": true, "is_focused": true, "active_window_id": 4}]}}"#;
        let Some(IpcMessage::WorkspacesList(ws)) = niri_mensaje(payload) else {
            panic!("el payload tiene que dar WorkspacesList");
        };
        assert_eq!(ws.len(), 2, "los dos del payload");
        assert_eq!(ws[0].id, 1, "y ordenados por id");
        assert!(ws[0].active, "el activo sale de is_active");
        assert!(ws[0].urgent, "y la urgencia de is_urgent");
        assert_eq!(ws[0].output, "eDP-1");
        assert_eq!(ws[1].id, 2);
        assert!(!ws[1].active && !ws[1].urgent);
        assert!(ws[1].empty, "sin active_window_id = vacío");
        assert!(!ws[0].empty, "con active_window_id = con ventanas");
        // ----- un `WorkspacesChanged` sin la lista (versión vieja de niri) no explota:
        // da lista vacía, y el tick siguiente la corrige -----
        let Some(IpcMessage::WorkspacesList(vacia)) = niri_mensaje(r#"{"WorkspacesChanged":{}}"#)
        else {
            panic!("sin lista tiene que dar lista vacía, no obras");
        };
        assert!(vacia.is_empty());
        for linea in [
            r#"{"KeyboardLayoutsChanged":{}}"#,
            r#"{"KeyboardLayoutSwitched":{"idx":1}}"#,
        ] {
            assert!(
                matches!(niri_mensaje(linea), Some(IpcMessage::KbdLayoutChanged)),
                "{linea}"
            );
        }
        for (linea, abierto) in [
            (r#"{"OverviewOpenedOrClosed":{"is_open":true}}"#, true),
            (r#"{"OverviewOpenedOrClosed":{"is_open":false}}"#, false),
        ] {
            assert!(
                matches!(niri_mensaje(linea), Some(IpcMessage::OverviewChanged(v)) if v == abierto),
                "{linea}"
            );
        }
        assert!(niri_mensaje(r#"{"Ok":"Handled"}"#).is_none());
    }
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

#[cfg(test)]
mod ipc_socket_y_comando_tests {
    use super::*;

    /// B2: el socket vive en `XDG_RUNTIME_DIR` y, si falta, en un subdirectorio por uid
    /// (0700), NO en `/tmp` pelado. Y el nombre lleva el perfil.
    #[test]
    fn el_socket_no_cae_en_tmp_pelado_y_lleva_el_perfil() {
        let con_perfil = socket_path("dp");
        assert_eq!(
            con_perfil.file_name().unwrap().to_string_lossy(),
            "dockyrs-dp.sock"
        );
        assert_eq!(
            socket_path("").file_name().unwrap().to_string_lossy(),
            "dockyrs.sock"
        );
        if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
            assert_eq!(con_perfil.parent().unwrap(), std::path::Path::new(&dir));
        } else {
            let padre = con_perfil.parent().unwrap().to_string_lossy().to_string();
            assert!(
                padre.contains(&format!("dockyrs-{}", unsafe { libc::getuid() })),
                "sin XDG_RUNTIME_DIR tiene que ser un dir por uid: {padre}"
            );
            assert!(!padre.ends_with("/tmp"), "no /tmp pelado: {padre}");
        }
    }

    /// C6: un `notify` largo ya no se corta en 1024 bytes.
    #[test]
    fn un_comando_largo_se_lee_entero() {
        let (mut cliente, servidor) = UnixStream::pair().expect("socketpair AF_UNIX");
        let cuerpo = "x".repeat(4000);
        let comando = format!("notify{NOTIFY_SEP}Título{NOTIFY_SEP}{cuerpo}");
        cliente.write_all(comando.as_bytes()).unwrap();
        drop(cliente);
        let (leido, truncado) = leer_comando(&servidor).expect("comando");
        assert!(!truncado, "4000 bytes tienen que entrar");
        assert_eq!(leido, comando, "llega entero, con el cuerpo completo");
    }

    /// Y el tope sigue existiendo: un cliente que manda basura sin fin se corta (con
    /// aviso), no se come la memoria del dock.
    #[test]
    fn lo_que_pasa_el_tope_se_corta() {
        let (mut cliente, servidor) = UnixStream::pair().expect("socketpair AF_UNIX");
        let basura = "y".repeat(IPC_MAX_BYTES + 500);
        cliente.write_all(basura.as_bytes()).unwrap();
        drop(cliente);
        let (leido, truncado) = leer_comando(&servidor).expect("comando");
        assert!(truncado, "tiene que avisar que cortó");
        assert_eq!(leido.len(), IPC_MAX_BYTES);
    }

    /// Un cliente que conecta y no escribe no produce comando (y no cuelga: para eso
    /// está el timeout de `preparar_cliente`).
    #[test]
    fn un_cliente_mudo_no_produce_comando() {
        let (cliente, servidor) = UnixStream::pair().expect("socketpair AF_UNIX");
        drop(cliente);
        assert!(leer_comando(&servidor).is_none());
    }
}
