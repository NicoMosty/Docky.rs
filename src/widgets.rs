use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn run_with_timeout(
    mut cmd: std::process::Command,
    timeout: Duration,
) -> Option<std::process::Output> {
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        if let Some(_status) = child.try_wait().ok()? {
            return child.wait_with_output().ok();
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Ritmo mínimo de un widget con script. El tick de sistema de dos segundos es
/// el que despierta el chequeo, así que un intervalo menor significa "en cada
/// tick de sistema" en vez de arrancar otro temporizador con el dock oculto.
pub fn custom_poll_interval(source: &crate::config::CustomWidgetSource) -> Duration {
    Duration::from_millis(source.interval_ms.max(2_000))
}

/// Techo por ejecución de un widget con script. El bucle ya confía en procesos
/// cortos; este techo es más estricto para que un script lento no domine un
/// refresco de sistema.
pub fn custom_command_timeout(source: &crate::config::CustomWidgetSource) -> Duration {
    Duration::from_millis(source.timeout_ms.clamp(100, 500))
}

/// Presupuesto de texto de un widget personalizado, contado en caracteres
/// Unicode (no en bytes) para que `max_chars` no parta un carácter multibyte.
pub fn custom_text_limit(source: &crate::config::CustomWidgetSource) -> usize {
    source.max_chars.clamp(1, 128)
}

/// Una fuente apagada o sin comando no tiene salida visible, aunque la caché
/// todavía guarde una línea vieja. Se usa en la medición, el dibujo y el
/// refresco para que los tres vean lo mismo.
fn custom_source_active(source: &crate::config::CustomWidgetSource) -> bool {
    source.enabled && !source.command.trim().is_empty()
}

/// Deja pasar una ejecución sólo si la fuente está activa y ya venció su
/// intervalo. Quien llama también tiene que mirar colocación y visibilidad del
/// dock; este helper no conoce ninguna de las dos.
pub fn should_poll_custom(
    source: &crate::config::CustomWidgetSource,
    last_poll: Option<Instant>,
    now: Instant,
) -> bool {
    if !custom_source_active(source) {
        return false;
    }
    last_poll.is_none_or(|last| now.duration_since(last) >= custom_poll_interval(source))
}

/// Corre un comando de shell configurado por el usuario y normaliza su primera
/// línea de salida. Lo que no se puede volver texto visible (fallo de arranque,
/// subproceso matado, línea vacía, estado distinto de cero) conserva el valor
/// anterior en caché.
pub fn read_custom_text(source: &crate::config::CustomWidgetSource) -> Option<String> {
    let command = source.command.trim();
    if command.is_empty() {
        return None;
    }
    let mut shell = std::process::Command::new("sh");
    shell.arg("-c").arg(command);
    let output = run_with_timeout(shell, custom_command_timeout(source))?;
    if !output.status.success() {
        return None;
    }
    clean_custom_text(
        &String::from_utf8_lossy(&output.stdout),
        custom_text_limit(source),
    )
}

/// Normaliza una línea de salida igual para medir y dibujar. El mismo texto
/// tiene que manejar las dos para que la pastilla nunca supere su ancho
/// reservado.
pub fn clean_custom_text(output: &str, max_chars: usize) -> Option<String> {
    let line = output.lines().next().unwrap_or_default().trim();
    if line.is_empty() {
        return None;
    }
    let mut text = String::with_capacity(line.len().min(256));
    for c in line.chars() {
        if c == '\t' {
            text.push(' ');
        } else if !c.is_control() {
            text.push(c);
        }
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let limit = max_chars.max(1);
    if text.chars().count() > limit {
        Some(text.chars().take(limit).collect())
    } else {
        Some(text)
    }
}

/// Texto en caché por payload del widget. `None` significa que el widget no
/// tiene salida visible ahora, incluso cuando su fuente quedó fuera de rango,
/// está apagada o no trae comando.
pub fn custom_text_for<'a>(
    settings: &crate::config::DockSettings,
    snapshot: &'a WidgetSnapshot,
    kind: crate::config::WidgetKind,
) -> Option<&'a str> {
    let crate::config::WidgetKind::Custom(index) = kind else {
        return None;
    };
    let source = settings.custom_widgets.get(usize::from(index))?;
    if !custom_source_active(source) {
        return None;
    }
    snapshot
        .custom_texts
        .get(usize::from(index))
        .and_then(|text| text.as_deref())
}

pub struct MediaInfo {
    pub title: String,
    pub playing: bool,
    pub art_path: Option<String>,
}

pub struct BluetoothInfo {
    pub powered: bool,
    pub connected: Option<String>,
}

pub struct WorkspaceInfo {
    pub id: i32,
    pub active: bool,
    /// Sin ventanas: el autohide deja el dock fijo en pantalla.
    pub empty: bool,
    pub output: String,
}

// ----- fijado una vez al arrancar desde --output; filtra workspaces por monitor -----
static PINNED_OUTPUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn set_pinned_output(name: &str) {
    if !name.is_empty() {
        let _ = PINNED_OUTPUT.set(name.to_string());
    }
}

fn pinned_output() -> Option<&'static str> {
    PINNED_OUTPUT.get().map(|s| s.as_str())
}

pub struct NetworkInfo {
    pub label: String,
    pub online: bool,
}

pub struct KbLayout {
    pub short: String,
}

/// Estado de carga de la bateria.
/// `Conserving` = enchufado pero sin cargar (sysfs lo reporta como
/// "Not charging": modo conservacion / tope de carga).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryState {
    Discharging,
    Charging,
    Conserving,
}

