mod app;
mod clipboard;
mod compositor;
mod config;
mod desktop;
mod dock;
mod icon_browser;
mod ipc;
mod menu;
mod menu_render;
mod power;
mod render;
mod screenshot;
mod tray;
mod usage;
mod wallpaper;
mod widget;
mod widgets;

use app::App;
use config::Config;
use dock::Dock;
use dockyrs_canvas::{IconCache, TextCache, ThumbnailCache};
use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::{
        WaylandSurface,
        wlr_layer::{KeyboardInteractivity, Layer, LayerShell, LayerSurface},
    },
    shm::{Shm, slot::SlotPool},
};
use wayland_client::{Connection, QueueHandle, globals::registry_queue_init, protocol::wl_output};

pub const ICON_THEME: &str = "WhiteSur";

struct Cli {
    profile: String,
    output: Option<String>,
    command: Option<String>,
    notify_args: Vec<String>,
}

fn parse_cli() -> Cli {
    let mut cli = Cli {
        profile: String::new(),
        output: None,
        command: None,
        notify_args: Vec::new(),
    };
    let mut it = std::env::args().skip(1).peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--profile" => cli.profile = it.next().unwrap_or_default(),
            "--output" => cli.output = it.next(),
            other => {
                if cli.command.is_none() {
                    cli.command = Some(other.to_string());
                } else {
                    cli.notify_args.push(other.to_string());
                }
            }
        }
    }
    cli
}

fn ns(base: &str, profile: &str) -> String {
    if profile.is_empty() {
        base.to_string()
    } else {
        format!("{base}-{profile}")
    }
}

#[allow(clippy::too_many_arguments)]
fn create_dock_surfaces(
    compositor: &CompositorState,
    layer_shell: &LayerShell,
    qh: &QueueHandle<App>,
    dock: &Dock,
    base_w: u32,
    base_h: u32,
    namespace: String,
    output: Option<&wl_output::WlOutput>,
) -> LayerSurface {
    let surface = compositor.create_surface(qh);
    let layer = layer_shell.create_layer_surface(qh, surface, Layer::Top, Some(namespace), output);

    let s = &dock.config.settings;
    let (anchor, margin) = app::edge_anchor_margin(s.dock_edge, s.dock_align, s.pos_y, 0);
    layer.set_anchor(anchor);
    layer.set_size(base_w, base_h);
    layer.set_margin(margin.0, margin.1, margin.2, margin.3);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.commit();
    // ----- sin capa de reserva: el dock flota encima de las ventanas y se
    // revela al pasar el mouse por su propia franja superior -----
    layer
}

// ----- recuperación ante fallo fatal (AUDIT a5/a6) -----
const REEXEC_ENV: &str = "DOCKYRS_REEXEC";
/// Relanzamientos seguidos antes de rendirse.
const MAX_REEXEC: u32 = 3;

/// ¿Vale la pena relanzarse? Un fallo determinista (una config que paniquea al
/// arrancar) no se arregla relanzando: sin este tope el dock gira en un crash
/// loop, que es peor que morir una vez.
fn puede_relanzar(previos: u32) -> bool {
    previos < MAX_REEXEC
}

/// Qué binario relanzar. Prefiere `current_exe` sobre `argv[0]`: argv[0] puede ser
/// RELATIVO (el arranque manual es `./target/release/dockyrs`) y `Command::new` lo
/// resolvería contra el cwd del proceso, que ya no tiene por qué ser el del
/// arranque — ahí la recuperación fallaría justo cuando más hace falta. El
/// autostart de niri usa ruta absoluta, pero relaunch.sh y los arranques a mano no.
fn binario_a_relanzar(argv: &[String]) -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .or_else(|| argv.first().map(std::path::PathBuf::from))
}

