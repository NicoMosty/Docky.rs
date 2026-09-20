use std::path::{Path, PathBuf};

pub struct WallpaperEntry {
    pub path: PathBuf,
    pub name: String,
}

/// Carpetas de fondos que ofrece el selector, en orden. La primera es la de
/// fábrica; el ajuste `wallpaper_dir` guarda la elegida y puede ser cualquier
/// ruta (absoluta, o relativa al home).
///
/// A propósito son carpetas de fondos y no carpetas genéricas (Descargas,
/// ~/Pictures): esas traen logos, bocetos y fotos sueltas que, como la lista se
/// ordena por nombre, tapan los fondos de verdad — probado: el selector abría
/// mostrando "A", "B", "C".
pub const WALLPAPER_DIRS: [&str; 4] = [
    "Pictures/Wallpapers",
    "Imágenes/wallpaper",
    "wallpapers",
    ".config/hypr/wallpapers",
];

/// Resuelve la carpeta del ajuste: `~/` y las rutas relativas son al home.
fn resolve_dir(dir: &str) -> PathBuf {
    let dir = dir.trim();
    if let Some(rest) = dir.strip_prefix("~/") {
        return dirs::home_dir().unwrap_or_default().join(rest);
    }
    let path = PathBuf::from(dir);
    if path.is_absolute() {
        path
    } else {
        dirs::home_dir().unwrap_or_default().join(path)
    }
}

/// Etiqueta corta de la carpeta, para mostrarla en el panel.
pub fn dir_label(dir: &str) -> String {
    let dir = if dir.trim().is_empty() {
        WALLPAPER_DIRS[0]
    } else {
        dir.trim()
    };
    if dir.starts_with('/') {
        dir.to_string()
    } else {
        format!("~/{dir}")
    }
}

fn is_wallpaper_file(path: &std::path::Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp"))
            .unwrap_or(false)
}

/// Junta las imágenes de TODAS las carpetas candidatas. Antes se quedaba con la
/// primera no vacía, así que el resto de las carpetas no existía para el
/// selector. Deduplica por ruta y ordena por nombre.
pub fn scan_dirs(dirs: &[PathBuf]) -> Vec<WallpaperEntry> {
    let mut found: Vec<WallpaperEntry> = Vec::new();
    for dir in dirs {
        let Ok(read_dir) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !is_wallpaper_file(&path) || found.iter().any(|w| w.path == path) {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("wallpaper")
                .to_string();
            found.push(WallpaperEntry { path, name });
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

pub fn scan_wallpapers(configured: &str) -> Vec<WallpaperEntry> {
    let dir = if configured.trim().is_empty() {
        WALLPAPER_DIRS[0]
    } else {
        configured.trim()
    };
    scan_dirs(&[resolve_dir(dir)])
}

#[cfg(test)]
mod scan_tests {
    use super::*;

    /// Un fondo en una carpeta distinta a la primera tiene que aparecer igual: con
    /// el escaneo viejo (se quedaba con la primera carpeta no vacía) este test
    /// devolvía solo ["primero"].
    #[test]
    fn junta_varias_carpetas_ignora_no_imagenes_y_deduplica() {
        let base = std::env::temp_dir().join(format!("dockyrs-scan-{}", std::process::id()));
        let a = base.join("Wallpapers");
        let b = base.join("Imágenes/wallpaper");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("primero.jpg"), b"x").unwrap();
        std::fs::write(b.join("segundo.png"), b"x").unwrap();
        std::fs::write(b.join("notas.txt"), b"x").unwrap();
        std::fs::create_dir_all(b.join("subcarpeta.jpg")).unwrap();

        let found = scan_dirs(&[a.clone(), b.clone(), b.clone()]);
        let names: Vec<&str> = found.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["primero", "segundo"]);
        assert!(found.iter().all(|w| w.path.is_file()));

        let _ = std::fs::remove_dir_all(&base);
    }

    /// La carpeta de fábrica es la de Pictures/Wallpapers: sin ajuste, el selector
    /// escanea esa y nada más.
    #[test]
    fn la_carpeta_por_defecto_es_pictures_wallpapers() {
        assert_eq!(WALLPAPER_DIRS[0], "Pictures/Wallpapers");
        assert_eq!(dir_label(""), "~/Pictures/Wallpapers");
        assert_eq!(
            resolve_dir("Pictures/Wallpapers"),
            dirs::home_dir().unwrap().join("Pictures/Wallpapers")
        );
        // ----- una ruta absoluta se respeta tal cual -----
        assert_eq!(dir_label("/tmp/fondos"), "/tmp/fondos");
    }
}

pub fn suggested_dir() -> String {
    dir_label("")
}

/// Archivo que usan los setups de swaybg + systemd: un path unit lo vigila y
/// reinicia swaybg cuando cambia. es la única forma de cambiar el fondo cuando el
/// programa no tiene IPC, que es el caso de swaybg.
fn swaybg_path_file() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/wallpaper-path")
}