pub struct WidgetSnapshot {
    pub time: String,
    pub date: String,
    pub battery: Option<(u8, BatteryState)>,
    pub media: Option<MediaInfo>,
    pub bluetooth: Option<BluetoothInfo>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub cpu: Option<u8>,
    pub ram: Option<u8>,
    pub ram_gb: Option<(f32, f32)>,
    pub volume: Option<(u8, bool)>,
    pub network: NetworkInfo,
    pub kblayout: KbLayout,
    /// Un texto opcional por widget personalizado con script, alineado por
    /// posición con `DockSettings.custom_widgets`. Guardar sólo la línea ya
    /// renderizada (no la salida cruda del proceso) acota la memoria al
    /// presupuesto de texto configurado.
    pub custom_texts: Vec<Option<String>>,
    /// Último intento de sondeo por widget personalizado. Anotar el intento,
    /// no sólo el éxito, hace que un comando que falla siempre espere su
    /// intervalo antes de reintentar.
    pub custom_last_polls: Vec<Option<Instant>>,
}

impl WidgetSnapshot {
    /// Formato de `strftime` del reloj: 24 h si el usuario lo pidió para el
    /// widget `Clock`, 12 h con AM/PM si no.
    pub fn clock_format(settings: &crate::config::DockSettings) -> &'static str {
        if settings.widget_clock_24h(crate::config::WidgetKind::Clock) {
            "%H:%M"
        } else {
            "%I:%M %p"
        }
    }

    pub fn refresh(settings: &crate::config::DockSettings) -> Self {
        Self {
            time: strftime_now(Self::clock_format(settings)).unwrap_or_default(),
            date: clock_date_now().unwrap_or_default(),
            battery: read_battery(),
            media: read_media(),
            bluetooth: read_bluetooth(),
            workspaces: read_workspaces(),
            cpu: read_cpu(),
            ram: read_ram().map(|(pct, _)| pct),
            ram_gb: read_ram().map(|(_, gb)| gb),
            volume: read_volume(),
            network: read_network(),
            kblayout: read_kblayout(),
            custom_texts: Vec::new(),
            custom_last_polls: Vec::new(),
        }
    }

    pub fn refresh_cpu_ram(&mut self) {
        self.cpu = read_cpu();
        if let Some((pct, gb)) = read_ram() {
            self.ram = Some(pct);
            self.ram_gb = Some(gb);
        }
    }

    /// true si cambió algo visible (requiere relayout). Ya NO relee el layout de
    /// teclado: ese llega por el event-stream (`refresh_kblayout`).
    pub fn refresh_sys(&mut self) -> bool {
        let volume = self.refresh_volume();
        let network = read_network();
        let changed = volume || network.label != self.network.label;
        self.network = network;
        changed
    }

    /// Sólo el layout de teclado. Lo dispara el event-stream de niri
    /// (`KeyboardLayoutsChanged`), no el tick: el widget se actualiza al instante y el
    /// tick de 2 s se ahorra el `niri msg -j keyboard-layouts` (~14 ms por spawn,
    /// medidos). Ver `ipc::niri_mensaje`.
    pub fn refresh_kblayout(&mut self) -> bool {
        let kblayout = read_kblayout();
        let changed = kblayout.short != self.kblayout.short;
        self.kblayout = kblayout;
        changed
    }

    /// Sólo el volumen. Es lo único que puede cambiar en dos caminos que corren
    /// muy seguido: el evento de PipeWire (las teclas de volumen del sistema
    /// corren `wpctl` por fuera del dock) y la rueda sobre el widget, que viene en
    /// ráfaga. `refresh_sys` de paso relee la red (`iw`, ~2 ms medidos) que no tiene
    /// nada que ver. Una sola definición de la comparación: `refresh_sys` la usa para
    /// no duplicarla.
    pub fn refresh_volume(&mut self) -> bool {
        let volume = read_volume();
        let changed = volume != self.volume;
        self.volume = volume;
        changed
    }

    pub fn refresh_workspaces(&mut self) {
        self.workspaces = read_workspaces();
    }

    pub fn refresh_clock(&mut self, settings: &crate::config::DockSettings) -> bool {
        let time = strftime_now(Self::clock_format(settings)).unwrap_or_default();
        let date = clock_date_now().unwrap_or_default();
        let changed = time != self.time || date != self.date;
        self.time = time;
        self.date = date;
        changed
    }

    pub fn refresh_media(&mut self) {
        self.media = read_media();
    }

    pub fn refresh_bluetooth(&mut self) {
        self.bluetooth = read_bluetooth();
    }

    pub fn refresh_battery(&mut self) -> bool {
        let battery = read_battery();
        let changed = battery != self.battery;
        self.battery = battery;
        changed
    }

    /// `true` si cambió algún texto visible (requiere relayout). Sincroniza las
    /// cachés con la lista de fuentes y corre sólo las que están activas y
    /// vencidas. Un comando que falla conserva su texto anterior pero anota el
    /// intento, para que no reintente en cada tick.
    pub fn refresh_custom(
        &mut self,
        sources: &[crate::config::CustomWidgetSource],
        now: Instant,
    ) -> bool {
        self.custom_texts.resize(sources.len(), None);
        self.custom_last_polls.resize(sources.len(), None);
        let mut changed = false;
        for (index, source) in sources.iter().enumerate() {
            if !custom_source_active(source) {
                if self.custom_texts[index].take().is_some() {
                    changed = true;
                }
                continue;
            }
            if !should_poll_custom(source, self.custom_last_polls[index], now) {
                continue;
            }
            self.custom_last_polls[index] = Some(now);
            if let Some(text) = read_custom_text(source)
                && self.custom_texts[index].as_deref() != Some(text.as_str())
            {
                self.custom_texts[index] = Some(text);
                changed = true;
            }
        }
        changed
    }
}

