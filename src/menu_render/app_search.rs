use super::*;

/// `highlight` es la posición (x, y) de la pastilla del resaltado dentro del
/// contenido del strip. Son dos valores porque la grilla del panel horizontal
/// tiene dos filas: con uno solo la pastilla se dibujaría siempre en la de
/// arriba (lo anima `tick_app_search_frame`, que interpola los dos).
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

    // ----- la banda es fija; el cuerpo (caja de búsqueda + grilla) es lo que se
    // corre en un cambio de pestaña -----
    if let Some(index) = args.overlay_tabs {
        draw_overlay_tabs(
            pixmap,
            text_cache,
            settings,
            args.panel_width,
            s,
            index,
            args.dock.is_vertical(),
        );
    }
    draw_body(pixmap, args.slide_offset * s, args.body_opacity, |dst| {
        draw_control_rows(dst, icon_cache, text_cache, args.controls, args);
        draw_app_search_strip(dst, icon_cache, text_cache, args, highlight, content_anim);
    });
}

pub(crate) fn draw_app_search_strip(
    pixmap: &mut Pixmap,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    args: &DrawArgs,
    highlight: (f32, f32),
    content_anim: f32,
) {
    if args.dock.is_vertical() {
        draw_app_search_strip_vertical(
            pixmap,
            icon_cache,
            text_cache,
            args,
            highlight,
            content_anim,
        );
    } else {
        draw_app_search_strip_horizontal(
            pixmap,
            icon_cache,
            text_cache,
            args,
            highlight,
            content_anim,
        );
    }
}

/// Grilla de `APP_SEARCH_ROWS` filas x `app_search_cols` columnas, row-major, que
/// scrollea de a páginas. El reparto de cada tarjeta sale de
/// `app_search_card_offset`, la MISMA que usa el hit test.
pub(crate) fn draw_app_search_strip_horizontal(
    pixmap: &mut Pixmap,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    args: &DrawArgs,
    highlight: (f32, f32),
    content_anim: f32,
) {
    use crate::menu::{
        APP_CARD_ICON, APP_CARD_LABEL_GAP, APP_CARD_PAD, APP_CARD_ROW_H, APP_CARD_W,
        app_search_card_offset, app_search_strip_h, app_search_strip_y,
    };
    let s = args.render_scale;
    let settings = &args.dock.config.settings;
    let row_y = app_search_strip_y(false) * s;
    let strip_h = app_search_strip_h(false) * s;
    // ----- el strip va insetado por MENU_PADDING igual que la caja de búsqueda;
    // el ancho que tiene que coincidir con esto es `app_search_viewport_along`. -----
    let inset = MENU_PADDING * s;
    let viewport_w = (args.panel_width * s - inset * 2.0).max(1.0);
    let scroll = args.wallpaper_scroll_x * s;

    if args.app_entries.is_empty() {
        let ty = row_y + centered_text_y(APP_CARD_ROW_H, 9.0) * s;
        draw_text(
            pixmap,
            text_cache,
            "No matching apps",
            MENU_PADDING * s,
            ty,
            9.0 * s,
            &text_dim_hex(settings),
            500,
        );
        return;
    }

    let mut strip = Pixmap::new(
        viewport_w.round().max(1.0) as u32,
        strip_h.round().max(1.0) as u32,
    )
    .unwrap();
    fill_rrect(
        &mut strip,
        highlight.0 * s - scroll,
        highlight.1 * s,
        APP_CARD_W * s,
        APP_CARD_ROW_H * s,
        crate::menu::OVERLAY_RADIUS * s,
        accent(settings),
    );

    let default_color = text_hex(settings);
    // ----- el color de la etiqueta sigue a la PASTILLA (`highlight`), no al hover:
    // al salir de las tarjetas el hover se borra pero la pastilla se queda, así que
    // la etiqueta de la seleccionada se encendía sola con la pastilla en otro lado
    // (el titileo del nombre). Ver `menu::app_search_hot_card`. -----
    let hot_index = crate::menu::app_search_hot_card(
        args.app_entries.len(),
        args.panel_width,
        false,
        highlight,
    );
    for (i, entry) in args.app_entries.iter().enumerate() {
        let (ox, oy) = app_search_card_offset(i, args.panel_width, false);
        let cx0 = ox * s - scroll;
        let cy0 = oy * s;
        if cx0 + APP_CARD_W * s < 0.0
            || cx0 > viewport_w
            || cy0 + APP_CARD_ROW_H * s < 0.0
            || cy0 > strip_h
        {
            continue;
        }
        let hot = hot_index == Some(i);
        let icon_size = APP_CARD_ICON * s;
        let icon_x = cx0 + (APP_CARD_W * s - icon_size) / 2.0;
        let icon_y = cy0 + APP_CARD_PAD * s;
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
        }
        let hot_color;
        let color: &str = if hot {
            hot_color = on_accent_hex(settings);
            &hot_color
        } else {
            &default_color
        };
        let label = truncate_label(&entry.name, 13);
        let label_w = text_width_estimate(&label, 7.5) * s;
        let label_x = cx0 + (APP_CARD_W * s - label_w) / 2.0;
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
        Transform::from_translate(inset, row_y),
        None,
    );
}

