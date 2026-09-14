//! Raster por CPU sobre tiny-skia: texto, iconos y miniaturas.
//!
//! Los tres modulos son el mismo concepto: una fuente (una corrida de texto,
//! un icono del tema, una imagen en disco) se rasteriza a un `Rc<Pixmap>` del
//! tamano pedido y se guarda en cache. Todo el resto -- subir el pixmap a un
//! buffer `wl_shm` y armar la superficie -- le toca a la aplicacion.
//!
//! Nada de aca sabe de docks, barras, ajustes ni atajos: el tema del icono, la
//! familia de fuente y las rutas entran por parametro. Es la regla que las
//! trampas 10 y 12 de AGENTS.md piden del otro lado (el reparto y el dibujo
//! tienen que salir de la misma funcion); aca la version es que ninguna
//! geometria depende de la escala del monitor, porque el pixmap ya viene en
//! pixeles fisicos.

mod icon_cache;
mod text;
mod thumbnail_cache;

pub use icon_cache::IconCache;
pub use text::{TextCache, list_font_families};
pub use thumbnail_cache::{ThumbnailCache, load as load_thumbnail};

use tiny_skia::Path;

/// Camino de rectangulo redondeado en el origen.
///
/// Estaba duplicado, identico, en `icon_cache` y en `thumbnail_cache`. El
/// tercer lugar donde vive esto -- `render::rounded_rect_path`, con firma
/// `(x, y, w, h, r)` -- se quedo en la app a proposito: consolidarlo implica
/// tocar un archivo del dibujo de la barra y no aporta a este crate.
pub(crate) fn rounded_rect_path(w: f32, h: f32, r: f32) -> Path {
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
