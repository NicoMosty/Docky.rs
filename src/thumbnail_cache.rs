use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use tiny_skia::{Pixmap, Transform};

pub struct ThumbnailCache {
    cache: HashMap<(String, u32, u32), Option<Rc<Pixmap>>>,
}

impl ThumbnailCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn peek(&self, path: &Path, w: u32, h: u32) -> Option<Rc<Pixmap>> {
        self.cache
            .get(&(path.to_string_lossy().to_string(), w, h))
            .and_then(|v| v.clone())
    }

    pub fn insert(&mut self, path: &Path, w: u32, h: u32, pixmap: Option<Pixmap>) {
        let key = (path.to_string_lossy().to_string(), w, h);
        self.cache.insert(key, pixmap.map(Rc::new));
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

// ----- baked corners -----
pub fn load(path: &Path, w: u32, h: u32, radius: f32) -> Option<Pixmap> {
    // ----- `resize_to_fill` = escalar para cubrir + recorte centrado. Antes esto
    // se hacia a mano con `crop_imm(..).to_image()`, que copiaba la imagen
    // ENTERA a resolucion completa: un fondo 4K (33 MB decodificado) pasaba a
    // ~66 MB de pico para terminar mostrando 200x112 px. Medido: abrir el
    // selector con 5 fondos costaba 45 MB de RSS. Mismo criterio que
    // `clipboard/history.rs` y `screenshot/encode.rs`, que ya lo usaban. -----
    let resized = image::open(path)
        .ok()?
        .resize_to_fill(w, h, image::imageops::FilterType::Triangle)
        .into_rgba8();

    let mut pixmap = Pixmap::new(w, h)?;
    let dst = pixmap.data_mut();
    for (i, px) in resized.pixels().enumerate() {
        let [r, g, b, a] = px.0;
        let a16 = a as u16;
        dst[i * 4] = ((r as u16 * a16) / 255) as u8;
        dst[i * 4 + 1] = ((g as u16 * a16) / 255) as u8;
        dst[i * 4 + 2] = ((b as u16 * a16) / 255) as u8;
        dst[i * 4 + 3] = a;
    }

    let rect = rounded_rect_path(w as f32, h as f32, radius);
    let mut mask = tiny_skia::Mask::new(w, h)?;
    mask.fill_path(
        &rect,
        tiny_skia::FillRule::Winding,
        true,
        Transform::identity(),
    );
    let mut clipped = Pixmap::new(w, h)?;
    clipped.draw_pixmap(
        0,
        0,
        pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        Some(&mask),
    );
    Some(clipped)
}

fn rounded_rect_path(w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(r, 0.0);
    pb.line_to(w - r, 0.0);
    pb.quad_to(w, 0.0, w, r);
    pb.line_to(w, h - r);
    pb.quad_to(w, h, w - r, h);
    pb.line_to(r, h);
    pb.quad_to(0.0, h, 0.0, h - r);
    pb.line_to(0.0, r);
    pb.quad_to(0.0, 0.0, r, 0.0);
    pb.close();
    pb.finish().unwrap()
}

#[cfg(test)]
mod thumbnail_tests {
    use super::*;

    /// PNG temporal de `w`x`h` con tres bandas verticales R/V/A.
    fn bandas(nombre: &str, w: u32, h: u32) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("dockyrs-thumb-{nombre}.png"));
        let img = image::RgbaImage::from_fn(w, h, |x, _| match x * 3 / w {
            0 => image::Rgba([255, 0, 0, 255]),
            1 => image::Rgba([0, 255, 0, 255]),
            _ => image::Rgba([0, 0, 255, 255]),
        });
        img.save(&path).unwrap();
        path
    }

    fn pixel(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.width() + x) * 4) as usize;
        let d = p.data();
        [d[i], d[i + 1], d[i + 2], d[i + 3]]
    }

    /// 400x100 (4:1) -> 50x50 (1:1) tiene que RECORTAR el centro (queda la banda
    /// verde), no aplastar la imagen entera. Si se vuelve a un resize sin
    /// recorte, el centro sale con las tres bandas y este test cae.
    #[test]
    fn recorta_al_centro_en_vez_de_aplastar() {
        let path = bandas("4a1", 400, 100);
        let p = load(&path, 50, 50, 0.0).expect("no cargo el png de prueba");
        assert_eq!((p.width(), p.height()), (50, 50));
        let [r, g, b, _] = pixel(&p, 25, 25);
        assert!(
            g > 200 && r < 60 && b < 60,
            "centro = {r},{g},{b}; esperaba verde (recorte centrado)"
        );
    }

    /// El radio tiene que dejar las esquinas transparentes y el centro opaco.
    #[test]
    fn las_esquinas_quedan_transparentes() {
        let path = bandas("radio", 200, 200);
        let p = load(&path, 40, 40, 12.0).expect("no cargo el png de prueba");
        assert_eq!(pixel(&p, 0, 0)[3], 0, "esquina sup-izq transparente");
        assert_eq!(pixel(&p, 39, 39)[3], 0, "esquina inf-der transparente");
        assert_eq!(pixel(&p, 20, 20)[3], 255, "el centro opaco");
    }
}
