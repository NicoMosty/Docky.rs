use super::*;

// ----- un solo Pixmap de recorte por hilo, del tamaño que pida el último uso. Antes
// cada llamada alocaba y zero-llenaba uno (`Pixmap::new`) por frame, y con la carátula
// scrolleando eso es todos los frames (~30 fps, AUDIT.md D6). El dibujo corre entero en
// el hilo principal, así que un scratch por hilo no comparte estado con nadie; se queda
// con el tamaño más grande pedido hasta ahora para no re-alocar con cada resize.
// ponytail: si algún día el raster se reparte en varios hilos, esto tiene que pasar a
// ser un buffer por hilo de render (o por llamador).
thread_local! {
    static CLIP_SCRATCH: std::cell::RefCell<Option<Pixmap>> = const { std::cell::RefCell::new(None) };
}

/// Corre `f` con un pixmap del tamaño pedido, reusando el scratch del hilo: el
/// `Pixmap::new` por llamada (que además zero-llenaba el buffer) era el costo del
/// hallazgo D6. Se limpia a transparente y sólo se re-aloca cuando cambia el tamaño
/// (en régimen estable el título de la carátula mide siempre lo mismo).
fn con_scratch(w: u32, h: u32, f: impl FnOnce(&mut Pixmap)) {
    let (w, h) = (w.max(1), h.max(1));
    CLIP_SCRATCH.with(|slot| {
        let mut slot = slot.borrow_mut();
        let mismo_tamano = slot
            .as_ref()
            .is_some_and(|p| p.width() == w && p.height() == h);
        if !mismo_tamano {
            let Some(nuevo) = Pixmap::new(w, h) else {
                return;
            };
            *slot = Some(nuevo);
        }
        let Some(px) = slot.as_mut() else {
            return;
        };
        // ----- lo que hacía el `Pixmap::new`: dejarlo transparente -----
        px.data_mut().fill(0);
        f(px);
    });
}

pub(super) fn draw_text_clipped(
    pixmap: &mut Pixmap,
    glyphs: &Pixmap,
    x: f32,
    y: f32,
    avail_w: f32,
    offset: f32,
) {
    let avail_i = avail_w.round().max(1.0) as u32;
    con_scratch(avail_i, glyphs.height(), |clip| {
        tile_text(clip, glyphs, avail_w, offset);
        pixmap.draw_pixmap(
            0,
            0,
            clip.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::from_translate(x, y),
            None,
        );
    });
}