/// Relanza el dock con el mismo argv y sale. No vuelve nunca.
///
/// El fd de Wayland es CLOEXEC: al hacer exec el compositor da de baja las
/// superficies viejas y la instancia nueva mapea las suyas. Sirve tanto para el
/// error de protocolo de a6 como para un panic del loop (a5).
fn reexec(argv: &[String], previos: u32) -> ! {
    if !puede_relanzar(previos) {
        log::error!("{previos} relanzamientos seguidos: salgo en vez de girar en bucle");
        std::process::exit(1);
    }
    let Some(bin) = binario_a_relanzar(argv) else {
        log::error!("no pude resolver la ruta del binario: no relanzo");
        std::process::exit(1);
    };
    log::error!(
        "relanzando el dock (intento {}) desde {}",
        previos + 1,
        bin.display()
    );
    // ----- stdin/out/err heredados: el hijo sigue escribiendo en el mismo log,
    // que es justo donde uno mira cuando esto pasa -----
    match std::process::Command::new(&bin)
        .args(&argv[1..])
        .env(REEXEC_ENV, (previos + 1).to_string())
        .spawn()
    {
        Ok(_) => std::process::exit(0),
        Err(err) => {
            log::error!("no pude relanzar: {err}");
            std::process::exit(1);
        }
    }
}