// ----- none first call -----
fn read_cpu() -> Option<u8> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let fields: Vec<u64> = stat
        .lines()
        .next()?
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    if fields.len() < 4 {
        return None;
    }
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    let total: u64 = fields.iter().sum();

    static LAST: std::sync::OnceLock<std::sync::Mutex<Option<(u64, u64)>>> =
        std::sync::OnceLock::new();
    let mut last = LAST
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap();
    let pct = last.and_then(|(prev_idle, prev_total)| {
        let total_delta = total.checked_sub(prev_total)?;
        if total_delta == 0 {
            return None;
        }
        let idle_delta = idle.saturating_sub(prev_idle);
        Some((100.0 * (1.0 - idle_delta as f64 / total_delta as f64)).clamp(0.0, 100.0) as u8)
    });
    *last = Some((idle, total));
    pct
}

fn read_ram() -> Option<(u8, (f32, f32))> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total = None;
    let mut avail = None;
    for line in meminfo.lines() {
        let value = |l: &str| l.split_whitespace().next()?.parse::<u64>().ok();
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total = value(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail = value(rest);
        }
    }
    let (total, avail) = (total?, avail?);
    if total == 0 {
        return None;
    }
    let pct = (100.0 * (1.0 - avail as f64 / total as f64)).clamp(0.0, 100.0) as u8;
    let gb = |kb: u64| kb as f32 / 1024.0 / 1024.0;
    Some((pct, (gb(total - avail), gb(total))))
}

fn read_network() -> NetworkInfo {
    if let Some(info) = wifi_info() {
        return info;
    }
    if let Some(label) = ethernet_ip() {
        return NetworkInfo {
            label,
            online: true,
        };
    }
    NetworkInfo {
        label: "Disconnected".to_string(),
        online: false,
    }
}

fn wifi_iface() -> Option<String> {
    let wireless = std::fs::read_to_string("/proc/net/wireless").ok()?;
    wireless
        .lines()
        .nth(2)?
        .split(':')
        .next()
        .map(|s| s.trim().to_string())
}

fn wifi_info() -> Option<NetworkInfo> {
    let iface = wifi_iface()?;
    let mut cmd = std::process::Command::new("iw");
    cmd.args(["dev", &iface, "link"]);
    let out = run_with_timeout(cmd, Duration::from_millis(500))?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if text.contains("Not connected") {
        return None;
    }
    let ssid = text.lines().find_map(|l| l.trim().strip_prefix("SSID: "))?;
    let dbm: i32 = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("signal: "))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pct = ((dbm + 100) * 2).clamp(0, 100);
    let mut label = format!("{ssid} ({pct}%)");
    if label.chars().count() > 24 {
        label = format!("{}…", label.chars().take(23).collect::<String>());
    }
    Some(NetworkInfo {
        label,
        online: true,
    })
}

fn ethernet_ip() -> Option<String> {
    let mut cmd = std::process::Command::new("ip");
    cmd.args(["-o", "-4", "addr", "show", "up", "scope", "global"]);
    let out = run_with_timeout(cmd, Duration::from_millis(500))?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?;
    let addr = line.split_whitespace().find_map(|t| {
        if t.contains('/') && t.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            Some(t)
        } else {
            None
        }
    })?;
    Some(addr.to_string())
}

fn read_kblayout() -> KbLayout {
    if matches!(
        crate::compositor::Compositor::detect(),
        crate::compositor::Compositor::Niri
    ) {
        let mut cmd = std::process::Command::new("niri");
        cmd.args(["msg", "-j", "keyboard-layouts"]);
        if let Some(out) = run_with_timeout(cmd, Duration::from_millis(500))
            && out.status.success()
            && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout)
            && let Some(names) = v.get("names").and_then(|n| n.as_array())
        {
            let idx = v.get("current_idx").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            let full = names.get(idx).and_then(|n| n.as_str()).unwrap_or("?");
            return KbLayout {
                short: kb_short(full),
            };
        }
    }
    KbLayout {
        short: "--".to_string(),
    }
}

fn kb_short(full: &str) -> String {
    match full {
        "Spanish (Latin American)" => "ES".to_string(),
        s if s.starts_with("English") => "EN".to_string(),
        s => s.chars().take(6).collect(),
    }
}

pub fn kblayout_next() {
    if matches!(
        crate::compositor::Compositor::detect(),
        crate::compositor::Compositor::Niri
    ) {
        let _ = std::process::Command::new("niri")
            .args(["msg", "action", "switch-layout", "next"])
            .spawn();
    }
}

pub fn volume_toggle_mute() {
    let _ = std::process::Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
        .spawn();
}

