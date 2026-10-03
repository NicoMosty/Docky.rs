use std::path::PathBuf;

#[derive(Clone)]
pub struct DesktopEntry {
    /// Nombre del archivo `.desktop` sin extensión: es la identidad estable de la
    /// entrada. La usa el historial de uso y es lo que deduplica entre carpetas.
    pub id: String,
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub keywords: Vec<String>,
    /// `Terminal=true`: hay que lanzarla dentro de una terminal.
    pub terminal: bool,
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data_home) = dirs::data_dir() {
        dirs.push(data_home.join("applications"));
    }
    if let Ok(xdg_data_dirs) = std::env::var("XDG_DATA_DIRS") {
        for dir in xdg_data_dirs.split(':') {
            dirs.push(PathBuf::from(dir).join("applications"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/local/share/applications"));
        dirs.push(PathBuf::from("/usr/share/applications"));
    }
    dirs
}

fn clean_exec(raw: &str) -> String {
    raw.split_whitespace()
        .filter(|tok| !tok.starts_with('%'))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Los escritorios en los que estamos, para `OnlyShowIn`/`NotShowIn` (formato XDG,
/// separados por `:`). Además del entorno se suma el compositor: así las entradas
/// marcadas para KDE o GNOME no aparecen acá.
fn current_desktops() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(value) = std::env::var("XDG_CURRENT_DESKTOP") {
        out.extend(
            value
                .split(':')
                .filter(|p| !p.is_empty())
                .map(|p| p.to_lowercase()),
        );
    }
    out.push(match crate::compositor::Compositor::detect() {
        crate::compositor::Compositor::Niri => "niri".to_string(),
        crate::compositor::Compositor::Hyprland => "hyprland".to_string(),
    });
    out
}

/// Prefijos de locale a probar para un `Name[es]`: `es_AR` y después `es`.
fn locale_prefixes() -> Vec<String> {
    let mut out = Vec::new();
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        let Ok(value) = std::env::var(var) else {
            continue;
        };
        if value.is_empty() || value == "C" || value == "POSIX" {
            continue;
        }
        let base = value.split('.').next().unwrap_or_default();
        if base.is_empty() {
            continue;
        }
        out.push(base.to_string());
        if let Some(lang) = base.split('_').next()
            && lang != base
        {
            out.push(lang.to_string());
        }
        break;
    }
    out
}

/// Una lista XDG (`a;b;`) incluye alguno de los escritorios actuales?
fn list_has_current(list: &str, current: &[String]) -> bool {
    list.split(';')
        .filter(|s| !s.is_empty())
        .any(|s| current.iter().any(|c| c.eq_ignore_ascii_case(s)))
}

fn parse_entry(contents: &str, id: &str) -> Option<DesktopEntry> {
    let mut in_main_section = false;
    let mut name = None;
    let mut localized: Vec<(String, String)> = Vec::new();
    let mut icon = None;
    let mut exec = None;
    let mut keywords = Vec::new();
    let mut terminal = false;
    let mut no_display = false;
    let mut only_show_in = String::new();
    let mut not_show_in = String::new();

    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main_section = line == "[Desktop Entry]";
            continue;
        }
        if !in_main_section || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if let Some(locale) = key.strip_prefix("Name[")
            && let Some(locale) = locale.strip_suffix(']')
        {
            localized.push((locale.to_string(), value.to_string()));
            continue;
        }
        match key {
            "Name" if name.is_none() => name = Some(value.to_string()),
            "Icon" => icon = Some(value.to_string()),
            "Exec" => exec = Some(clean_exec(value)),
            "Keywords" => {
                keywords = value
                    .split(';')
                    .filter(|k| !k.is_empty())
                    .map(|k| k.to_string())
                    .collect();
            }
            "Terminal" => terminal = value.eq_ignore_ascii_case("true"),
            "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
            "OnlyShowIn" => only_show_in = value.to_string(),
            "NotShowIn" => not_show_in = value.to_string(),
            _ => {}
        }
    }

    if no_display {
        return None;
    }
    // ----- filtrado por escritorio, como hace rofi -----
    let current = current_desktops();
    if !only_show_in.is_empty() && !list_has_current(&only_show_in, &current) {
        return None;
    }
    if list_has_current(&not_show_in, &current) {
        return None;
    }
    // ----- nombre localizado: `Name[es]` gana sobre `Name` -----
    let mut name = name?;
    for prefix in locale_prefixes() {
        if let Some((_, localized_name)) = localized
            .iter()
            .find(|(locale, _)| locale.eq_ignore_ascii_case(&prefix))
        {
            name = localized_name.clone();
            break;
        }
    }

    Some(DesktopEntry {
        id: id.to_string(),
        name,
        icon: icon.unwrap_or_default(),
        exec: exec?,
        keywords,
        terminal,
    })
}

