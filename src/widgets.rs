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
}

pub struct NetworkInfo {
    pub label: String,
    pub online: bool,
}

pub struct KbLayout {
    pub short: String,
}

pub struct WidgetSnapshot {
    pub time: String,
    pub date: String,
    pub battery: Option<(u8, bool)>,
    pub media: Option<MediaInfo>,
    pub bluetooth: Option<BluetoothInfo>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub cpu: Option<u8>,
    pub ram: Option<u8>,
    pub ram_gb: Option<(f32, f32)>,
    pub volume: Option<(u8, bool)>,
    pub network: NetworkInfo,
    pub kblayout: KbLayout,
}

impl WidgetSnapshot {
    pub fn refresh() -> Self {
        Self {
            time: strftime_now("%I:%M %p").unwrap_or_default(),
            date: strftime_now("%a %b %d").unwrap_or_default(),
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
        }
    }

    pub fn refresh_cpu_ram(&mut self) {
        self.cpu = read_cpu();
        if let Some((pct, gb)) = read_ram() {
            self.ram = Some(pct);
            self.ram_gb = Some(gb);
        }
    }

    /// true si cambió algo visible (requiere relayout)
    pub fn refresh_sys(&mut self) -> bool {
        let volume = read_volume();
        let network = read_network();
        let kblayout = read_kblayout();
        let changed = volume != self.volume
            || network.label != self.network.label
            || kblayout.short != self.kblayout.short;
        self.volume = volume;
        self.network = network;
        self.kblayout = kblayout;
        changed
    }

    pub fn refresh_workspaces(&mut self) {
        self.workspaces = read_workspaces();
    }

    pub fn refresh_clock(&mut self) -> bool {
        let time = strftime_now("%I:%M %p").unwrap_or_default();
        let date = strftime_now("%a %b %d").unwrap_or_default();
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
        return NetworkInfo { label, online: true };
    }
    NetworkInfo { label: "Disconnected".to_string(), online: false }
}

fn wifi_iface() -> Option<String> {
    let wireless = std::fs::read_to_string("/proc/net/wireless").ok()?;
    wireless.lines().nth(2)?.split(':').next().map(|s| s.trim().to_string())
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
    Some(NetworkInfo { label, online: true })
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
            return KbLayout { short: kb_short(full) };
        }
    }
    KbLayout { short: "--".to_string() }
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

fn home_script(path: &str) -> Option<std::path::PathBuf> {
    let p = dirs::home_dir()?.join(path);
    p.exists().then_some(p)
}

fn spawn_script(path: std::path::PathBuf) {
    let _ = std::process::Command::new("sh").arg("-c").arg(format!("{} &", path.display())).spawn();
}

/// waybar parity clicks
pub fn open_network_settings() {
    let _ = std::process::Command::new("nmrs").spawn();
}

pub fn open_system_monitor() {
    let _ = std::process::Command::new("alacritty").args(["-e", "btop"]).spawn();
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

/// true si lanzó el rofi externo, false si debe abrir el menú interno
pub fn open_power_external() -> bool {
    if let Some(p) = home_script("Scripts/sh-scripts/rofi/rofi-powermenu.sh") {
        spawn_script(p);
        true
    } else {
        false
    }
}

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

fn niri_json(args: &[&str]) -> Option<serde_json::Value> {
    let mut cmd = std::process::Command::new("niri");
    cmd.args(["msg", "--json"]).args(args);
    let out = run_with_timeout(cmd, Duration::from_millis(500))?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

fn read_workspaces_niri() -> Vec<WorkspaceInfo> {
    let Some(list) = niri_json(&["workspaces"]).and_then(|v| v.as_array().cloned()) else {
        return Vec::new();
    };
    let mut ws: Vec<WorkspaceInfo> = list
        .iter()
        .filter_map(|w| {
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
            (id > 0).then_some(WorkspaceInfo { id, active })
        })
        .collect();
    ws.sort_by_key(|w| w.id);
    ws
}

fn read_workspaces_hypr() -> Vec<WorkspaceInfo> {
    let Some(list) = hyprctl_json(&["workspaces", "-j"]).and_then(|v| v.as_array().cloned()) else {
        return Vec::new();
    };
    let mut ids: Vec<i32> = list
        .iter()
        .filter_map(|w| w.get("id").and_then(|v| v.as_i64()))
        .map(|v| v as i32)
        .filter(|&id| id > 0) // ----- skip special workspaces -----
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let active_id = hyprctl_json(&["activeworkspace", "-j"])
        .and_then(|v| v.get("id").and_then(|v| v.as_i64()))
        .map(|v| v as i32);
    ids.into_iter()
        .map(|id| WorkspaceInfo {
            id,
            active: Some(id) == active_id,
        })
        .collect()
}

pub fn workspace_switch(id: i32) {
    match crate::compositor::Compositor::detect() {
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

fn read_battery() -> Option<(u8, bool)> {
    let dir = battery_dir()?;
    let capacity: u8 = std::fs::read_to_string(dir.join("capacity"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let status = std::fs::read_to_string(dir.join("status")).ok()?;
    let charging = matches!(status.trim(), "Charging" | "Full");
    Some((capacity, charging))
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

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16)
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
    let out = std::process::Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()?;
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
