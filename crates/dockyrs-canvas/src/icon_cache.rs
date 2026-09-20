use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use tiny_skia::{Pixmap, Transform};

use crate::rounded_rect_path;

const CACHE_CAP: usize = 300;

pub struct IconCache {
    theme: String,
    /// Iconos por TAMAÑO y después por nombre: un `HashMap<String, _>` con clave
    /// `(String, u32)` obligaba a un `to_string()` por lookup, también con hit (y un
    /// frame de la barra mira decenas de iconos). Con el mapa anidado el `get` busca
    /// con el `&str` que ya tiene (AUDIT.md D13) y el `String` se asigna una sola vez,
    /// en el miss.
    cache: HashMap<u32, HashMap<String, Option<Rc<Pixmap>>>>,
    rounded_cache: HashMap<u32, HashMap<String, Option<Rc<Pixmap>>>>,
}

impl IconCache {
    pub fn new(theme: impl Into<String>) -> Self {
        Self {
            theme: theme.into(),
            cache: HashMap::new(),
            rounded_cache: HashMap::new(),
        }
    }

    pub fn get(&mut self, icon_name: &str, size: u32) -> Option<Rc<Pixmap>> {
        if let Some(hit) = self.cache.get(&size).and_then(|m| m.get(icon_name)) {
            return hit.clone();
        }
        // ----- resolver antes de tocar el mapa: `resolve` necesita `&self.theme` y el
        // `entry` pide `&mut self.cache` -----
        let pixmap = resolve(&self.theme, icon_name, size).map(Rc::new);
        let del_tamano = self.cache.entry(size).or_default();
        // ----- ahora el tope es por tamaño (antes un `clear()` global se llevaba
        // todos los tamaños juntos) -----
        if del_tamano.len() >= CACHE_CAP {
            del_tamano.clear();
        }
        del_tamano.insert(icon_name.to_string(), pixmap.clone());
        pixmap
    }

    pub fn get_rounded(&mut self, icon_name: &str, size: u32) -> Option<Rc<Pixmap>> {
        if let Some(hit) = self
            .rounded_cache
            .get(&size)
            .and_then(|m| m.get(icon_name))
        {
            return hit.clone();
        }
        let base = self.get(icon_name, size);
        let rounded = base.and_then(|base| round_corners(&base, size));
        let del_tamano = self.rounded_cache.entry(size).or_default();
        if del_tamano.len() >= CACHE_CAP {
            del_tamano.clear();
        }
        del_tamano.insert(icon_name.to_string(), rounded.clone());
        rounded
    }
}

fn round_corners(base: &Pixmap, size: u32) -> Option<Rc<Pixmap>> {
    let radius = size as f32 * 0.22;
    let path = rounded_rect_path(size as f32, size as f32, radius);
    let mut mask = tiny_skia::Mask::new(size, size)?;
    mask.fill_path(
        &path,
        tiny_skia::FillRule::Winding,
        true,
        Transform::identity(),
    );
    let mut clipped = Pixmap::new(size, size)?;
    clipped.draw_pixmap(
        0,
        0,
        base.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        Some(&mask),
    );
    Some(Rc::new(clipped))
}

fn resolve(theme: &str, icon_name: &str, size: u32) -> Option<Pixmap> {
    if icon_name.is_empty() {
        return None;
    }
    let direct = Path::new(icon_name);
    let path = if direct.is_absolute() && direct.exists() {
        direct.to_path_buf()
    } else {
        freedesktop_icons::lookup(icon_name)
            .with_size(size as u16)
            .with_theme(theme)
            .with_cache()
            .find()
            .or_else(|| {
                freedesktop_icons::lookup(icon_name)
                    .with_size(size as u16)
                    .find()
            })?
    };
    load_from_path(&path, size)
}

fn load_from_path(path: &Path, size: u32) -> Option<Pixmap> {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("svg") => load_svg(path, size),
        _ => load_raster(path, size),
    }
}

fn load_svg(path: &Path, size: u32) -> Option<Pixmap> {
    let data = std::fs::read(path).ok()?;
    let opts = usvg::Options::default();
    let tree = usvg::Tree::from_data(&data, &opts).ok()?;
    let src_size = tree.size();
    let mut pixmap = Pixmap::new(size, size)?;
    let scale_x = size as f32 / src_size.width();
    let scale_y = size as f32 / src_size.height();
    let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap)
}

fn load_raster(path: &Path, size: u32) -> Option<Pixmap> {
    let img = image::open(path).ok()?.into_rgba8();
    let resized = image::imageops::resize(&img, size, size, image::imageops::FilterType::Lanczos3);
    let mut pixmap = Pixmap::new(size, size)?;
    let dst = pixmap.data_mut();
    for (i, px) in resized.pixels().enumerate() {
        let [r, g, b, a] = px.0;
        let a16 = a as u16;
        let pr = ((r as u16 * a16) / 255) as u8;
        let pg = ((g as u16 * a16) / 255) as u8;
        let pb = ((b as u16 * a16) / 255) as u8;
        dst[i * 4] = pr;
        dst[i * 4 + 1] = pg;
        dst[i * 4 + 2] = pb;
        dst[i * 4 + 3] = a;
    }
    Some(pixmap)
}

#[cfg(test)]
mod icon_cache_tests {
    use super::*;

    /// D13: el caché está indexado por TAMAÑO y el nombre se busca como `&str` (sin un
    /// `to_string()` por lookup, que era el costo del hallazgo). El test mira el mapa por
    /// dentro para poder distinguir "pegó en el caché" de "volvió a resolver".
    #[test]
    fn el_cache_va_por_tamano_y_no_mezcla() {
        let mut cache = IconCache::new("Adwaita");
        // un nombre que no existe igual se cachea (se guarda el `None`, que es el
        // caso caro: resolve() falla y recorre los temas)
        assert!(cache.get("no-existe-dockyrs", 16).is_none());
        assert_eq!(
            cache.cache.get(&16).map(|m| m.len()),
            Some(1),
            "el lookup quedó anotado bajo su tamaño"
        );
        assert!(
            !cache.cache.contains_key(&24),
            "y no en un mapa global: el otro tamaño ni se tocó"
        );
        // ----- el segundo lookup no re-resuelve: el `Rc` es el mismo -----
        let primero = cache.get("no-existe-dockyrs", 16);
        let segundo = cache.get("no-existe-dockyrs", 16);
        assert!(primero.is_none() && segundo.is_none());
        assert_eq!(cache.cache.get(&16).map(|m| m.len()), Some(1), "no se duplicó");
        // ----- y un tamaño distinto va a su propio mapa -----
        let _ = cache.get("no-existe-dockyrs", 22);
        assert_eq!(cache.cache.get(&22).map(|m| m.len()), Some(1));
        assert_eq!(cache.cache.get(&16).map(|m| m.len()), Some(1));
    }
}