/// Calidad del match, como la ve rofi: prefijo (0) > inicio de palabra (1) >
/// contiene (2) > subsecuencia difusa (3). `None` si no matchea. Con la consulta
/// vacía todo matchea con la misma calidad, así que el orden lo decide el uso.
pub fn match_tier(entry: &DesktopEntry, query: &str) -> Option<u8> {
    let name = entry.name.to_lowercase();
    let keywords: Vec<String> = entry.keywords.iter().map(|k| k.to_lowercase()).collect();
    tier_of(&name, &keywords, query)
}

/// La calidad del match, con el nombre YA en minúsculas. Extraído de `match_tier` para que
/// las keybinds usen el MISMO ranking sin un `DesktopEntry` de por medio.
pub(crate) fn tier_of(name: &str, keywords: &[String], query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(0);
    }
    if name.starts_with(query) || keywords.iter().any(|k| k.starts_with(query)) {
        return Some(0);
    }
    if name
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(query))
    {
        return Some(1);
    }
    if name.contains(query) || keywords.iter().any(|k| k.contains(query)) {
        return Some(2);
    }
    if fuzzy_match(name, query) {
        return Some(3);
    }
    None
}

/// Un atajo de niri: las teclas y qué hace. `label` es el `hotkey-overlay-title` si lo
/// tiene —es lo que el usuario escribió para leerlo— y si no, el comando.
pub struct Keybind {
    pub keys: String,
    pub label: String,
    /// Teclas + comando + título, en minúsculas, para buscar.
    pub search: String,
}

/// Los atajos de la config de niri del usuario. Niri **no** los expone por IPC, así que se
/// leen los `.kdl` de `~/.config/niri` (recursivo, como `wallpaper_program`), salteando los
/// comentarios. Un bind de una sola tecla que no sea `Print`, `XF86…` o `F<n>` no se
/// reconoce: agregarlo a `es_bind` si algún día aparece.
pub fn list_niri_keybinds() -> Vec<Keybind> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    list_niri_keybinds_en(&home)
}

fn list_niri_keybinds_en(home: &std::path::Path) -> Vec<Keybind> {
    let mut pendientes = vec![home.join(".config/niri")];
    let mut out: Vec<Keybind> = Vec::new();
    let mut vistos = std::collections::HashSet::new();
    while let Some(dir) = pendientes.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pendientes.push(path);
                continue;
            }
            // ----- `.kdl.bak` no es `.kdl`: los backups quedan afuera solos -----
            if path.extension().and_then(|e| e.to_str()) != Some("kdl") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for kb in keybinds_from_kdl(&text) {
                if vistos.insert((kb.keys.clone(), kb.label.clone())) {
                    out.push(kb);
                }
            }
        }
    }
    out.sort_by(|a, b| a.keys.cmp(&b.keys));
    out
}

/// `true` si el primer token de una línea es una combinación de teclas. Es una heurística:
/// acá no hay un KDL formal, así que se acepta lo que tiene `+` (la forma normal) y las
/// teclas sueltas conocidas.
fn es_bind(tok: &str) -> bool {
    if tok.contains('+') {
        return true;
    }
    let es_f =
        tok.len() > 1 && tok.starts_with('F') && tok[1..].chars().all(|c| c.is_ascii_digit());
    tok == "Print" || tok.starts_with("XF86") || es_f
}