fn main() -> anyhow::Result<()> {
    unsafe { libc::mallopt(libc::M_ARENA_MAX, 1) };
    env_logger::init();

    let cli = parse_cli();
    match cli.command.as_deref() {
        Some("--toggle-search") => {
            ipc::send_message("toggle-search", &cli.profile);
            return Ok(());
        }
        Some("--osd-volume") => {
            ipc::send_message("osd-volume", &cli.profile);
            return Ok(());
        }
        Some("--osd-brightness") => {
            ipc::send_message("osd-brightness", &cli.profile);
            return Ok(());
        }
        Some("--toggle-wallpaper") => {
            ipc::send_message("toggle-wallpaper", &cli.profile);
            return Ok(());
        }
        Some("--toggle-clipboard") => {
            ipc::send_message("toggle-clipboard", &cli.profile);
            return Ok(());
        }
        Some("--screenshot-full") => {
            ipc::send_message("screenshot-full", &cli.profile);
            return Ok(());
        }
        Some("--screenshot-region") => {
            ipc::send_message("screenshot-region", &cli.profile);
            return Ok(());
        }
        Some("--toggle-dock-menu") => {
            ipc::send_message("toggle-dock-menu", &cli.profile);
            return Ok(());
        }
        Some("--test-notification") => {
            ipc::send_message("test-notification", &cli.profile);
            return Ok(());
        }
        Some("--notify") => {
            let title = cli.notify_args.first().cloned().unwrap_or_default();
            let body = cli.notify_args.get(1).cloned().unwrap_or_default();
            ipc::send_message(&format!("notify\u{1f}{title}\u{1f}{body}"), &cli.profile);
            return Ok(());
        }
        Some(unknown) => {
            log::warn!("unknown command: {unknown}");
            return Ok(());
        }
        None => {}
    }

    let config = Config::load(&cli.profile);
    if let Some(output) = cli.output.as_deref() {
        widgets::set_pinned_output(output);
    }
    let mut dock = Dock::new(config);
    let initial_widgets = widgets::WidgetSnapshot::refresh();
    if dock.icons.is_empty() {
        let is_vertical = dock.is_vertical();
        let cross_len = dock.cross_len();
        // ----- resized later -----
        let widget_scale = dock.config.settings.widget_scale;
        dock.widget_bar_content_len = render::widget_bar_natural_len(
            &dock.config.settings,
            &initial_widgets,
            0,
            is_vertical,
            cross_len,
            widget_scale,
        );
    }
    let (base_w, base_h) = dock.base_size();

    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init::<App>(&conn)?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)?;
    let shm = Shm::bind(&globals, &qh)?;
    let clipboard_manager = globals
        .bind::<wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, _, _>(&qh, 1..=2, ())
        .ok();

    let seat_state = SeatState::new(&globals, &qh);
    let seat = seat_state.seats().next();
    // ----- eager avoids race -----
    let clipboard_device = clipboard_manager
        .as_ref()
        .zip(seat.as_ref())
        .map(|(m, s)| m.get_data_device(s, &qh, ()));

    let layer = create_dock_surfaces(
        &compositor,
        &layer_shell,
        &qh,
        &dock,
        base_w,
        base_h,
        ns("dockyrs", &cli.profile),
        None,
    );

    let pool_size = (base_w as usize * 3) * (base_h as usize * 3) * 4;
    let pool = SlotPool::new(pool_size.max(4096), &shm)?;

    let (osd_reset_tx, osd_reset_rx) = std::sync::mpsc::channel::<()>();
    let (ws_reset_tx, ws_reset_rx) = std::sync::mpsc::channel::<()>();
    let (notification_reset_tx, notification_reset_rx) = std::sync::mpsc::channel::<u64>();
    let (marquee_tick_tx, marquee_tick_rx) = std::sync::mpsc::channel::<u64>();
    let (autohide_hide_tx, autohide_hide_rx) = std::sync::mpsc::channel::<u64>();

    let tray_state: tray::TrayState = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let tray_tick_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    tray::spawn(
        tray_state.clone(),
        tray_tick_pending.clone(),
        conn.clone(),
        qh.clone(),
    );

    let available_fonts = std::rc::Rc::new(dockyrs_canvas::list_font_families());
    let mut text_cache = TextCache::new();
    text_cache.set_default_family(&dock.config.settings.dock_font);

    let (paste_tx, paste_rx) = std::sync::mpsc::channel::<(clipboard::PasteTarget, String)>();
    let (clip_tx, clip_rx) = std::sync::mpsc::channel::<(String, Vec<u8>)>();

    let output_state = OutputState::new(&globals, &qh);

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state,
        seat_state,
        shm,
        pool,
        compositor,
        layer_shell,
        layer,
        dock_visible: true,
        autohide_armed: false,
        applied_geom: None,
        applied_size: None,
        last_ptr_event: None,
        ptr_left_at: None,
        autohide_hide_tx,
        pointer: None,
        keyboard: None,
        dock,
        icon_cache: IconCache::new(ICON_THEME),
        text_cache,
        frame_pixmap: None,
        available_fonts: available_fonts.clone(),
        thumbnail_cache: ThumbnailCache::new(),
        output_scale: 1,
        pinned_output: None,
        awaiting_frame: false,
        exit: false,
        first_configure: true,
        pointer_down: false,
        press_pos: None,
        press_icon_index: None,
        menu: None,
        popup_mode: None,
        wallpaper_mode: None,
        dock_menu_mode: None,
        app_search_mode: None,
        osd_mode: None,
        osd_reset_tx,
        ws_flash_mode: None,
        ws_reset_tx,
        notification_mode: None,
        notification_reset_tx,
        widgets: initial_widgets,
        tray: tray_state,
        last_tray_count: 0,
        last_hyprctl_send: None,
        last_empty_click: None,
        mode_was_open: false,
        keyboard_state: 0,
        marquee: render::MarqueeState::default(),
        marquee_tick_tx,
        marquee_rate: 0,
        modifiers: Default::default(),
        held_key: None,
        seat,
        conn: conn.clone(),
        qh: qh.clone(),
        screenshot: None,
        clipboard_manager,
        clipboard_device,
        clipboard_offer: None,
        clipboard_offers: std::collections::HashMap::new(),
        clipboard_source: None,
        clipboard_copy_bytes: std::sync::Arc::new(Vec::new()),
        clipboard_history: clipboard::ClipboardHistory::new(),
        clipboard_ready_at: std::time::Instant::now() + std::time::Duration::from_millis(500),
        clipboard_mode: None,
        clip_tx,
        paste_tx,
    };

    // ----- pin por output: descubrir nombres en la cola principal y recrear anclado -----
    // (reemplazar suelta la superficie temporal vía Drop; los eventos viejos se ignoran por identidad)
    if let Some(want) = cli.output.as_deref() {
        let mut found = None;
        for _ in 0..4 {
            if event_queue.roundtrip(&mut app).is_err() {
                break;
            }
            found = app
                .output_state
                .outputs()
                .find(|o| app.output_state.info(o).and_then(|i| i.name).as_deref() == Some(want));
            if found.is_some() {
                break;
            }
        }
        match found {
            Some(o) => {
                let (bw, bh) = app.dock.base_size();
                let nl = create_dock_surfaces(
                    &app.compositor,
                    &app.layer_shell,
                    &app.qh,
                    &app.dock,
                    bw,
                    bh,
                    ns("dockyrs", &cli.profile),
                    Some(&o),
                );
                app.layer = nl;
                app.pinned_output = Some(o);
                app.first_configure = true;
                app.awaiting_frame = false;
            }
            None => log::warn!("output '{want}' not found, running unpinned"),
        }
    }

    app.screenshot = screenshot::ScreenshotState::new(
        &globals,
        &app.qh,
        &app.compositor,
        &app.layer_shell,
        &app.output_state,
        app.pinned_output.clone(),
    );

    let (ipc_tx, ipc_rx) = std::sync::mpsc::channel::<ipc::IpcMessage>();
    ipc::spawn_listener(
        ipc_tx.clone(),
        conn.clone(),
        qh.clone(),
        cli.profile.clone(),
    );
    ipc::spawn_brightness_watcher(ipc_tx.clone(), conn.clone(), qh.clone());
    ipc::spawn_battery_watcher(ipc_tx.clone(), conn.clone(), qh.clone());
    ipc::spawn_bluetooth_watcher(ipc_tx.clone(), conn.clone(), qh.clone());
    ipc::spawn_media_watcher(ipc_tx.clone(), conn.clone(), qh.clone());
    ipc::spawn_workspace_watcher(ipc_tx, conn.clone(), qh.clone());

    let clock_tick_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_clock_ticker(clock_tick_pending.clone(), conn.clone(), qh.clone());

    let cpu_ram_tick_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_cpu_ram_ticker(cpu_ram_tick_pending.clone(), conn.clone(), qh.clone());

    let sys_tick_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_sys_ticker(sys_tick_pending.clone(), conn.clone(), qh.clone());

    let osd_timeout_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_osd_timer(
        osd_reset_rx,
        osd_timeout_pending.clone(),
        conn.clone(),
        qh.clone(),
        menu::OSD_TIMEOUT_MS,
    );

    let ws_timeout_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_osd_timer(
        ws_reset_rx,
        ws_timeout_pending.clone(),
        conn.clone(),
        qh.clone(),
        menu::WS_FLASH_TIMEOUT_MS,
    );

    let notification_timeout_pending =
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_notification_timer(
        notification_reset_rx,
        notification_timeout_pending.clone(),
        conn.clone(),
        qh.clone(),
    );

    let marquee_tick_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_marquee_ticker(
        marquee_tick_rx,
        marquee_tick_pending.clone(),
        conn.clone(),
        qh.clone(),
    );

    let autohide_timeout_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_autohide_timer(
        autohide_hide_rx,
        autohide_timeout_pending.clone(),
        conn.clone(),
        qh.clone(),
    );

    // ----- estado inicial coherente del autohide -----
    app.sync_autohide_surfaces();

    // ----- relanzarse en vez de desaparecer. El contador va por entorno para
    // sobrevivir al exec y cortar el crash loop. -----
    let argv: Vec<String> = std::env::args().collect();
    let reexecs: u32 = std::env::var(REEXEC_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    loop {
        // ----- a6: un error de protocolo Wayland no se puede "seguir" (la
        // conexión está muerta), así que la recuperación es relanzarse. El
        // `catch_unwind` cubre además los panics del dispatch (dibujo y punteros),
        // que corren todos por acá. Los panics de los drenajes de abajo no pasan
        // por el guard y se arreglan en la fuente, que es donde importan. -----
        let corrida = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            event_queue.blocking_dispatch(&mut app)
        }));
        match corrida {
            // ----- `blocking_dispatch` devuelve la cantidad de eventos -----
            Ok(Ok(_)) => {}
            Ok(Err(err)) => {
                log::error!("wayland: {err}");
                reexec(&argv, reexecs);
            }
            Err(_) => reexec(&argv, reexecs),
        }
        while let Ok(msg) = ipc_rx.try_recv() {
            match msg {
                ipc::IpcMessage::ToggleSearch => app.toggle_app_search(&qh),
                ipc::IpcMessage::OsdVolume => app.show_osd(menu::OsdKind::Volume, &qh),
                ipc::IpcMessage::OsdBrightness => app.show_osd(menu::OsdKind::Brightness, &qh),
                ipc::IpcMessage::MediaChanged => app.refresh_media(&qh),
                ipc::IpcMessage::BatteryChanged => app.refresh_battery(&qh),
                ipc::IpcMessage::BluetoothChanged => app.refresh_bluetooth(&qh),
                ipc::IpcMessage::WorkspacesChanged => app.refresh_workspaces(&qh),
                ipc::IpcMessage::ToggleWallpaper => app.toggle_wallpaper_picker(&qh),
                ipc::IpcMessage::ToggleClipboard => app.toggle_clipboard(&qh),
                ipc::IpcMessage::ScreenshotFull => app.start_full_screenshot(&qh),
                ipc::IpcMessage::ScreenshotRegion => app.start_region_screenshot(&qh),
                ipc::IpcMessage::ToggleDockMenu => app.toggle_dock_menu(&qh),
                ipc::IpcMessage::TestNotification => app.show_notification(
                    "Test Notification".to_string(),
                    "This is a test notification from Docky.rs".to_string(),
                    &qh,
                ),
                ipc::IpcMessage::Notify(title, body) => app.show_notification(title, body, &qh),
            }
        }
        while let Ok((target, text)) = paste_rx.try_recv() {
            app.apply_pending_paste(target, text, &qh);
        }
        while let Ok((mime, data)) = clip_rx.try_recv() {
            app.ingest_clipboard_capture(mime, data, &qh);
        }
        if clock_tick_pending.swap(false, std::sync::atomic::Ordering::SeqCst) {
            app.refresh_clock(&qh);
        }
        if cpu_ram_tick_pending.swap(false, std::sync::atomic::Ordering::SeqCst) {
            app.refresh_cpu_ram(&qh);
        }
        if sys_tick_pending.swap(false, std::sync::atomic::Ordering::SeqCst) {
            app.refresh_sys(&qh);
        }
        if tray_tick_pending.swap(false, std::sync::atomic::Ordering::SeqCst) {
            app.sync_tray_layout(&qh);
        }
        if osd_timeout_pending.swap(false, std::sync::atomic::Ordering::SeqCst) {
            app.close_osd_mode(&qh);
        }
        if ws_timeout_pending.swap(false, std::sync::atomic::Ordering::SeqCst) {
            app.close_ws_flash_mode(&qh);
        }
        if notification_timeout_pending.swap(false, std::sync::atomic::Ordering::SeqCst) {
            app.close_notification_mode(&qh);
        }
        if marquee_tick_pending.swap(false, std::sync::atomic::Ordering::SeqCst) {
            app.tick_marquee(&qh);
        }
        if autohide_timeout_pending.swap(false, std::sync::atomic::Ordering::SeqCst) {
            app.autohide_timeout(&qh);
        }
        if app.exit {
            break;
        }
    }

    Ok(())
}

