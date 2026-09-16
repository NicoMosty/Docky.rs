use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tiny_skia::{Pixmap, Transform};

use crate::rounded_rect_path;

/// Cuántas miniaturas se guardan en memoria. Cada una pesa `w * h * 4` (una de
/// 240x150 = 144 KB), así que 64 son ~9 MB de tope. Antes se tiraban todas al
/// cerrar el selector de fondos y la próxima visita volvía a decodificar los
/// originales: medido, 1370 ms de CPU y un pico de 26 MB **por visita**. Lo que
/// se desaloja por el tope se puede recuperar del caché en disco.
const MEM_CAP: usize = 64;

/// Tope de archivos del caché en disco (~100 KB cada uno = ~50 MB).
const DISK_CAP: usize = 512;

pub struct ThumbnailCache {
    cache: HashMap<(String, u32, u32), Option<Rc<Pixmap>>>,
    /// Orden de inserción, para desalojar el más viejo cuando pasa el tope.
    order: std::collections::VecDeque<(String, u32, u32)>,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ThumbnailCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            order: std::collections::VecDeque::new(),
        }
    }

    pub fn peek(&self, path: &Path, w: u32, h: u32) -> Option<Rc<Pixmap>> {
        self.cache
            .get(&(path.to_string_lossy().to_string(), w, h))
            .and_then(|v| v.clone())
    }

    pub fn insert(&mut self, path: &Path, w: u32, h: u32, pixmap: Option<Pixmap>) {
        let key = (path.to_string_lossy().to_string(), w, h);
        if self
            .cache
            .insert(key.clone(), pixmap.map(Rc::new))
            .is_none()
        {
            self.order.push_back(key);
        }
        while self.order.len() > MEM_CAP {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
            }
        }
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

    let pixmap = premultiply(&resized, w, h)?;
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

/// Igual que `load`, pero con un caché en disco de la miniatura **ya escalada y
/// con las esquinas horneadas**: los originales del usuario son fotos de varios
/// MB (un 4K decodifica a ~33 MB de pico y tarda ~85 ms por imagen), y esa cuenta
/// se pagaba en cada visita al selector. Leer el PNG de 240x150 cuesta ~1-3 ms.
///
/// La clave lleva el `mtime` y el tamaño del original, así que si cambiás el
/// fondo (o lo editás) la miniatura vieja no se reusa: se regenera sola y la
/// vieja la limpia `prune`. `cache_dir` en `None` = sin disco, sólo decodificar.
pub fn load_cached(
    path: &Path,
    w: u32,
    h: u32,
    radius: f32,
    cache_dir: Option<&Path>,
) -> Option<Pixmap> {
    let cached =
        cache_dir.map(|dir| dir.join(format!("{:016x}.png", cache_key(path, w, h, radius))));
    if let Some(cached) = cached.as_ref()
        && let Some(pixmap) = read_cached(cached, w, h)
    {
        return Some(pixmap);
    }
    let pixmap = load(path, w, h, radius)?;
    if let Some(cached) = cached.as_ref()
        && let Some(dir) = cached.parent()
    {
        let _ = std::fs::create_dir_all(dir);
        let _ = pixmap.save_png(cached);
        prune(dir);
    }
    Some(pixmap)
}

/// Identifica la miniatura: el archivo (por mtime y tamaño, para que editarlo la
/// invalide), el tamaño pedido y el radio horneado.
fn cache_key(path: &Path, w: u32, h: u32, radius: f32) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(&mut hasher);
        meta.modified().ok().hash(&mut hasher);
    }
    w.hash(&mut hasher);
    h.hash(&mut hasher);
    radius.to_bits().hash(&mut hasher);
    hasher.finish()
}

/// El PNG guarda el alfa recto, así que hay que volver a premultiplicar (lo que
/// hace `load` antes de recortar). Si el archivo no tiene el tamaño pedido (caché
/// de otra versión, o corrupto) se descarta y se regenera.
fn read_cached(path: &Path, w: u32, h: u32) -> Option<Pixmap> {
    let img = image::open(path).ok()?.into_rgba8();
    if img.dimensions() != (w, h) {
        return None;
    }
    premultiply(&img, w, h)
}

/// `RgbaImage` -> `Pixmap` con el alfa premultiplicado, que es lo que espera
/// tiny-skia.
fn premultiply(img: &image::RgbaImage, w: u32, h: u32) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(w, h)?;
    let dst = pixmap.data_mut();
    for (dst_px, src_px) in dst.chunks_exact_mut(4).zip(img.pixels()) {
        let [r, g, b, a] = src_px.0;
        let a16 = a as u16;
        dst_px[0] = ((r as u16 * a16) / 255) as u8;
        dst_px[1] = ((g as u16 * a16) / 255) as u8;
        dst_px[2] = ((b as u16 * a16) / 255) as u8;
        dst_px[3] = a;
    }
    Some(pixmap)
}