/// Corta la línea en el primer `//` que esté FUERA de comillas: un `spawn-sh "…http://…"`
/// no se puede cortar por el `//` de una URL.
fn sin_comentario(line: &str) -> &str {
    let mut quotes = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'"' => quotes = !quotes,
            b'/' if !quotes && bytes[i + 1] == b'/' => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

/// El valor de `hotkey-overlay-title="…"` si está en la línea.
fn titulo_del_bind(line: &str) -> String {
    let Some(rest) = line.split_once("hotkey-overlay-title=").map(|(_, r)| r) else {
        return String::new();
    };
    let rest = rest.trim_start().strip_prefix('"').unwrap_or(rest);
    rest.split('"').next().unwrap_or("").to_string()
}

/// La acción, legible: sin comillas y con un solo espacio entre palabras.
fn limpiar_accion(accion: &str) -> String {
    accion
        .replace('"', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Traduce un `.kdl` de niri a atajos. Separado del sistema de archivos para poder testearlo
/// con un texto fijo (mismo criterio que `windows_from_niri`).
pub(crate) fn keybinds_from_kdl(text: &str) -> Vec<Keybind> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(raw) = lines.next() {
        let line = sin_comentario(raw);
        let t = line.trim();
        let Some(first) = t.split_whitespace().next() else {
            continue;
        };
        if !t.contains('{') || !es_bind(first) {
            continue;
        }
        let titulo = titulo_del_bind(t);
        // ----- el cuerpo: lo que sigue al `{`. Si el bloque no cierra en la misma línea,
        // se junta hasta el `}` (niri acepta binds multilínea) -----
        let mut cuerpo = t
            .split_once('{')
            .map(|(_, r)| r.to_string())
            .unwrap_or_default();
        while !cuerpo.contains('}') {
            let Some(next) = lines.next() else {
                break;
            };
            cuerpo.push(' ');
            cuerpo.push_str(sin_comentario(next).trim());
        }
        let accion = limpiar_accion(cuerpo.split([';', '}']).next().unwrap_or(""));
        let keys = first.to_string();
        let label = if titulo.is_empty() {
            accion.clone()
        } else {
            titulo.clone()
        };
        out.push(Keybind {
            search: format!("{keys} {accion} {titulo}").to_lowercase(),
            keys,
            label,
        });
    }
    out
}

/// Subsecuencia: `chr` encuentra "Google Chrome". Es lo que hace que una consulta
/// desprolija igual encuentre la app, como el fuzzy de rofi.
fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    let mut rest = haystack.chars();
    needle.chars().all(|n| rest.any(|h| h == n))
}

pub fn list_all_desktop_entries() -> Vec<DesktopEntry> {
    let mut seen = std::collections::HashSet::new();
    let mut entries = Vec::new();
    for dir in application_dirs() {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        for item in read_dir.flatten() {
            let path = item.path();
            let Some(file_name) = path.file_name().and_then(|f| f.to_str()) else {
                continue;
            };
            if !file_name.ends_with(".desktop") || !seen.insert(file_name.to_string()) {
                continue;
            }
            let id = file_name.trim_end_matches(".desktop");
            if let Ok(contents) = std::fs::read_to_string(&path)
                && let Some(entry) = parse_entry(&contents, id)
            {
                entries.push(entry);
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Las ventanas abiertas, como entradas del launcher: el nombre es el tí­tulo y el
/// `id` de la entrada es el id de ventana de niri (así Enter la enfoca en vez de
/// lanzar algo). El icono se busca por `app_id`, best-effort.
pub fn list_niri_windows() -> Vec<DesktopEntry> {
    let Some(list) = crate::widgets::niri_json(&["windows"]) else {
        return Vec::new();
    };
    windows_from_niri(&list, &list_all_desktop_entries())
}

/// Traduce `niri msg --json windows` a entradas del launcher. Está separado para
/// poder testearlo con un JSON sintético, sin niri corriendo.
fn windows_from_niri(list: &serde_json::Value, apps: &[DesktopEntry]) -> Vec<DesktopEntry> {
    let Some(list) = list.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for window in list {
        let Some(id) = window.get("id").and_then(|v| v.as_u64()) else {
            continue;
        };
        let title = window
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("(sin titulo)")
            .to_string();
        let app_id = window
            .get("app_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let focused = window
            .get("is_focused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let (icon, app_name) = apps
            .iter()
            .find(|e| {
                let app = app_id.to_lowercase();
                !app.is_empty()
                    && (e.id.to_lowercase().contains(&app) || e.name.to_lowercase() == app)
            })
            .map(|e| (e.icon.clone(), e.name.clone()))
            .unwrap_or_default();
        let name = if focused {
            format!("* {title}")
        } else if app_name.is_empty() {
            title
        } else {
            format!("{title}  ({app_name})")
        };
        out.push(DesktopEntry {
            id: id.to_string(),
            name,
            icon,
            exec: app_id.clone(),
            keywords: vec![app_id],
            terminal: false,
        });
    }
    out
}

#[cfg(test)]
mod entry_tests {
    use super::*;

    fn entry(name: &str, keywords: &[&str]) -> DesktopEntry {
        DesktopEntry {
            id: "test".into(),
            name: name.into(),
            icon: String::new(),
            exec: "test".into(),
            keywords: keywords.iter().map(|k| k.to_string()).collect(),
            terminal: false,
        }
    }

    #[test]
    fn parsea_keywords_terminal_y_nombre_localizado() {
        let contents = "[Desktop Entry]\n\
             Name=Files\n\
             Name[es]=Archivos\n\
             Icon=system-file-manager\n\
             Exec=nautilus %U\n\
             Keywords=files;manager;\n\
             Terminal=true\n";
        let parsed = parse_entry(contents, "org.gnome.Nautilus").unwrap();
        assert_eq!(parsed.id, "org.gnome.Nautilus");
        assert_eq!(parsed.exec, "nautilus");
        assert_eq!(parsed.keywords, vec!["files", "manager"]);
        assert!(parsed.terminal);
        // ----- el nombre depende del locale del entorno del test; lo que importa
        // es que sea uno de los dos y no basura -----
        assert!(parsed.name == "Files" || parsed.name == "Archivos");
    }

    #[test]
    fn descarta_entradas_de_otros_escritorios_y_nodisplay() {
        let kde = "[Desktop Entry]\nName=KDE App\nExec=x\nOnlyShowIn=KDE;\n";
        assert!(parse_entry(kde, "kde-app").is_none());
        let hidden = "[Desktop Entry]\nName=Hidden\nExec=x\nNoDisplay=true\n";
        assert!(parse_entry(hidden, "hidden").is_none());
        let neutral = "[Desktop Entry]\nName=Normal\nExec=x\n";
        assert!(parse_entry(neutral, "normal").is_some());
    }

    /// El JSON de `niri msg --json windows` se traduce a entradas: el `id` de la
    /// entrada es el id de ventana (con eso Enter la enfoca en vez de lanzar algo),
    /// el título es el nombre y el icono se resuelve por `app_id`.
    #[test]
    fn traduce_las_ventanas_de_niri() {
        let json: serde_json::Value = serde_json::from_str(
            r#"[{"id":2,"title":"Docky.rs","app_id":"kitty","workspace_id":1,"is_focused":true},
                {"id":15,"title":"Mozilla","app_id":"firefox","workspace_id":3,"is_focused":false}]"#,
        )
        .unwrap();
        let mut firefox = entry("Firefox", &[]);
        firefox.id = "firefox".into();
        firefox.icon = "firefox-icon".into();

        let windows = windows_from_niri(&json, &[firefox]);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].id, "2");
        assert!(windows[0].name.contains("Docky.rs"));
        assert_eq!(windows[1].id, "15");
        assert_eq!(windows[1].icon, "firefox-icon");
        // ----- y con algo que no es una lista, no explota -----
        assert!(windows_from_niri(&serde_json::json!({}), &[]).is_empty());
    }

    /// El matching tiene que escalonar como rofi: prefijo antes que contiene,
    /// contiene antes que difuso, y el difuso tiene que encontrar de verdad.
    #[test]
    fn el_matching_escala_y_el_difuso_encuentra() {
        let chrome = entry("Google Chrome", &[]);
        assert_eq!(match_tier(&chrome, "goo"), Some(0));
        assert_eq!(match_tier(&chrome, "chrome"), Some(1));
        assert_eq!(match_tier(&chrome, "ogle"), Some(2));
        assert_eq!(match_tier(&chrome, "gch"), Some(3));
        assert_eq!(match_tier(&chrome, "zzz"), None);
        // ----- y matchea por keywords -----
        let mut editor = entry("Text Editor", &["notas"]);
        assert_eq!(match_tier(&editor, "notas"), Some(0));
        editor.keywords.clear();
        assert_eq!(match_tier(&editor, "notas"), None);
        // ----- consulta vacía: todo entra con la misma calidad -----
        assert_eq!(match_tier(&chrome, ""), Some(0));
    }
}

#[cfg(test)]
mod keybind_tests {
    use super::*;

    /// El parser de los `.kdl` de niri: el `hotkey-overlay-title` como etiqueta, el bind sin
    /// título (la etiqueta es el comando, sin comillas), el bloque multilínea, los
    /// comentarios (incluido el que viene DESPUÉS de un bind en la misma línea) y las líneas
    /// que NO son binds (los bloques de config de niri).
    #[test]
    fn el_parser_de_binds_lee_lo_que_importa() {
        let texto = r#"
layout {
    gaps 8
}
binds {
    Mod+T hotkey-overlay-title="Open a Terminal: kitty" { spawn "kitty"; }
    Mod+Q { close-window; }
    Mod+Shift+Slash {
        show-hotkey-overlay;
    }
    Mod+Down  { focus-window-or-workspace-down; } // Mod+Down  { focus-window-down; }
    Print { screenshot; }
    XF86AudioMute allow-when-locked=true { spawn-sh "wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle"; }
    window-rule { open-floating true; }
}
"#;
        let binds = keybinds_from_kdl(texto);
        let keys: Vec<&str> = binds.iter().map(|b| b.keys.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "Mod+T",
                "Mod+Q",
                "Mod+Shift+Slash",
                "Mod+Down",
                "Print",
                "XF86AudioMute"
            ],
            "ni `layout` ni `window-rule` son binds"
        );
        // ----- el título manda como etiqueta -----
        assert_eq!(binds[0].label, "Open a Terminal: kitty");
        // ----- sin título, la etiqueta es el comando y sin comillas -----
        assert_eq!(binds[1].label, "close-window");
        // ----- el bloque multilínea junta la acción -----
        assert_eq!(binds[2].label, "show-hotkey-overlay");
        // ----- y el comentario de la MISMA línea no se come la acción -----
        assert_eq!(binds[3].label, "focus-window-or-workspace-down");
        assert_eq!(binds[4].label, "screenshot");
        assert!(binds[5].label.starts_with("spawn-sh wpctl set-mute"));
        // ----- el texto de búsqueda junta teclas + comando + título -----
        assert!(binds[0].search.contains("mod+t"));
        assert!(binds[0].search.contains("kitty"));
        assert!(binds[0].search.contains("open a terminal"));
    }

    /// El comentario entre comillas no corta la línea: una URL con `//` es válida.
    #[test]
    fn una_url_no_se_confunde_con_un_comentario() {
        let texto = r#"Mod+Y { spawn-sh "curl -s https://x.test/a//b"; }"#;
        let binds = keybinds_from_kdl(texto);
        assert_eq!(binds.len(), 1);
        assert!(binds[0].search.contains("https://x.test/a//b"));
    }

    /// El ranking es el MISMO del launcher (se comparte `tier_of`): prefijo > inicio de
    /// palabra > contiene > difuso.
    #[test]
    fn el_ranking_de_las_keybinds_es_el_del_launcher() {
        let kw = |s: &str| vec![s.to_string()];
        assert_eq!(tier_of("mod+t", &kw(""), "mod"), Some(0));
        // ----- "t" entra por inicio de palabra, no por contiene -----
        assert_eq!(tier_of("mod+t", &kw(""), "t"), Some(1));
        assert_eq!(tier_of("mod+t", &kw(""), "+t"), Some(2));
        assert_eq!(
            tier_of("mod+t", &kw("open a terminal"), "terminal"),
            Some(2)
        );
        assert!(tier_of("mod+t", &kw("close"), "zzz").is_none());
    }
}