/// Qué programa pone el fondo, según la config del compositor. En niri no hay
/// forma de preguntarlo: se busca un programa conocido en su config y, si no
/// aparece, se decide por lo que está corriendo. Hace falta porque no hay interfaz
/// común: swww/awww hablan por IPC y swaybg no tiene ninguna.
fn wallpaper_program() -> Option<String> {
    wallpaper_program_en(&dirs::home_dir()?)
}

/// El programa de fondos que usa el compositor, buscando en `.config/niri` y
/// `.config/hypr` del `home` que se le pase (la recursión entra en los subdirectorios de
/// config, que es como están armados los dos). Separado del `home` real para poder
/// probarlo con un `.kdl` de mentira en un dir temporal (AUDIT.md C5).
fn wallpaper_program_en(home: &std::path::Path) -> Option<String> {
    const KNOWN: [&str; 4] = ["swaybg", "awww", "swww", "hyprpaper"];
    let mut pending = vec![home.join(".config/niri"), home.join(".config/hypr")];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("kdl") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for prog in KNOWN {
                // ----- ignorar las lineas comentadas -----
                if text
                    .lines()
                    .any(|l| !l.trim_start().starts_with("//") && l.contains(prog))
                {
                    return Some(prog.to_string());
                }
            }
        }
    }
    None
}