fn spawn_clock_ticker(
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    conn: Connection,
    qh: wayland_client::QueueHandle<App>,
) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(20));
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            conn.display().sync(&qh, ());
            let _ = conn.flush();
        }
    });
}

fn spawn_sys_ticker(
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    conn: Connection,
    qh: wayland_client::QueueHandle<App>,
) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            conn.display().sync(&qh, ());
            let _ = conn.flush();
        }
    });
}

fn spawn_cpu_ram_ticker(
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    conn: Connection,
    qh: wayland_client::QueueHandle<App>,
) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(15));
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            conn.display().sync(&qh, ());
            let _ = conn.flush();
        }
    });
}

fn spawn_osd_timer(
    reset_rx: std::sync::mpsc::Receiver<()>,
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    conn: Connection,
    qh: wayland_client::QueueHandle<App>,
    timeout_ms: u64,
) {
    use std::sync::mpsc::RecvTimeoutError;
    std::thread::spawn(move || {
        loop {
            if reset_rx.recv().is_err() {
                return;
            }
            loop {
                match reset_rx.recv_timeout(std::time::Duration::from_millis(timeout_ms)) {
                    Ok(()) => continue,
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            conn.display().sync(&qh, ());
            let _ = conn.flush();
        }
    });
}

fn spawn_notification_timer(
    reset_rx: std::sync::mpsc::Receiver<u64>,
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    conn: Connection,
    qh: wayland_client::QueueHandle<App>,
) {
    use std::sync::mpsc::RecvTimeoutError;
    std::thread::spawn(move || {
        let mut dur;
        loop {
            match reset_rx.recv() {
                Ok(ms) => dur = ms,
                Err(_) => return,
            }
            loop {
                match reset_rx.recv_timeout(std::time::Duration::from_millis(dur)) {
                    Ok(ms) => {
                        dur = ms;
                        continue;
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            conn.display().sync(&qh, ());
            let _ = conn.flush();
        }
    });
}

fn spawn_autohide_timer(
    rx: std::sync::mpsc::Receiver<u64>,
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    conn: Connection,
    qh: wayland_client::QueueHandle<App>,
) {
    use std::sync::mpsc::RecvTimeoutError;
    std::thread::spawn(move || {
        let mut dur;
        loop {
            match rx.recv() {
                Ok(ms) => dur = ms,
                Err(_) => return,
            }
            loop {
                match rx.recv_timeout(std::time::Duration::from_millis(dur)) {
                    Ok(ms) => {
                        dur = ms;
                        continue;
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            conn.display().sync(&qh, ());
            let _ = conn.flush();
        }
    });
}

fn spawn_marquee_ticker(
    rx: std::sync::mpsc::Receiver<u64>,
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    conn: Connection,
    qh: wayland_client::QueueHandle<App>,
) {
    use std::sync::mpsc::RecvTimeoutError;
    std::thread::spawn(move || {
        let mut interval_ms: u64 = 0;
        loop {
            if interval_ms == 0 {
                match rx.recv() {
                    Ok(v) => interval_ms = v,
                    Err(_) => return,
                }
                continue;
            }
            match rx.recv_timeout(std::time::Duration::from_millis(interval_ms)) {
                Ok(v) => interval_ms = v,
                Err(RecvTimeoutError::Timeout) => {
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    conn.display().sync(&qh, ());
                    let _ = conn.flush();
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

#[cfg(test)]
mod reexec_tests {
    use super::*;

    #[test]
    fn el_crash_loop_se_corta() {
        // ----- si el fallo es determinista, relanzar no lo arregla: el tope es lo
        // que evita girar para siempre. -----
        assert!(puede_relanzar(0));
        assert!(puede_relanzar(MAX_REEXEC - 1));
        assert!(!puede_relanzar(MAX_REEXEC));
        assert!(!puede_relanzar(MAX_REEXEC + 5));
    }

    /// El guard del binario a relanzar. Si alguien vuelve a usar `argv[0]` a secas,
    /// el primer assert ya no pasa (queda `Some("./target/…")`, relativo) y el
    /// segundo tampoco, porque argv[0] relativo se resuelve contra el cwd.
    #[test]
    fn resuelve_el_binario_por_ruta_absoluta_no_por_argv0() {
        let relativo = vec!["./target/release/dockyrs".to_string()];
        let bin = binario_a_relanzar(&relativo).expect("tiene que resolver el binario");
        assert!(
            bin.is_absolute(),
            "el binario a relanzar tiene que ser absoluto, no {}",
            bin.display()
        );
        assert!(
            bin.exists(),
            "{} tiene que existir para poder relanzar",
            bin.display()
        );
    }
}
