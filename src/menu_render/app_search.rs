use super::*;

/// `highlight` es la posición (x, y) de la pastilla del resaltado dentro del
/// viewport del strip. Son dos valores porque la grilla tiene dos dimensiones:
/// con uno solo la pastilla se dibujaría siempre en la primera columna o fila (lo
/// anima `tick_app_search_frame`, que interpola los dos).
pub fn draw_app_search(
    pixmap: &mut Pixmap,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    args: &DrawArgs,
    highlight: (f32, f32),
    content_anim: f32,
) {
    let s = args.render_scale;
    let w = args.panel_width * s;
    let h = args.content_height * s;
    let settings = &args.dock.config.settings;

    let bg = panel_bg(settings);
    let path = rounded_rect_path(0.0, 0.0, w, h, menu_radius(settings, s));
    let mut paint = Paint::default();
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

    let frame = args.dock.panel_frame(args.panel_width, args.content_height);
    // ----- la banda es fija; el cuerpo (caja de búsqueda + grilla) es lo que se
    // corre en un cambio de pestaña -----
    if let Some(index) = args.overlay_tabs {
        draw_overlay_tabs(
            pixmap,
            text_cache,
            settings,
            args.panel_width,
            args.content_height,
            s,
            index,
            args.dock.is_vertical(),
            args.dock.band_left(),
        );
    }
    draw_body(pixmap, args.slide_offset * s, args.body_opacity, |dst| {
        draw_control_rows(dst, icon_cache, text_cache, args.controls, args);
        draw_app_search_strip(
            dst,
            icon_cache,
            text_cache,
            args,
            frame,
            highlight,
            content_anim,
        );
    });
}