pub(super) fn draw_text_clipped_rotated(
    pixmap: &mut Pixmap,
    glyphs: &Pixmap,
    cx: f32,
    cy: f32,
    avail_len: f32,
    offset: f32,
) {
    let avail_i = avail_len.round().max(1.0) as u32;
    let h_i = glyphs.height();
    con_scratch(avail_i, h_i, |clip| {
        tile_text(clip, glyphs, avail_len, offset);
        let transform = Transform::from_translate(-(avail_i as f32) / 2.0, -(h_i as f32) / 2.0)
            .post_rotate(-90.0)
            .post_translate(cx, cy);
        pixmap.draw_pixmap(
            0,
            0,
            clip.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            transform,
            None,
        );
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_cover_art(
    pixmap: &mut Pixmap,
    icon_cache: &mut IconCache,
    art_path: Option<&str>,
    cx: f32,
    cy: f32,
    side: f32,
    playing: bool,
    colors: &WidgetColors,
) {
    let side_i = side.round().max(1.0) as u32;
    if let Some(path) = art_path
        && let Some(art) = icon_cache.get_rounded(path, side_i)
    {
        let x = cx - side_i as f32 / 2.0;
        let y = cy - side_i as f32 / 2.0;
        pixmap.draw_pixmap(
            0,
            0,
            art.as_ref().as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::from_translate(x, y),
            None,
        );
        return;
    }
    draw_media_glyph(pixmap, cx, cy, side_i as f32 * 0.28, playing, colors);
}

pub(super) fn draw_media_glyph(
    pixmap: &mut Pixmap,
    icon_cx: f32,
    icon_cy: f32,
    icon_r: f32,
    playing: bool,
    colors: &WidgetColors,
) {
    let mut icon_paint = Paint::default();
    icon_paint.set_color_rgba8(colors.accent.0, colors.accent.1, colors.accent.2, 255);
    icon_paint.anti_alias = true;
    let mut pb = tiny_skia::PathBuilder::new();
    if playing {
        let bar_w = icon_r * 0.35;
        if let Some(rect) = Rect::from_xywh(
            icon_cx - icon_r * 0.55,
            icon_cy - icon_r,
            bar_w,
            icon_r * 2.0,
        ) {
            pixmap.fill_rect(rect, &icon_paint, Transform::identity(), None);
        }
        if let Some(rect) = Rect::from_xywh(
            icon_cx + icon_r * 0.2,
            icon_cy - icon_r,
            bar_w,
            icon_r * 2.0,
        ) {
            pixmap.fill_rect(rect, &icon_paint, Transform::identity(), None);
        }
    } else {
        pb.move_to(icon_cx - icon_r * 0.6, icon_cy - icon_r);
        pb.line_to(icon_cx - icon_r * 0.6, icon_cy + icon_r);
        pb.line_to(icon_cx + icon_r, icon_cy);
        pb.close();
        if let Some(path) = pb.finish() {
            pixmap.fill_path(
                &path,
                &icon_paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) struct MediaLayout {
    art_cx: f32,
    art_cy: f32,
    art_side: f32,
    text_x: f32,
    text_w: f32,
    title_cy: f32,
}

pub(super) fn media_fixed_geometry(render_scale: f32) -> (f32, f32) {
    let art_side = 34.0 * render_scale;
    let gap1 = 8.0 * render_scale;
    (art_side, gap1)
}

pub(super) const MEDIA_MAX_TEXT_W: f32 = 22.0;
pub(super) const MEDIA_WIDE_TEXT_W: f32 = 65.0;
pub(super) const MEDIA_WIDE_THRESHOLD: f32 = 250.0;

pub(crate) fn media_ideal_len(is_vertical: bool, render_scale: f32, width_scale: f32) -> f32 {
    if is_vertical {
        let side = 26.0 * render_scale;
        side + 12.0 * render_scale + 60.0 * render_scale * width_scale
    } else {
        let (art_side, gap1) = media_fixed_geometry(render_scale);
        art_side + gap1 + MEDIA_WIDE_TEXT_W * render_scale * width_scale
    }
}

pub(super) fn media_layout(
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    mirrored: bool,
    bar_len: f32,
    width_scale: f32,
) -> MediaLayout {
    let art_side = (zh - 10.0 * render_scale).clamp(22.0 * render_scale, 34.0 * render_scale);
    let (_, gap1) = media_fixed_geometry(render_scale);

    let text_cap = if bar_len > MEDIA_WIDE_THRESHOLD * render_scale {
        MEDIA_WIDE_TEXT_W
    } else {
        MEDIA_MAX_TEXT_W
    };
    let text_cap = text_cap * width_scale;
    let avail = (zw - art_side - gap1).max(1.0);
    let content_w = (text_cap * render_scale).min(avail);
    let total_w = art_side + gap1 + content_w;
    let block_x = zx + (zw - total_w) / 2.0;

    let title_cy = zy + zh / 2.0;
    let art_cy = zy + zh / 2.0;

    let (art_cx, text_x) = if mirrored {
        (block_x + content_w + gap1 + art_side / 2.0, block_x)
    } else {
        (block_x + art_side / 2.0, block_x + art_side + gap1)
    };

    MediaLayout {
        art_cx,
        art_cy,
        art_side,
        text_x,
        text_w: content_w,
        title_cy,
    }
}

pub(super) fn draw_media_placeholder_art(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    side: f32,
    colors: &WidgetColors,
) {
    let path = rounded_rect_path(cx - side / 2.0, cy - side / 2.0, side, side, side * 0.22);
    let mut paint = Paint::default();
    paint.set_color_rgba8(18, 18, 22, 235);
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    draw_music_note_icon(pixmap, cx, cy, side * 0.30, colors);
}

pub(super) fn draw_music_note_icon(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    r: f32,
    colors: &WidgetColors,
) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(colors.text_rgb.0, colors.text_rgb.1, colors.text_rgb.2, 210);
    paint.anti_alias = true;

    let head_r = r * 0.26;
    let left_head = (cx - r * 0.55, cy + r * 0.6);
    let right_head = (cx + r * 0.5, cy + r * 0.15);

    let mut heads = tiny_skia::PathBuilder::new();
    heads.push_circle(left_head.0, left_head.1, head_r);
    heads.push_circle(right_head.0, right_head.1, head_r);
    if let Some(path) = heads.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let stem_w = r * 0.13;
    let beam_top_y = cy - r * 0.85;
    let beam_top_slant = beam_top_y - r * 0.12;
    let left_stem_x = left_head.0 + head_r * 0.8;
    let right_stem_x = right_head.0 + head_r * 0.8;

    let mut stems = tiny_skia::PathBuilder::new();
    if let Some(rect) = Rect::from_xywh(left_stem_x, beam_top_y, stem_w, left_head.1 - beam_top_y) {
        stems.push_rect(rect);
    }
    if let Some(rect) = Rect::from_xywh(
        right_stem_x,
        beam_top_slant,
        stem_w,
        right_head.1 - beam_top_slant,
    ) {
        stems.push_rect(rect);
    }
    if let Some(path) = stems.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let beam_h = r * 0.16;
    let beam_gap = r * 0.14;
    let mut beams = tiny_skia::PathBuilder::new();
    for i in 0..2 {
        let dy = i as f32 * (beam_h + beam_gap);
        beams.move_to(left_stem_x, beam_top_y + dy);
        beams.line_to(right_stem_x + stem_w, beam_top_slant + dy);
        beams.line_to(right_stem_x + stem_w, beam_top_slant + dy + beam_h);
        beams.line_to(left_stem_x, beam_top_y + dy + beam_h);
        beams.close();
    }
    if let Some(path) = beams.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

pub(super) fn media_vertical_icon(
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
) -> (f32, f32, f32) {
    let side = (zw - 14.0 * render_scale).clamp(16.0 * render_scale, 26.0 * render_scale);
    let title_budget = (zh - side - 22.0 * render_scale).max(20.0 * render_scale);
    let total = side + 12.0 * render_scale + title_budget;
    let block_start = zy + (zh - total) / 2.0;
    (zx + zw / 2.0, block_start + side / 2.0, side)
}

pub fn media_toggle_hit(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    x: f64,
    y: f64,
) -> bool {
    if widgets.media.is_none() {
        return false;
    }
    let is_vertical = dock.is_vertical();
    // ----- la misma escala que el resto de los hit tests: ver `hit_layout` -----
    let widget_scale = hit_scale(&dock.config.settings);
    // ----- `media_layout` espeja/clampa contra el largo de la barra, en las mismas
    // unidades que el reparto: el ancho base lógico -----
    let (base_w, _) = dock.base_size();
    let rects = hit_layout(dock, widgets, tray_count);
    let Some(r) = rects
        .into_iter()
        .find(|r| r.kind == crate::config::WidgetKind::Media)
    else {
        return false;
    };
    let width_scale = dock.config.settings.media_width_scale;
    let (cx, cy, side) = if is_vertical {
        media_vertical_icon(r.x, r.y, r.w, r.h, widget_scale)
    } else {
        let mirrored = r.slot == crate::config::WidgetSlot::Right;
        let layout = media_layout(
            r.x,
            r.y,
            r.w,
            r.h,
            widget_scale,
            mirrored,
            base_w as f32,
            width_scale,
        );
        (layout.art_cx, layout.art_cy, layout.art_side)
    };
    let hit_r = side / 2.0 * 1.2;
    let (dx, dy) = (x as f32 - cx, y as f32 - cy);
    dx * dx + dy * dy <= hit_r * hit_r
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_media_widget(
    pixmap: &mut Pixmap,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    marquee: &mut MarqueeState,
    advance: bool,
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    colors: &WidgetColors,
    is_vertical: bool,
    mirrored: bool,
    bar_len: f32,
    smooth_scroll: bool,
    width_scale: f32,
    text_px: f32,
) -> bool {
    if is_vertical {
        return draw_media_widget_vertical(
            pixmap,
            icon_cache,
            text_cache,
            widgets,
            marquee,
            advance,
            zx,
            zy,
            zw,
            zh,
            render_scale,
            colors,
            smooth_scroll,
            text_px,
        );
    }

    let media = widgets.media.as_ref();
    let title: &str = media
        .map(|m| m.title.as_str())
        .unwrap_or("Nothing is playing");
    let playing = media.map(|m| m.playing).unwrap_or(false);

    let layout = media_layout(zx, zy, zw, zh, render_scale, mirrored, bar_len, width_scale);

    match media {
        Some(m) => draw_cover_art(
            pixmap,
            icon_cache,
            m.art_path.as_deref(),
            layout.art_cx,
            layout.art_cy,
            layout.art_side,
            playing,
            colors,
        ),
        None => draw_media_placeholder_art(
            pixmap,
            layout.art_cx,
            layout.art_cy,
            layout.art_side,
            colors,
        ),
    }

    let title_size = text_px;
    let glyphs = text_cache.get(title, title_size, colors.text_color, 600);
    let title_w = glyphs.as_ref().map(|p| p.width() as f32).unwrap_or(0.0);
    let (title_offset, title_anim) = marquee_step(
        &mut marquee.title,
        title_w,
        layout.text_w,
        advance,
        render_scale,
        smooth_scroll,
    );
    if let Some(px) = &glyphs {
        let ty = layout.title_cy - px.height() as f32 / 2.0;
        draw_text_clipped(pixmap, px, layout.text_x, ty, layout.text_w, title_offset);
    }

    title_anim
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_media_widget_vertical(
    pixmap: &mut Pixmap,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    marquee: &mut MarqueeState,
    advance: bool,
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    colors: &WidgetColors,
    smooth_scroll: bool,
    text_px: f32,
) -> bool {
    let media = widgets.media.as_ref();
    let title: &str = media
        .map(|m| m.title.as_str())
        .unwrap_or("Nothing is playing");
    let playing = media.map(|m| m.playing).unwrap_or(false);

    let (cx, icon_cy, side) = media_vertical_icon(zx, zy, zw, zh, render_scale);

    let title_size = text_px;
    let title_budget = (zh - side - 22.0 * render_scale).max(20.0 * render_scale);
    let glyphs = text_cache.get(title, title_size, colors.text_color, 600);
    let title_w = glyphs.as_ref().map(|p| p.width() as f32).unwrap_or(0.0);
    let (title_offset, title_anim) = marquee_step(
        &mut marquee.title,
        title_w,
        title_budget,
        advance,
        render_scale,
        smooth_scroll,
    );

    match media {
        Some(m) => draw_cover_art(
            pixmap,
            icon_cache,
            m.art_path.as_deref(),
            cx,
            icon_cy,
            side,
            playing,
            colors,
        ),
        None => draw_media_placeholder_art(pixmap, cx, icon_cy, side, colors),
    }

    if let Some(px) = &glyphs {
        let ty = icon_cy + side / 2.0 + 12.0 * render_scale + title_budget / 2.0;
        draw_text_clipped_rotated(pixmap, px, cx, ty, title_budget, title_offset);
    }

    title_anim
}

#[cfg(test)]
mod clip_scratch_tests {
    use super::*;

    fn glifos() -> Pixmap {
        let mut px = Pixmap::new(4, 3).expect("glifos");
        for b in px.data_mut().chunks_exact_mut(4) {
            b.copy_from_slice(&[255, 255, 255, 255]);
        }
        px
    }

    fn tinta(px: &Pixmap) -> usize {
        px.data().chunks_exact(4).filter(|c| c[3] > 0).count()
    }

    /// D6: el scratch se reusa y se limpia entre llamadas. Lo que tiene que atrapar este
    /// test es el fantasma: si el buffer compartido no se limpiara, dibujar con el texto
    /// fuera de vista dejaría la tinta de la llamada anterior.
    #[test]
    fn el_scratch_no_deja_fantasmas() {
        let glyphs = glifos();
        let mut dst = Pixmap::new(8, 3).expect("dst");
        // ----- con el glifo a la vista hay tinta, y dibujar dos veces da lo mismo -----
        draw_text_clipped(&mut dst, &glyphs, 0.0, 0.0, 8.0, 0.0);
        let primera: Vec<u8> = dst.data().to_vec();
        assert!(tinta(&dst) > 0, "el glifo tiene que dibujarse");
        dst.data_mut().fill(0);
        draw_text_clipped(&mut dst, &glyphs, 0.0, 0.0, 8.0, 0.0);
        assert_eq!(dst.data(), primera.as_slice(), "determinista");
        // ----- con el glifo lejos, el pixmap de destino queda vacío: el scratch se limpió
        // (sin la limpieza, acá quedaría la tinta de la llamada de arriba) -----
        dst.data_mut().fill(0);
        draw_text_clipped(&mut dst, &glyphs, 0.0, 0.0, 8.0, 1000.0);
        assert_eq!(tinta(&dst), 0, "no puede quedar tinta vieja");
        // ----- y también el de la versión rotada (comparte el mismo scratch) -----
        let mut dst2 = Pixmap::new(8, 8).expect("dst2");
        draw_text_clipped_rotated(&mut dst2, &glyphs, 4.0, 4.0, 8.0, 0.0);
        assert!(tinta(&dst2) > 0, "la rotada también dibuja");
        dst2.data_mut().fill(0);
        draw_text_clipped_rotated(&mut dst2, &glyphs, 4.0, 4.0, 8.0, 1000.0);
        assert_eq!(tinta(&dst2), 0, "y tampoco deja fantasmas");
    }
}
