//! Historial de uso de apps para ordenar el launcher como rofi: lo que más usás,
//! primero. Es caché, no configuración: vive en
//! `~/.cache/dockyrs/app-usage.json` y se puede borrar sin perder nada.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Usage {
    /// id del `.desktop` -> (veces lanzada, último uso en segundos epoch)
    apps: HashMap<String, (u32, u64)>,
}

fn file() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("dockyrs").join("app-usage.json"))
}

fn load() -> Usage {
    file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(usage: &Usage) {
    let Some(path) = file() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(usage) {
        let _ = std::fs::write(path, text);
    }
}

/// Cuánto conviene subir una app en la lista. Combina frecuencia y antigüedad
/// (frecency): 10 por lanzamiento y hasta 20 de bonus si el último uso fue hace
/// poco, decayendo en semanas. Es el criterio de rofi, simplificado.
pub fn score(id: &str) -> f64 {
    let usage = load();
    let Some((count, last)) = usage.apps.get(id) else {
        return 0.0;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(*last);
    let age_days = now.saturating_sub(*last) as f64 / 86_400.0;
    let recency = 1.0 / (1.0 + age_days / 7.0);
    *count as f64 * 10.0 + recency * 20.0
}

/// Registrar un lanzamiento (se llama al elegir una app en el launcher).
pub fn record(id: &str) {
    let mut usage = load();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let slot = usage.apps.entry(id.to_string()).or_insert((0, now));
    slot.0 = slot.0.saturating_add(1);
    slot.1 = now;
    save(&usage);
}

#[cfg(test)]
mod usage_tests {
    use super::*;

    /// Sin historial, score 0 (así el orden queda alfabético).
    #[test]
    fn sin_historial_no_suma() {
        assert_eq!(score("no-existe-esta-app"), 0.0);
    }

    /// Más lanzamientos y más reciente, mejor score.
    #[test]
    fn frecuencia_y_antiguedad_ordenan() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut usage = Usage::default();
        usage.apps.insert("poco".into(), (1, now));
        usage.apps.insert("mucho".into(), (10, now));
        usage.apps.insert("vieja".into(), (10, now - 90 * 86_400));
        save(&usage);

        assert!(score("mucho") > score("poco"));
        assert!(score("mucho") > score("vieja"));
        // ----- limpieza: el test no debe dejar historial real -----
        if let Some(p) = file() {
            let _ = std::fs::remove_file(p);
        }
    }
}