/// La grilla entera: row-major (se llena la fila antes de bajar, así el ranking
/// se lee de izquierda a derecha), con las tarjetas del tamaño de cada orientación
/// (dos columnas en el panel vertical) y scrolleando en el eje del reparto.
///
/// El reparto de cada tarjeta sale de `app_search_card_offset`, la MISMA función
/// que usa el hit test.
pub(crate) fn draw_app_search_strip(
    pixmap: &mut Pixmap,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    args: &DrawArgs,
    frame: crate::menu::PanelFrame,
    highlight: (f32, f32),
    content_anim: f32,
) {
    use crate::menu::{
        APP_CARD_ICON, APP_CARD_LABEL_GAP, APP_CARD_PAD, MENU_PADDING, app_search_card_offset,
        app_search_card_w, app_search_row_h, app_search_strip_h, app_search_strip_origin,
        app_search_strip_pos,
    };
    let s = args.render_scale;
    let settings = &args.dock.config.settings;
    let is_vertical = args.dock.is_vertical();
    let (ox0, oy0) = app_search_strip_origin(frame);
    let strip_h = app_search_strip_h(frame, is_vertical);
    // ----- el viewport del strip es el ancho del frame: las tarjetas van insetadas
    // por MENU_PADDING igual que la caja de búsqueda, y en el horizontal lo que
    // scrollea es la página entera (`app_search_viewport_along`) -----
    let viewport_w = frame.w * s;
    let viewport_h = strip_h * s;
    // ----- el scroll va en unidades lógicas: `app_search_strip_pos` es la MISMA
    // cuenta que usa el hit test, y la escala se aplica después -----
    let scroll = args.wallpaper_scroll_x;

    if args.app_entries.is_empty() {
        let ty = oy0 * s + centered_text_y(crate::menu::APP_CARD_ROW_H, 9.0) * s;
        draw_text(
            pixmap,
            text_cache,
            if is_vertical {
                "No matches"
            } else {
                "No matching apps"
            },
            (frame.x + MENU_PADDING) * s,
            ty,
            9.0 * s,
            &text_dim_hex(settings),
            500,
        );
        return;
    }

    let mut strip = Pixmap::new(
        viewport_w.round().max(1.0) as u32,
        viewport_h.round().max(1.0) as u32,
    )
    .unwrap();
    let card_w = app_search_card_w(is_vertical);
    let card_h = app_search_row_h(is_vertical);
    let icon_size = APP_CARD_ICON.min(card_w - 8.0).max(10.0) * s;
    // ----- la tarjeta puede ser más alta que el icono + la etiqueta (92 contra 66
    // en el panel vertical): el bloque va centrado, si no queda todo el aire abajo
    // y la pastilla se ve pesada. En el horizontal la cuenta da 0. -----
    let content_h =
        APP_CARD_PAD * 2.0 + APP_CARD_ICON + APP_CARD_LABEL_GAP + crate::menu::APP_CARD_LABEL_H;
    let block_pad = ((card_h - content_h) / 2.0).max(0.0);
    let max_chars = if is_vertical {
        (((card_w - 4.0) / (7.5 * 0.56)).floor().max(3.0)) as usize
    } else {
        13
    };
    // ----- la pastilla del resaltado: misma cuenta que las tarjetas (y que el
    // hit test), o se corre 10px de lo dibujado (trampa 10) -----
    let (px, py) = {
        let (rx, ry) = app_search_strip_pos(highlight, is_vertical, scroll);
        (rx * s, ry * s)
    };
    fill_rrect(
        &mut strip,
        px,
        py,
        card_w * s,
        card_h * s,
        crate::menu::OVERLAY_RADIUS * s,
        accent(settings),
    );

    let default_color = text_hex(settings);
    // ----- el color de la etiqueta sigue a la PASTILLA (`highlight`), no al hover:
    // al salir de las tarjetas el hover se borra pero la pastilla se queda, así que
    // la etiqueta de la seleccionada se encendía sola con la pastilla en otro lado
    // (el titileo del nombre). Ver `menu::app_search_hot_card`. -----
    let hot_index =
        crate::menu::app_search_hot_card(args.app_entries.len(), frame, is_vertical, highlight);
    for (i, entry) in args.app_entries.iter().enumerate() {
        let (ox, oy) = app_search_card_offset(i, frame, is_vertical);
        let (cx0, cy0) = {
            let (rx, ry) = app_search_strip_pos((ox, oy), is_vertical, scroll);
            (rx * s, ry * s)
        };
        if cx0 + card_w * s < 0.0 || cx0 > viewport_w || cy0 + card_h * s < 0.0 || cy0 > viewport_h
        {
            continue;
        }
        let hot = hot_index == Some(i);
        let icon_x = cx0 + (card_w * s - icon_size) / 2.0;
        let icon_y = cy0 + (block_pad + APP_CARD_PAD) * s;
        if let Some(icon_pixmap) = icon_cache.get(&entry.icon, icon_size.max(1.0) as u32) {
            let scale = icon_size / icon_pixmap.width() as f32;
            let paint = tiny_skia::PixmapPaint::default();
            strip.draw_pixmap(
                0,
                0,
                icon_pixmap.as_ref().as_ref(),
                &paint,
                Transform::from_translate(icon_x, icon_y).pre_scale(scale, scale),
                None,
            );
        } else {
            // ----- sin ícono en el tema (p. ej. `Advanced Network Configuration`,
            // que sólo existe en AdwaitaLegacy) la tarjeta quedaba vacía: el mismo
            // placeholder que usan las apps fijas del dock -----
            crate::render::draw_placeholder(
                &mut strip,
                &entry.name,
                icon_x + icon_size / 2.0,
                icon_y + icon_size / 2.0,
                icon_size,
            );
        }
        let hot_color;
        let color: &str = if hot {
            hot_color = on_accent_hex(settings);
            &hot_color
        } else {
            &default_color
        };
        let label = truncate_label(&entry.name, max_chars);
        let label_w = text_width_estimate(&label, 7.5) * s;
        let label_x = cx0 + (card_w * s - label_w) / 2.0;
        let label_y = icon_y + icon_size + APP_CARD_LABEL_GAP * s;
        draw_text(
            &mut strip,
            text_cache,
            &label,
            label_x,
            label_y,
            7.5 * s,
            color,
            500,
        );
    }

    let strip_paint = tiny_skia::PixmapPaint {
        opacity: content_anim,
        ..Default::default()
    };
    pixmap.draw_pixmap(
        0,
        0,
        strip.as_ref(),
        &strip_paint,
        Transform::from_translate(ox0 * s, oy0 * s),
        None,
    );
}