/// Sube/baja el volumen en un paso (5%), recortando en 100% (`-l`). Devuelve si
/// el proceso terminó bien: se espera a que termine, no como el mute, porque el
/// número del widget se relee enseguida y con `spawn` llegaría viejo.
pub fn volume_step(up: bool) -> bool {
    std::process::Command::new("wpctl")
        .args([
            "set-volume",
            "-l",
            "1.0",
            "@DEFAULT_AUDIO_SINK@",
            if up { "5%+" } else { "5%-" },
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn home_script(path: &str) -> Option<std::path::PathBuf> {
    let p = dirs::home_dir()?.join(path);
    p.exists().then_some(p)
}

fn spawn_script(path: std::path::PathBuf) {
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{} &", path.display()))
        .spawn();
}

/// waybar parity clicks
pub fn open_network_settings() {
    let _ = std::process::Command::new("nmrs").spawn();
}

pub fn open_system_monitor() {
    let _ = std::process::Command::new("alacritty")
        .args(["-e", "btop"])
        .spawn();
}

pub fn open_bluetooth_manager(fallback_toggle: bool, powered: bool) {
    if let Some(p) = home_script(".config/waybar/modules/float-bluetui.sh") {
        spawn_script(p);
    } else if fallback_toggle {
        bluetooth_toggle(powered);
    }
}

pub fn open_volume_control() {
    if std::process::Command::new("pavucontrol").spawn().is_err() {
        volume_toggle_mute();
    }
}

// ===== panel de volumen (click izquierdo en el widget) =====
//
// Fila de la salida + una fila por app que esté sonando + selector de
// dispositivo de salida. Los streams salen de `pactl -f json list sink-inputs`:
// `wpctl status` sólo lista ids de nodo, sin volumen por stream ni forma estable
// de agrupar el par de patas del mismo stream. La salida por defecto sigue con
// wpctl, que es lo que ya mide el widget. Por debajo son los mismos valores
// (PipeWire), así que lo que muestra uno coincide con lo que ajusta el otro.

/// Qué ajusta una fila del panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VolumeTarget {
    /// Salida por defecto (wpctl).
    Output,
    /// Stream de una app, por índice de sink-input (pactl).
    Stream(u32),
}

#[derive(Clone, PartialEq, Debug)]
pub struct VolumeRow {
    pub target: VolumeTarget,
    pub label: String,
    pub pct: u8,
    pub muted: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct AudioDevice {
    pub name: String,
    pub label: String,
    pub default: bool,
}

/// Filas del panel: la salida primero y después un stream por app con audio.
/// Si no hay `wpctl` ni `pactl` vuelve vacía y el panel no se abre.
pub fn read_volume_rows() -> Vec<VolumeRow> {
    let mut rows = Vec::new();
    if let Some((pct, muted)) = read_volume() {
        rows.push(VolumeRow {
            target: VolumeTarget::Output,
            label: "Output".to_string(),
            pct,
            muted,
        });
    }
    let Some(json) = pactl_json(&["-f", "json", "list", "sink-inputs"]) else {
        return rows;
    };
    rows.extend(parse_streams(&json));
    rows
}

pub fn read_audio_devices() -> Vec<AudioDevice> {
    let Some(sinks) = pactl_json(&["-f", "json", "list", "sinks"]) else {
        return Vec::new();
    };
    let default = pactl_json(&["-f", "json", "info"])
        .as_deref()
        .and_then(parse_default_sink)
        .unwrap_or_default();
    parse_devices(&sinks, &default)
}

/// `ponytail:` sin timeout propio más allá de los 400ms de `run_with_timeout`:
/// si pactl se cuelga el panel queda con las filas que ya tenía, sin trabar el
/// hilo principal (que es lo que importa acá).
fn pactl_json(args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new("pactl");
    cmd.args(args);
    let out = run_with_timeout(cmd, Duration::from_millis(400))?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn parse_streams(json: &str) -> Vec<VolumeRow> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|it| {
            let props = it.get("properties");
            let name = props
                .and_then(|p| p.get("application.name"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    props
                        .and_then(|p| p.get("media.name"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("App");
            Some(VolumeRow {
                target: VolumeTarget::Stream(it.get("index")?.as_u64()? as u32),
                label: short_label(name, 20),
                pct: volume_percent(it.get("volume")),
                muted: it.get("mute").and_then(|m| m.as_bool()).unwrap_or(false),
            })
        })
        .collect()
}

pub fn parse_default_sink(info_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(info_json)
        .ok()?
        .get("default_sink_name")?
        .as_str()
        .map(|s| s.to_string())
}

pub fn parse_devices(sinks_json: &str, default: &str) -> Vec<AudioDevice> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(sinks_json) else {
        return Vec::new();
    };
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|s| {
                    let name = s.get("name")?.as_str()?.to_string();
                    let label = s
                        .get("description")
                        .and_then(|d| d.as_str())
                        .map(|d| short_label(d, 22))
                        .unwrap_or_else(|| short_label(&name, 22));
                    Some(AudioDevice {
                        default: name == default,
                        name,
                        label,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// El más alto de los canales: suelen ir juntos, y con el máximo una app con un
/// canal en cero no queda mostrada como muda.
fn volume_percent(volume: Option<&serde_json::Value>) -> u8 {
    let Some(map) = volume.and_then(|v| v.as_object()) else {
        return 0;
    };
    let top = map
        .values()
        .filter_map(|ch| {
            if let Some(p) = ch.get("value_percent").and_then(|v| v.as_str())
                && let Ok(n) = p.trim_end_matches('%').parse::<f32>()
            {
                return Some(n);
            }
            // ----- PA_VOLUME_NORM = 65536 = 100% -----
            ch.get("value")
                .and_then(|v| v.as_f64())
                .map(|v| (v / 655.36) as f32)
        })
        .fold(f32::MIN, f32::max);
    if top == f32::MIN {
        return 0;
    }
    (top.round().clamp(0.0, 150.0)) as u8
}

fn short_label(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}\u{2026}")
}

pub fn set_output_volume(pct: u8) {
    spawn_quiet(
        "wpctl",
        &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{pct}%")],
    );
}

pub fn set_stream_volume(id: u32, pct: u8) {
    let id = id.to_string();
    spawn_quiet("pactl", &["set-sink-input-volume", &id, &format!("{pct}%")]);
}

pub fn toggle_stream_mute(id: u32) {
    let id = id.to_string();
    spawn_quiet("pactl", &["set-sink-input-mute", &id, "toggle"]);
}

pub fn set_default_sink(name: &str) {
    spawn_quiet("pactl", &["set-default-sink", name]);
}

fn spawn_quiet(program: &str, args: &[&str]) {
    let _ = std::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// true si lanzó el rofi externo, false si debe abrir el menú interno
fn hyprctl_json(args: &[&str]) -> Option<serde_json::Value> {
    let mut cmd = std::process::Command::new("hyprctl");
    cmd.args(args);
    let out = run_with_timeout(cmd, Duration::from_millis(500))?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

fn read_workspaces() -> Vec<WorkspaceInfo> {
    match crate::compositor::Compositor::detect() {
        crate::compositor::Compositor::Niri => read_workspaces_niri(),
        _ => read_workspaces_hypr(),
    }
}

pub(crate) fn niri_json(args: &[&str]) -> Option<serde_json::Value> {
    let mut cmd = std::process::Command::new("niri");
    cmd.args(["msg", "--json"]).args(args);
    let out = run_with_timeout(cmd, Duration::from_millis(500))?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// Enfocar una ventana por id (niri). Lo usa el cambiador de ventanas.
pub fn niri_focus_window(id: u64) {
    let _ = std::process::Command::new("niri")
        .args(["msg", "action", "focus-window", "--id", &id.to_string()])
        .spawn();
}

fn read_workspaces_niri() -> Vec<WorkspaceInfo> {
    let Some(list) = niri_json(&["workspaces"]).and_then(|v| v.as_array().cloned()) else {
        return Vec::new();
    };
    let pinned = pinned_output();
    let mut ws: Vec<WorkspaceInfo> = list
        .iter()
        .filter_map(|w| {
            let output = w
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(pin) = pinned
                && output != pin
            {
                return None;
            }
            let id = w
                .get("idx")
                .and_then(|v| v.as_i64())
                .or_else(|| w.get("id").and_then(|v| v.as_i64()))? as i32;
            let active = w
                .get("is_active")
                .and_then(|v| v.as_bool())
                .or_else(|| w.get("is_focused").and_then(|v| v.as_bool()))
                .or_else(|| w.get("active").and_then(|v| v.as_bool()))
                .unwrap_or(false);
            // ----- niri: `active_window_id` es null cuando el workspace no tiene
            // ventanas. Si el campo falta (versión vieja), se asume con ventanas
            // y el autohide sigue como siempre. -----
            let empty = w
                .get("active_window_id")
                .map(|v| v.is_null())
                .unwrap_or(false);
            (id > 0).then_some(WorkspaceInfo {
                id,
                active,
                empty,
                output,
            })
        })
        .collect();
    ws.sort_by_key(|w| w.id);
    ws
}

fn read_workspaces_hypr() -> Vec<WorkspaceInfo> {
    let Some(list) = hyprctl_json(&["workspaces", "-j"]).and_then(|v| v.as_array().cloned()) else {
        return Vec::new();
    };
    // ----- `windows` es el conteo por workspace; sin el campo se asume con
    // ventanas para no cambiar el autohide por una versión vieja de hyprctl. -----
    let mut ws: Vec<(i32, bool)> = list
        .iter()
        .filter_map(|w| {
            let id = w.get("id").and_then(|v| v.as_i64())? as i32;
            // ----- skip special workspaces -----
            if id <= 0 {
                return None;
            }
            let empty = w.get("windows").and_then(|v| v.as_u64()).unwrap_or(1) == 0;
            Some((id, empty))
        })
        .collect();
    ws.sort_unstable();
    ws.dedup();
    let active_id = hyprctl_json(&["activeworkspace", "-j"])
        .and_then(|v| v.get("id").and_then(|v| v.as_i64()))
        .map(|v| v as i32);
    ws.into_iter()
        .map(|(id, empty)| WorkspaceInfo {
            id,
            active: Some(id) == active_id,
            empty,
            output: String::new(),
        })
        .collect()
}

pub fn workspace_switch(id: i32, output: &str, current_output: &str) {
    match crate::compositor::Compositor::detect() {
        crate::compositor::Compositor::Niri if !output.is_empty() => {
            // ----- el índice se repite por monitor, así que hay que enfocar el
            // monitor destino primero... pero SOLO si es otro monitor: la acción
            // focus-monitor de niri mueve el cursor al centro del monitor, y con
            // un único monitor eso recentraba el cursor en cada cambio de
            // workspace. -----
            if output != current_output {
                let _ = std::process::Command::new("niri")
                    .args(["msg", "action", "focus-monitor", output])
                    .output();
            }
            let _ = std::process::Command::new("niri")
                .args(["msg", "action", "focus-workspace", &id.to_string()])
                .spawn();
        }
        crate::compositor::Compositor::Niri => {
            let _ = std::process::Command::new("niri")
                .args(["msg", "action", "focus-workspace", &id.to_string()])
                .spawn();
        }
        _ => {
            let _ = std::process::Command::new("hyprctl")
                .args(["dispatch", "workspace", &id.to_string()])
                .spawn();
        }
    }
}

fn bluetoothctl(args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new("bluetoothctl");
    cmd.args(args);
    let out = run_with_timeout(cmd, Duration::from_millis(500))?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

fn read_bluetooth() -> Option<BluetoothInfo> {
    let show = bluetoothctl(&["show"])?;
    let powered = show.lines().any(|l| l.trim() == "Powered: yes");
    let connected = bluetoothctl(&["devices", "Connected"]).and_then(|out| {
        out.lines()
            .next()
            .and_then(|l| l.splitn(3, ' ').nth(2))
            .map(str::to_string)
    });
    Some(BluetoothInfo { powered, connected })
}

pub fn bluetooth_toggle(currently_powered: bool) {
    let arg = if currently_powered { "off" } else { "on" };
    let _ = std::process::Command::new("bluetoothctl")
        .args(["power", arg])
        .spawn();
}

const MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sept", "Oct", "Nov", "Dec",
];

// ----- fecha estilo "11-Sept" (strftime no tiene ese formato) -----
fn clock_date_now() -> Option<String> {
    unsafe {
        let mut t: libc::time_t = 0;
        libc::time(&mut t);
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        let mon = MONTH_ABBR.get(tm.tm_mon as usize).copied().unwrap_or("???");
        Some(format!("{}-{mon}", tm.tm_mday))
    }
}

fn strftime_now(fmt: &str) -> Option<String> {
    let cfmt = std::ffi::CString::new(fmt).ok()?;
    let mut buf = [0u8; 128];
    let len = unsafe {
        let mut t: libc::time_t = 0;
        libc::time(&mut t);
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            cfmt.as_ptr(),
            &tm,
        )
    };
    if len == 0 {
        return None;
    }
    Some(String::from_utf8_lossy(&buf[..len]).into_owned())
}

pub fn battery_dir() -> Option<PathBuf> {
    let base = Path::new("/sys/class/power_supply");
    std::fs::read_dir(base)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("BAT"))
                .unwrap_or(false)
        })
}

fn read_battery() -> Option<(u8, BatteryState)> {
    let dir = battery_dir()?;
    let capacity: u8 = std::fs::read_to_string(dir.join("capacity"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let status = std::fs::read_to_string(dir.join("status")).ok()?;
    let state = match status.trim() {
        "Charging" => BatteryState::Charging,
        // ----- "Not charging" = enchufado sin cargar (modo conservacion de
        // Lenovo). "Full" es el final de una carga normal: sigue en verde. -----
        "Not charging" => BatteryState::Conserving,
        "Full" => BatteryState::Charging,
        _ => BatteryState::Discharging,
    };
    Some((capacity, state))
}

fn playerctl(args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new("playerctl");
    cmd.args(args);
    let out = run_with_timeout(cmd, Duration::from_millis(500))?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn read_media() -> Option<MediaInfo> {
    let raw = playerctl(&[
        "-a",
        "metadata",
        "--format",
        "{{status}}\t{{title}}\t{{mpris:artUrl}}\t{{xesam:url}}",
    ])?;

    let has_title = |l: &&str| l.split('\t').nth(1).is_some_and(|t| !t.is_empty());
    let line = raw
        .lines()
        .find(|l| l.starts_with("Playing\t") && has_title(l))
        .or_else(|| raw.lines().find(has_title))?;

    let mut fields = line.split('\t');
    let playing = fields.next() == Some("Playing");
    let title = fields.next().unwrap_or_default().to_string();
    let art_field = fields.next().unwrap_or_default();
    let page_url = fields.next().unwrap_or_default();

    let art_path = resolve_art_path(art_field)
        .or_else(|| cached_remote_art(&youtube_thumbnail_url(page_url)?));

    Some(MediaInfo {
        title,
        playing,
        art_path,
    })
}

fn youtube_thumbnail_url(page_url: &str) -> Option<String> {
    if !page_url.contains("youtube.com") && !page_url.contains("youtu.be") {
        return None;
    }
    let id = if let Some(idx) = page_url.find("v=") {
        page_url[idx + 2..].split(['&', '#']).next()
    } else if let Some(idx) = page_url.find("youtu.be/") {
        page_url[idx + "youtu.be/".len()..]
            .split(['?', '&', '#'])
            .next()
    } else {
        None
    }?;
    let valid = id.len() >= 8
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    valid.then(|| format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"))
}

fn resolve_art_path(url: &str) -> Option<String> {
    if let Some(p) = url.strip_prefix("file://") {
        return Some(percent_decode(p));
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return cached_remote_art(url);
    }
    None
}

/// Valor de un dígito hex, o `None` si el byte no lo es.
fn hex_digit(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|d| d as u8)
}

/// El `%XX` se decodifica sobre BYTES, nunca con `&s[i+1..i+3]`: cortar un `str`
/// en un límite que no es de char paniquea, así que un `artUrl` como `%€` mataba
/// el dock (y el chequeo de índices no alcanza: `€` ocupa 3 bytes, o sea que
/// `i + 2` está dentro de `bytes` pero en medio del char).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = || {
            let hi = bytes.get(i + 1).copied().and_then(hex_digit)?;
            let lo = bytes.get(i + 2).copied().and_then(hex_digit)?;
            Some(hi * 16 + lo)
        };
        if bytes[i] == b'%'
            && let Some(byte) = hex()
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn art_cache_dir() -> PathBuf {
    let mut dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    dir.push("dockyrs");
    dir.push("art");
    dir
}

fn cached_remote_art(url: &str) -> Option<String> {
    let dir = art_cache_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let hash = url.bytes().fold(5381u64, |acc, b| {
        acc.wrapping_mul(33).wrapping_add(b as u64)
    });
    let ext = if url.contains(".png") { "png" } else { "jpg" };
    let path = dir.join(format!("{hash:x}.{ext}"));
    if path.exists() {
        return Some(path.to_string_lossy().to_string());
    }
    let status = std::process::Command::new("curl")
        .args(["-s", "-L", "--max-time", "3", "-o"])
        .arg(&path)
        .arg(url)
        .status()
        .ok()?;
    if !status.success() || std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) == 0 {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(path.to_string_lossy().to_string())
}

pub fn media_toggle() {
    let _ = std::process::Command::new("playerctl")
        .arg("play-pause")
        .spawn();
}

// ----- percent and muted -----
pub fn read_volume() -> Option<(u8, bool)> {
    let mut cmd = std::process::Command::new("wpctl");
    cmd.args(["get-volume", "@DEFAULT_AUDIO_SINK@"]);
    // ----- con PipeWire caído `wpctl get-volume` se cuelga. Esto corre en el
    // tick de 2s, así que sin timeout el dock se congelaba para siempre y no
    // revivía ni con Escape. Mismo helper que el resto de las lecturas. -----
    let out = run_with_timeout(cmd, Duration::from_millis(500))?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let muted = text.contains("MUTED");
    let fraction: f32 = text.split_whitespace().nth(1)?.parse().ok()?;
    Some(((fraction * 100.0).round().clamp(0.0, 200.0) as u8, muted))
}

pub fn backlight_dir() -> Option<PathBuf> {
    std::fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .next()
}

pub fn read_brightness() -> Option<u8> {
    let dir = backlight_dir()?;
    let cur: u32 = std::fs::read_to_string(dir.join("brightness"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let max: u32 = std::fs::read_to_string(dir.join("max_brightness"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if max == 0 {
        return None;
    }
    Some(((cur as f32 / max as f32) * 100.0).round() as u8)
}

#[cfg(test)]
mod volume_panel_tests {
    use super::*;

    // ----- muestra real de `pactl -f json list sink-inputs` (recortada) -----
    const INPUTS: &str = r#"[
      {"index": 1731, "mute": false,
       "volume": {"front-left": {"value": 65536, "value_percent": "100%"},
                  "front-right": {"value": 32768, "value_percent": "50%"}},
       "properties": {"application.name": "mpv", "media.name": "Front_Center.wav - mpv"}},
      {"index": 1740, "mute": true,
       "volume": {"mono": {"value": 13107, "value_percent": "20%"}},
       "properties": {"media.name": "Un nombre largo de mas de veinte"}}
    ]"#;

    #[test]
    fn streams_toman_el_canal_mas_alto_y_el_nombre_de_la_app() {
        let rows = parse_streams(INPUTS);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].target, VolumeTarget::Stream(1731));
        assert_eq!(rows[0].label, "mpv");
        assert_eq!(rows[0].pct, 100); // el máximo de los dos canales
        assert!(!rows[0].muted);
        // sin application.name cae al media.name, recortado
        assert_eq!(rows[1].target, VolumeTarget::Stream(1740));
        assert_eq!(rows[1].pct, 20);
        assert!(rows[1].muted);
        assert_eq!(rows[1].label.chars().count(), 20);
    }

    #[test]
    fn json_roto_no_paniquea() {
        assert!(parse_streams("no soy json").is_empty());
        assert!(parse_streams("{}").is_empty());
        assert!(parse_devices("[]", "x").is_empty());
        assert_eq!(parse_default_sink("{}"), None);
    }

    #[test]
    fn el_dispositivo_marcado_es_el_que_manda_pactl() {
        let sinks = r#"[{"name": "alsa_output.pci", "description": "Ryzen HD Audio"},
                        {"name": "hdmi", "description": "HDMI/DP"}]"#;
        let devs = parse_devices(sinks, "hdmi");
        assert_eq!(devs.len(), 2);
        assert!(!devs[0].default);
        assert_eq!(devs[0].label, "Ryzen HD Audio");
        assert!(devs[1].default);
        assert_eq!(
            parse_default_sink(r#"{"default_sink_name": "hdmi"}"#).as_deref(),
            Some("hdmi")
        );
    }
}

#[cfg(test)]
mod percent_decode_tests {
    use super::*;

    #[test]
    fn decodifica_lo_que_tiene_que_decodificar() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("%2Ftmp%2Fx.png"), "/tmp/x.png");
        assert_eq!(percent_decode("sin escapes"), "sin escapes");
        assert_eq!(percent_decode("%41%42"), "AB");
    }

    #[test]
    fn un_porcentaje_roto_no_paniquea() {
        // ----- los casos que mataban el dock: `%` seguido de un char multibyte
        // (cortar el `str` en `i+1..i+3` no era límite de char), `%` al final y
        // `%` con dígitos no-hex. -----
        for entrada in [
            "%€", "a%€b", "%", "a%", "%zz", "%2", "%2€", "100%€", "%ff%€%", "%\u{ff}",
        ] {
            let salida = percent_decode(entrada);
            assert!(
                !salida.is_empty() || entrada.is_empty(),
                "{entrada:?} se decodificó a algo vacío"
            );
        }
        assert_eq!(percent_decode("%"), "%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn un_timeout_mata_al_hijo_colgado() {
        // ----- lo que sostiene a1: `read_volume` corre en el tick de 2s, así que
        // si `run_with_timeout` no matara al proceso colgado el dock se queda
        // congelado. La firma que importa es "vuelve rápido con None". -----
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "sleep 5"]);
        let start = std::time::Instant::now();
        let out = run_with_timeout(cmd, Duration::from_millis(150));
        assert!(out.is_none(), "un comando que se cuelga tiene que dar None");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "tardó {:?}: no está matando al hijo",
            start.elapsed()
        );
    }

    #[test]
    fn un_comando_rapido_pasa_su_stdout() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "echo hola"]);
        let out = run_with_timeout(cmd, Duration::from_secs(5)).expect("debería salir");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hola");
    }
}

#[cfg(test)]
mod custom_widget_tests {
    use super::*;
    use crate::config::{DockSettings, WidgetKind};

    fn fuente(command: &str) -> crate::config::CustomWidgetSource {
        crate::config::CustomWidgetSource {
            enabled: true,
            command: command.to_string(),
            interval_ms: 30_000,
            timeout_ms: 500,
            max_chars: 64,
        }
    }

    fn ajustes_con(fuentes: Vec<crate::config::CustomWidgetSource>) -> DockSettings {
        DockSettings {
            custom_widgets: fuentes,
            ..Default::default()
        }
    }

    fn instantanea(textos: Vec<Option<String>>) -> WidgetSnapshot {
        let mut snapshot = WidgetSnapshot::refresh(&crate::config::DockSettings::default());
        snapshot.custom_last_polls = vec![None; textos.len()];
        snapshot.custom_texts = textos;
        snapshot
    }

    #[test]
    fn normaliza_la_primera_linea_para_medir_y_dibujar() {
        assert_eq!(
            clean_custom_text("  hola\tmundo\u{7}\nsegunda", 64).as_deref(),
            Some("hola mundo")
        );
        assert_eq!(clean_custom_text("abcdef", 3).as_deref(), Some("abc"));
        // ----- el presupuesto es en caracteres, no en bytes -----
        assert_eq!(clean_custom_text("áéí", 2).as_deref(), Some("áé"));
        assert_eq!(clean_custom_text("   \nsegunda", 64), None);
        assert_eq!(custom_text_limit(&fuente("echo hola")), 64);
    }

    #[test]
    fn la_fuente_apagada_o_sin_comando_no_muestra_texto() {
        let mut apagada = fuente("printf 'hola\\n'");
        apagada.enabled = false;
        let ajustes = ajustes_con(vec![apagada]);
        let snapshot = instantanea(vec![Some("viejo".to_string())]);
        assert_eq!(
            custom_text_for(&ajustes, &snapshot, WidgetKind::Custom(0)),
            None
        );
        assert_eq!(
            custom_text_for(&ajustes, &snapshot, WidgetKind::Clock),
            None
        );
        assert_eq!(
            custom_text_for(&ajustes, &snapshot, WidgetKind::Custom(7)),
            None
        );
    }

    #[test]
    fn respeta_el_intervalo_y_conserva_el_texto_si_falla() {
        let ajustes = ajustes_con(vec![fuente("printf '  hola\\nsegunda\\n'")]);
        let mut snapshot = instantanea(Vec::new());
        let ahora = Instant::now();
        assert!(snapshot.refresh_custom(&ajustes.custom_widgets, ahora));
        assert_eq!(snapshot.custom_texts, [Some("hola".to_string())]);
        assert_eq!(snapshot.custom_last_polls, [Some(ahora)]);
        // ----- el intervalo de 30s todavía no venció -----
        assert!(!snapshot.refresh_custom(&ajustes.custom_widgets, ahora));

        let ajustes = ajustes_con(vec![fuente("exit 3")]);
        let mut snapshot = instantanea(vec![Some("viejo".to_string())]);
        assert!(!snapshot.refresh_custom(&ajustes.custom_widgets, ahora));
        assert_eq!(snapshot.custom_texts, [Some("viejo".to_string())]);
        assert_eq!(snapshot.custom_last_polls, [Some(ahora)]);
    }

    #[test]
    fn una_fuente_inactiva_limpia_su_cache_sin_correr_nada() {
        let mut fuente = fuente("exit 3");
        fuente.enabled = false;
        let ajustes = ajustes_con(vec![fuente]);
        let mut snapshot = instantanea(vec![Some("viejo".to_string())]);
        // ----- si corriera `exit 3`, anotaría el intento y conservaría el
        // texto; al limpiar sin tocar `last_polls` prueba que no corrió -----
        assert!(snapshot.refresh_custom(&ajustes.custom_widgets, Instant::now()));
        assert_eq!(snapshot.custom_texts, [None]);
        assert_eq!(snapshot.custom_last_polls, [None]);
    }
}

#[cfg(test)]
mod clock_tests {
    use super::*;
    use crate::config::{DockSettings, WidgetKind, WidgetOptions};

    /// El formato sale de la opción del widget `Clock`, no del reloj global: es
    /// lo que la fila "24-Hour Clock" del editor escribe.
    #[test]
    fn el_formato_del_reloj_sale_de_la_opcion_del_widget() {
        let mut s = DockSettings::default();
        assert_eq!(WidgetSnapshot::clock_format(&s), "%I:%M %p");
        for (puesto, esperado) in [(true, "%H:%M"), (false, "%I:%M %p")] {
            s.set_widget_options(
                WidgetKind::Clock,
                WidgetOptions {
                    clock_24h: Some(puesto),
                    ..Default::default()
                },
            );
            assert_eq!(WidgetSnapshot::clock_format(&s), esperado);
        }
    }

    /// El supuesto del formato de 24 h: `%H:%M` no trae AM/PM ni letras. Si
    /// alguien lo cambia por un formato con sufijo, el ancho del reloj pasa a
    /// depender del idioma y esto lo delata.
    #[test]
    fn el_formato_de_24h_no_trae_sufijo() {
        let texto = strftime_now("%H:%M").expect("strftime");
        assert_eq!(texto.len(), 5, "HH:MM esperado, salió {texto:?}");
        assert!(
            !texto.chars().any(|c| c.is_alphabetic()),
            "el formato de 24 h no debería traer letras: {texto:?}"
        );
    }
}