/// Panel vertical: una tarjeta por fila, scrolleando en y.
pub(crate) fn draw_app_search_strip_vertical(
    pixmap: &mut Pixmap,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    args: &DrawArgs,
    highlight: (f32, f32),
    content_anim: f32,
) {
    use crate::menu::{
        APP_CARD_ICON, APP_CARD_LABEL_GAP, APP_CARD_PAD, APP_CARD_W, MENU_PADDING,
        app_search_card_along, app_search_strip_y,
    };
    let s = args.render_scale;
    let settings = &args.dock.config.settings;
    let row_y0 = app_search_strip_y(true) * s;
    let panel_w_px = args.panel_width * s;
    let card_w = (panel_w_px - MENU_PADDING * 2.0 * s).max(1.0);
    let card_h = APP_CARD_W * s;
    let viewport_h = (args.content_height * s - row_y0).max(1.0);
    let scroll = args.wallpaper_scroll_x * s;

    if args.app_entries.is_empty() {
        let ty = row_y0 + 14.0 * s;
        draw_text(
            pixmap,
            text_cache,
            "No matches",
            MENU_PADDING * s,
            ty,
            8.0 * s,
            &text_dim_hex(settings),
            500,
        );
        return;
    }

    let mut strip = Pixmap::new(
        panel_w_px.round().max(1.0) as u32,
        viewport_h.round().max(1.0) as u32,
    )
    .unwrap();
    let card_x = (panel_w_px - card_w) / 2.0;
    // ----- acá el resaltado se mueve sólo en y; la x la fija la columna -----
    let hl_y = highlight.1 * s - scroll;
    fill_rrect(
        &mut strip,
        card_x,
        hl_y,
        card_w,
        card_h,
        crate::menu::OVERLAY_RADIUS * s,
        accent(settings),
    );

    let icon_size = APP_CARD_ICON.min(card_w / s - 8.0).max(10.0) * s;
    let max_chars = (((card_w / s - 4.0) / (7.5 * 0.56)).floor().max(3.0)) as usize;
    let default_color = text_hex(settings);
    // ----- ver el strip horizontal: el color sigue a la pastilla, no al hover -----
    let hot_index =
        crate::menu::app_search_hot_card(args.app_entries.len(), args.panel_width, true, highlight);
    for (i, entry) in args.app_entries.iter().enumerate() {
        let cy0 = app_search_card_along(i, args.panel_width, true) * s - scroll;
        if cy0 + card_h < 0.0 || cy0 > viewport_h {
            continue;
        }
        let hot = hot_index == Some(i);
        let icon_x = card_x + (card_w - icon_size) / 2.0;
        let icon_y = cy0 + APP_CARD_PAD * s;
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
        let label_x = card_x + (card_w - label_w) / 2.0;
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
        Transform::from_translate(0.0, row_y0),
        None,
    );
}
