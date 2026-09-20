/// Escribe un archivo de config de OTRA app sin dejarlo a medias: `.tmp` en el mismo
/// directorio + `rename` (atómico dentro del mismo sistema de archivos). Un `write`
/// directo que se corta (crash, disco lleno, kill) deja `gtk-3.0/settings.ini`,
/// `kdeglobals` o `kitty.conf` rotos **para el resto del sistema**, no sólo para el dock
/// (AUDIT.md B10). El temporal lleva el pid para que dos instancias del dock no se pisen
/// —`--profile` permite correr una por monitor— y se copian los permisos del original
/// si ya existía (un `rename` no los hereda).
fn escribir_atomico(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, content)?;
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&tmp);
            Err(err)
        }
    }
}

fn read_ini_value(path: &std::path::Path, section: &str, key: &str) -> Option<String> {
    let existing = std::fs::read_to_string(path).ok()?;
    let section_header = format!("[{section}]");
    let key_prefix = format!("{key}=");
    let mut in_section = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == section_header {
            in_section = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_section = false;
            continue;
        }
        if in_section && trimmed.starts_with(&key_prefix) {
            return Some(trimmed[key_prefix.len()..].to_string());
        }
    }
    None
}

fn merge_ini_key(path: &std::path::Path, section: &str, key: &str, value: &str) {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let section_header = format!("[{section}]");
    let key_prefix = format!("{key}=");
    let full_line = format!("{key}={value}");

    let mut lines: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut wrote_key = false;
    let mut saw_section = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == section_header {
            saw_section = true;
            in_section = true;
            lines.push(line.to_string());
            continue;
        }
        if trimmed.starts_with('[') {
            if in_section && !wrote_key {
                lines.push(full_line.clone());
                wrote_key = true;
            }
            in_section = false;
            lines.push(line.to_string());
            continue;
        }
        if in_section && trimmed.starts_with(&key_prefix) {
            lines.push(full_line.clone());
            wrote_key = true;
            continue;
        }
        lines.push(line.to_string());
    }
    if !saw_section {
        lines.push(section_header);
        lines.push(full_line);
    } else if !wrote_key {
        lines.push(full_line);
    }
    if let Err(err) = escribir_atomico(path, &(lines.join("\n") + "\n")) {
        log::warn!("no pude escribir {path:?}: {err}");
    }
}

// ----- gtk ini -----
pub(super) fn apply_system_gtk_font(family: &str) {
    for gtk_dir in ["gtk-3.0", "gtk-4.0"] {
        let Some(mut path) = dirs::config_dir() else {
            continue;
        };
        path.push(gtk_dir);
        if std::fs::create_dir_all(&path).is_err() {
            continue;
        }
        path.push("settings.ini");
        merge_ini_key(&path, "Settings", "gtk-font-name", &format!("{family} 10"));
    }
}

// ----- qt kdeglobals -----
pub(super) fn apply_system_qt_font(family: &str) {
    let Some(mut path) = dirs::config_dir() else {
        return;
    };
    path.push("kdeglobals");
    let tail = read_ini_value(&path, "General", "font")
        .and_then(|v| v.split_once(',').map(|(_, rest)| rest.to_string()))
        .unwrap_or_else(|| "10,-1,0,50,0,0,0,0,0".to_string());
    merge_ini_key(&path, "General", "font", &format!("{family},{tail}"));
}

fn merge_flat_config_key(path: &std::path::Path, key: &str, value: &str) {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let full_line = format!("{key} {value}");
    let mut lines: Vec<String> = Vec::new();
    let mut wrote = false;
    for line in existing.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') && trimmed.split_whitespace().next() == Some(key) {
            lines.push(full_line.clone());
            wrote = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !wrote {
        lines.push(full_line);
    }
    if let Err(err) = escribir_atomico(path, &(lines.join("\n") + "\n")) {
        log::warn!("no pude escribir {path:?}: {err}");
    }
}

pub(super) fn apply_kitty_font(family: &str) {
    let Some(mut path) = dirs::config_dir() else {
        return;
    };
    path.push("kitty");
    path.push("kitty.conf");
    if !path.exists() {
        return;
    }
    merge_flat_config_key(&path, "font_family", family);

    let Ok(entries) = std::fs::read_dir("/tmp") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("kitty-dockyrs.sock-") {
            continue;
        }
        let target = format!("unix:{}", entry.path().display());
        let _ = std::process::Command::new("kitty")
            .args(["@", "--to", &target, "load-config"])
            .spawn();
    }
}

#[cfg(test)]
mod font_write_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn dir_temporal(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dockyrs-fonts-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).expect("dir temporal");
        d
    }

    /// B10: la escritura es atómica (`.tmp` + `rename`), no deja temporales y no le
    /// afloja los permisos al archivo que ya existía (el `rename` no los hereda).
    #[test]
    fn la_escritura_no_deja_temporales_y_conserva_permisos() {
        let dir = dir_temporal("perm");
        let path = dir.join("kdeglobals");
        std::fs::write(&path, "[General]\nfont=Ubuntu,10\n").expect("original");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
        merge_ini_key(&path, "General", "font", "JetBrainsMono Nerd Font,11");
        let out = std::fs::read_to_string(&path).expect("leer");
        assert!(
            out.contains("JetBrainsMono Nerd Font,11"),
            "escribió: {out:?}"
        );
        assert!(!out.contains("Ubuntu"), "reemplaza la clave vieja: {out:?}");
        let modo = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo, 0o600, "los permisos del original se conservan");
        let sobrantes: Vec<String> = std::fs::read_dir(&dir)
            .expect("read_dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(sobrantes.is_empty(), "quedaron temporales: {sobrantes:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Y el caso normal del arranque: un archivo que todavía no existe se crea.
    #[test]
    fn un_archivo_nuevo_se_crea() {
        let dir = dir_temporal("nuevo");
        let path = dir.join("settings.ini");
        merge_ini_key(&path, "Settings", "gtk-font-name", "Inter 10");
        let out = std::fs::read_to_string(&path).expect("leer");
        assert!(
            out.contains("[Settings]") && out.contains("gtk-font-name=Inter 10"),
            "{out:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