/// Poda barata del caché: si pasó el tope, borra los más viejos (por mtime). Sin
/// esto el directorio crece sin techo cada vez que cambiás un fondo, porque cada
/// versión de un archivo tiene su propia clave.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let modified = e.metadata().ok()?.modified().ok()?;
            path.extension()
                .is_some_and(|x| x == "png")
                .then_some((modified, path))
        })
        .collect();
    if files.len() <= DISK_CAP {
        return;
    }
    files.sort();
    let sobra = files.len() - DISK_CAP / 2;
    for (_, path) in files.iter().take(sobra) {
        let _ = std::fs::remove_file(path);
    }
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

    fn tmp_dir(nombre: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dockyrs-cache-{nombre}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
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

    /// El caché en disco tiene que escribir la miniatura y devolver **los mismos
    /// píxeles** al leerla (si el PNG perdiera el alfa premultiplicado o las
    /// esquinas, la segunda visita mostraría la miniatura distinta).
    #[test]
    fn el_cache_en_disco_devuelve_lo_mismo() {
        let dir = tmp_dir("disco");
        let src = bandas("cache", 200, 200);
        let prima = load_cached(&src, 40, 40, 12.0, Some(&dir)).expect("primera carga");
        let archivos: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(archivos.len(), 1, "tiene que quedar un PNG en el caché");

        let segunda =
            load_cached(&src, 40, 40, 12.0, Some(&dir)).expect("segunda carga (del caché)");
        for (x, y) in [(0, 0), (5, 5), (20, 20), (39, 39), (12, 30)] {
            assert_eq!(
                pixel(&prima, x, y),
                pixel(&segunda, x, y),
                "pixel {x},{y} distinto entre decodificar y leer del caché"
            );
        }
        // ----- y las esquinas siguen horneadas: el PNG guarda alfa recto, así que
        // sin premultiplicar de vuelta el borde saldría sucio -----
        assert_eq!(pixel(&segunda, 0, 0)[3], 0);
    }

    /// Sin carpeta de caché no escribe nada, y un tamaño distinto no reusa la
    /// miniatura de otro tamaño.
    #[test]
    fn la_clave_distingue_tamano_y_no_escribe_sin_disco() {
        let dir = tmp_dir("clave");
        let src = bandas("clave", 200, 200);
        let _ = load_cached(&src, 40, 40, 12.0, Some(&dir)).unwrap();
        let _ = load_cached(&src, 20, 20, 12.0, Some(&dir)).unwrap();
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            2,
            "dos tamaños, dos miniaturas"
        );
        let sin_disco = tmp_dir("sin-disco");
        let _ = load_cached(&src, 40, 40, 12.0, None).unwrap();
        assert_eq!(std::fs::read_dir(&sin_disco).unwrap().count(), 0);
    }

    /// Editar el original (misma ruta, otro contenido) no puede reusar la
    /// miniatura vieja: la clave lleva el tamaño y el `mtime` del archivo.
    #[test]
    fn cambiar_el_original_invalida_la_miniatura() {
        let dir = tmp_dir("invalidar");
        let src = bandas("invalidar", 200, 200);
        let _ = load_cached(&src, 40, 40, 12.0, Some(&dir)).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        // ----- mismo nombre, imagen distinta -----
        image::RgbaImage::from_fn(300, 300, |x, _| {
            if x < 150 {
                image::Rgba([255, 255, 0, 255])
            } else {
                image::Rgba([0, 255, 255, 255])
            }
        })
        .save(&src)
        .unwrap();
        let actualizada = load_cached(&src, 40, 40, 12.0, Some(&dir)).unwrap();
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            2,
            "el fondo editado tiene que generar su propia miniatura"
        );
        let [r, g, b, _] = pixel(&actualizada, 5, 20);
        assert!(r > 200 && g > 200 && b < 60, "y mostrar el contenido nuevo");
    }

    /// El tope de memoria desaloja el más viejo: si no, un directorio con cientos
    /// de fondos dejaría todas las miniaturas residentes para siempre.
    #[test]
    fn el_tope_de_memoria_desaloja_el_mas_viejo() {
        let mut cache = ThumbnailCache::new();
        let chico = || Pixmap::new(2, 2).unwrap();
        for i in 0..MEM_CAP + 5 {
            let path = PathBuf::from(format!("/tmp/fondo-{i}.jpg"));
            cache.insert(&path, 2, 2, Some(chico()));
        }
        assert!(
            cache.peek(Path::new("/tmp/fondo-0.jpg"), 2, 2).is_none(),
            "el primero tiene que haber salido"
        );
        assert!(
            cache
                .peek(Path::new(&format!("/tmp/fondo-{}.jpg", MEM_CAP + 4)), 2, 2)
                .is_some(),
            "el último tiene que estar"
        );
    }
}