fn is_running(prog: &str) -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", prog])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// swaybg no tiene IPC: los setups con systemd escriben la ruta en un archivo que
/// un path unit vigila (y que reinicia swaybg y corre matugen). Si ese pipeline no
/// está instalado, se reinicia swaybg a mano con la imagen nueva.
fn apply_with_swaybg(path: &Path) {
    let wrote = std::fs::write(swaybg_path_file(), path.to_string_lossy().as_bytes()).is_ok();
    if wrote {
        let ok = std::process::Command::new("systemctl")
            .args(["user", "start", "wallpaper-change.service"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return;
        }
    }
    let _ = std::process::Command::new("pkill")
        .args(["-x", "swaybg"])
        .status();
    let _ = std::process::Command::new("swaybg")
        .args(["-i", &path.to_string_lossy(), "-m", "fill"])
        .spawn();
}

fn apply_with_awww(path: &Path) {
    let running = std::process::Command::new("awww")
        .arg("query")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !running {
        let _ = std::process::Command::new("awww-daemon").spawn();
        std::thread::sleep(std::time::Duration::from_millis(700));
    }
    let _ = std::process::Command::new("awww")
        .args([
            "img",
            &path.to_string_lossy(),
            "--transition-type",
            "grow",
            "--transition-duration",
            "1.0",
        ])
        .status();
}

/// Corre el matugen del usuario (`~/.config/matugen/config.toml` → sus templates:
/// kitty, waybar, rofi, niri, gtk, …) con el esquema y el modo del dock. Bloquea
/// (1-2 s): el que lo llame decide el hilo.
pub fn run_matugen(path: &Path, scheme: &str, mode: &str) -> bool {
    std::process::Command::new("matugen")
        .args([
            "image",
            &path.to_string_lossy(),
            "--source-color-index",
            "0",
            "--type",
            scheme,
            "--mode",
            mode,
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Aplica el fondo. Con `matugen` (esquema, modo) corre el matugen del usuario
/// **después** de aplicar: con swaybg se espera al `wallpaper-change.service`
/// (`start` bloquea el oneshot), así el esquema del dock gana sobre el `type`
/// del config.toml que ese service ya corrió.
pub fn apply_wallpaper(path: PathBuf, matugen: Option<(String, String)>) {
    std::thread::spawn(move || {
        let programa = wallpaper_program();
        let usa_swaybg =
            programa.as_deref() == Some("swaybg") || (programa.is_none() && is_running("swaybg"));
        log::debug!(
            "wallpaper: aplicando {} con {}",
            path.display(),
            if usa_swaybg { "swaybg" } else { "awww" }
        );
        if usa_swaybg {
            apply_with_swaybg(&path);
        } else {
            apply_with_awww(&path);
        }
        if let Some((scheme, mode)) = matugen {
            log::debug!(
                "wallpaper: matugen apps ({scheme}/{mode}) sobre {}",
                path.display()
            );
            run_matugen(&path, &scheme, &mode);
        }
    });
}

pub fn current_wallpaper_path() -> Option<PathBuf> {
    // ----- setups con swaybg + path unit: la ruta vive en un archivo -----
    if let Ok(text) = std::fs::read_to_string(swaybg_path_file()) {
        let p = text.trim();
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    // ----- swww / awww: por IPC -----
    let output = std::process::Command::new("awww")
        .arg("query")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?;
    let path_str = line.split("image: ").nth(1)?.trim();
    if path_str.is_empty() {
        None
    } else {
        Some(PathBuf::from(path_str))
    }
}

// ----- matugen roles -----
pub struct ColorScheme {
    pub primary: (u8, u8, u8),
    pub secondary: (u8, u8, u8),
    pub on_primary: (u8, u8, u8),
    pub surface: (u8, u8, u8),
    pub on_surface: (u8, u8, u8),
    pub outline: (u8, u8, u8),
}

/// Roles de matugen para el dock. `mode` es `"dark"`/`"light"`: matugen los
/// publica en `colors.<role>.<mode>.color`, así que la clave del JSON sigue al
/// `--mode`.
pub fn extract_color_scheme(
    path: &std::path::Path,
    scheme: &str,
    mode: &str,
) -> Option<ColorScheme> {
    // ----- CON TIMEOUT: matugen tarda 1-2 s de verdad (de ahí el tope holgado), pero
    // colgado dejaba al dock sin dibujar para siempre, porque esto corre en el hilo
    // principal al elegir un fondo (AUDIT.md A2) -----
    let mut cmd = std::process::Command::new("matugen");
    cmd.args([
        "--type",
        scheme,
        "image",
        &path.to_string_lossy(),
        "--source-color-index",
        "0",
        "--json",
        "hex",
        "--mode",
        mode,
    ]);
    let output = crate::widgets::run_with_timeout(cmd, std::time::Duration::from_secs(10))?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let role = |name: &str| -> Option<(u8, u8, u8)> {
        hex_to_rgb(json["colors"][name][mode]["color"].as_str()?)
    };
    Some(ColorScheme {
        primary: role("primary")?,
        secondary: role("secondary")?,
        on_primary: role("on_primary")?,
        surface: role("surface")?,
        on_surface: role("on_surface")?,
        outline: role("outline")?,
    })
}

fn hex_to_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

#[cfg(test)]
mod wallpaper_program_tests {
    use super::wallpaper_program_en;

    fn home_temporal(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dockyrs-wp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(".config/niri/sub")).expect("dir temporal");
        d
    }

    /// C5: la búsqueda del programa de fondos. Entra en los subdirectorios, ignora las
    /// líneas comentadas y devuelve el primero de la lista conocida que aparezca.
    #[test]
    fn encuentra_el_programa_e_ignora_lo_comentado() {
        let home = home_temporal("find");
        // ----- comentado NO cuenta (si no, un `// spawn-at-startup "swaybg"` viejo
        // ganaría y el dock creería que el compositor usa swaybg) -----
        std::fs::write(
            home.join(".config/niri/config.kdl"),
            "// spawn-at-startup \"swaybg\"\n",
        )
        .unwrap();
        assert_eq!(wallpaper_program_en(&home), None, "lo comentado no vale");
        // ----- en un subdirectorio, sí -----
        std::fs::write(
            home.join(".config/niri/sub/2_wallpaper.kdl"),
            "spawn-at-startup \"awww-daemon\"\n",
        )
        .unwrap();
        assert_eq!(wallpaper_program_en(&home).as_deref(), Some("awww"));
        // ----- también mira .config/hypr, y el orden de KNOWN manda -----
        std::fs::create_dir_all(home.join(".config/hypr")).unwrap();
        std::fs::write(
            home.join(".config/hypr/hyprpaper.conf"),
            "preload = /tmp/x.png\n",
        )
        .unwrap();
        std::fs::write(
            home.join(".config/hypr/hyprland.conf"),
            "exec-once = hyprpaper\n",
        )
        .unwrap();
        assert_eq!(
            wallpaper_program_en(&home).as_deref(),
            Some("awww"),
            "awww va antes que hyprpaper en KNOWN"
        );
        // ----- sin nada, None (y no paniquea) -----
        let vacio = std::env::temp_dir().join(format!("dockyrs-wp-vacio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&vacio);
        assert_eq!(wallpaper_program_en(&vacio), None);
        std::fs::remove_dir_all(&home).ok();
    }
}
