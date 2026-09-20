//! Dibujo del panel de notificaciones. La geometría (rect de cada fila, hit test) vive
//! en `menu/notifications.rs`: acá sólo se pinta lo que esa cuenta dice, así el click y
//! el dibujo no se pueden despegar (trampa 10).
//!
//! Tipografía: **título en negrilla** (700), cuerpo normal y **hora en itálica** (dim).
//! La itálica la rasteriza el canvas (`TextCache::get_italic`, un `font-style="italic"`
//! en el SVG); la negrilla ya viajaba por el peso.

use super::clipboard::fit;
use super::{
    accent, fill_rrect, menu_radius, panel_bg, rounded_rect_path, stroke_menu_border, text_dim_hex,
    text_hex,
};
use crate::menu::{self, MENU_PADDING, NOTIF_ROW_H, NotifyEntry, OVERLAY_RADIUS, PanelFrame};
use dockyrs_canvas::TextCache;
use tiny_skia::{Pixmap, Transform};

pub struct NotifArgs<'a> {
    pub settings: &'a crate::config::DockSettings,
    pub entries: &'a [NotifyEntry],
    /// Scroll en píxeles lógicos (lo clampea `menu::notif_max_scroll`).
    pub scroll: f32,
    pub hovered: Option<usize>,
    pub render_scale: f32,
    /// Caja del CONTENIDO del panel: el panel menos la banda de pestañas.
    pub frame: PanelFrame,
    pub is_vertical: bool,
    /// Índice de la pestaña activa (para la banda); `None` = sin banda.
    pub overlay_tabs: Option<usize>,
}

pub fn draw_notifications(pixmap: &mut Pixmap, text_cache: &mut TextCache, args: NotifArgs) {
    let s = args.render_scale;
    let settings = args.settings;
    let frame = args.frame;
    let (panel_w, panel_h) = menu::panel_size(frame, args.is_vertical);
    let (w, h) = (panel_w * s, panel_h * s);

    // ----- fondo y borde: los mismos helpers que el resto de los paneles -----
    let bg = panel_bg(settings);
    let path = rounded_rect_path(0.0, 0.0, w, h, menu_radius(settings, s));
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(bg.0, bg.1, bg.2, bg.3);
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    stroke_menu_border(pixmap, &path, settings, s);

    // ----- banda de pestañas: fila arriba en el panel ancho, columna al costado del
    // dock en el vertical -----
    if let Some(index) = args.overlay_tabs {
        super::draw_overlay_tabs(
            pixmap,
            text_cache,
            settings,
            panel_w,
            panel_h,
            s,
            index,
            args.is_vertical,
            frame.x > 0.0,
        );
    }

    let dim = text_dim_hex(settings);
    if args.entries.is_empty() {
        super::draw_text(
            pixmap,
            text_cache,
            "No notifications",
            (frame.x + MENU_PADDING) * s,
            (frame.y + MENU_PADDING + 18.0) * s,
            10.0 * s,
            &dim,
            400,
        );
        return;
    }

    let title_c = text_hex(settings);
    let acc = accent(settings);
    for (i, entry) in args.entries.iter().enumerate() {
        let (rx, ry, rw, rh) = menu::notif_row_rect(frame, i, args.scroll);
        // ----- recorte vertical: las filas fuera del panel no se pintan -----
        if ry + rh < frame.y || ry > frame.y + frame.h {
            continue;
        }
        if args.hovered == Some(i) {
            fill_rrect(
                pixmap,
                (rx + 4.0) * s,
                ry * s + 2.0 * s,
                (rw - 8.0) * s,
                (rh - 4.0) * s,
                OVERLAY_RADIUS * s,
                (255, 255, 255, 20),
            );
        }
        // ----- la hora en itálica, alineada a la derecha -----
        let at = if entry.at.is_empty() {
            None
        } else {
            text_cache.get_italic(&entry.at, 10.0 * s, &dim, 400)
        };
        let at_w = at.as_ref().map(|p| p.width() as f32 / s).unwrap_or(0.0);
        let right = if at_w > 0.0 { at_w + 8.0 } else { 0.0 };
        let avail = (rw - 2.0 * MENU_PADDING - right).max(24.0);

        // ----- título (negrilla) y cuerpo (normal, una sola línea) -----
        let title = fit(entry.title.trim(), &entry.title, 11.0, avail);
        super::draw_text(
            pixmap,
            text_cache,
            &title,
            (rx + MENU_PADDING) * s,
            (ry + 11.0) * s,
            11.0 * s,
            &title_c,
            700,
        );
        let cuerpo = entry.body.replace(['\n', '\t'], " ");
        let body = fit(cuerpo.trim(), &cuerpo, 10.0, avail);
        super::draw_text(
            pixmap,
            text_cache,
            &body,
            (rx + MENU_PADDING) * s,
            (ry + 26.0) * s,
            10.0 * s,
            &dim,
            400,
        );
        if let Some(at) = at {
            // ----- separador tenue + la hora, para que la fila no se lea como una
            // sola frase cuando el título es corto -----
            fill_rrect(
                pixmap,
                (rx + rw - MENU_PADDING - at_w - 4.0) * s,
                (ry + 8.0) * s,
                1.0 * s,
                (NOTIF_ROW_H - 16.0) * s,
                0.5 * s,
                (acc.0, acc.1, acc.2, 60),
            );
            pixmap.draw_pixmap(
                0,
                0,
                at.as_ref().as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::from_translate((rx + rw - MENU_PADDING - at_w) * s, (ry + 11.0) * s),
                None,
            );
        }
    }
}
