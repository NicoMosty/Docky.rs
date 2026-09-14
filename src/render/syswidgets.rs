use super::*;

fn draw_centered_label(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    label: &str,
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    colors: &WidgetColors,
    is_vertical: bool,
) {
    if label.is_empty() {
        return;
    }
    if is_vertical {
        let label_len = text_width_estimate_render(label, 8.5 * render_scale);
        draw_text_rotated(
            pixmap,
            text_cache,
            label,
            zx + zw / 2.0,
            zy + zh / 2.0,
            8.5 * render_scale,
            colors.text_color,
            600,
        );
        let _ = label_len;
    } else if let Some(txt) = text_cache.get(label, 8.5 * render_scale, colors.text_color, 600) {
        let tx = zx + (zw - txt.width() as f32) / 2.0;
        let ty = zy + (zh - txt.height() as f32) / 2.0;
        pixmap.draw_pixmap(
            0,
            0,
            txt.as_ref().as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::from_translate(tx, ty),
            None,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_icon_label(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    draw_icon: fn(&mut Pixmap, f32, f32, f32, &WidgetColors, bool),
    icon_r: f32,
    active: bool,
    label: &str,
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    colors: &WidgetColors,
    is_vertical: bool,
) {
    if is_vertical {
        let label_len = text_width_estimate_render(label, VOLUME_LABEL_PX * render_scale);
        let gap = VOLUME_ICON_GAP * render_scale;
        let total = icon_r * 2.0 + gap + label_len;
        let block_start = zy + (zh - total) / 2.0;
        let cx = zx + zw / 2.0;
        draw_icon(
            pixmap,
            render_scale,
            cx,
            block_start + icon_r,
            colors,
            active,
        );
        draw_text_rotated(
            pixmap,
            text_cache,
            label,
            cx,
            block_start + icon_r * 2.0 + gap + label_len / 2.0,
            VOLUME_LABEL_PX * render_scale,
            colors.text_color,
            600,
        );
    } else {
        // ----- centrado con el ancho REAL del texto, no con la estimación: la
        // estimación subestima un par de px y el contenido quedaba pegado al
        // borde izquierdo de la pastilla (padding 1px izq / 4px der) -----
        let Some(txt) = text_cache.get(
            label,
            VOLUME_LABEL_PX * render_scale,
            colors.text_color,
            600,
        ) else {
            return;
        };
        let label_w = txt.width() as f32;
        let gap = VOLUME_ICON_GAP * render_scale;
        let content_w = icon_r * 2.0 + gap + label_w;
        let block_x = zx + (zw - content_w) / 2.0;
        let cy = zy + zh / 2.0;
        let icon_cx = block_x + icon_r;
        draw_icon(pixmap, render_scale, icon_cx, cy, colors, active);
        pixmap.draw_pixmap(
            0,
            0,
            txt.as_ref().as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::from_translate(icon_cx + icon_r + gap, cy - txt.height() as f32 / 2.0),
            None,
        );
    }
}

fn stroke(
    pixmap: &mut Pixmap,
    path: &tiny_skia::Path,
    render_scale: f32,
    colors: &WidgetColors,
    active: bool,
) {
    let mut paint = Paint::default();
    let alpha = if active { 230 } else { 110 };
    paint.set_color_rgba8(
        colors.text_rgb.0,
        colors.text_rgb.1,
        colors.text_rgb.2,
        alpha,
    );
    paint.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width: 1.6 * render_scale,
        line_cap: tiny_skia::LineCap::Round,
        ..Default::default()
    };
    pixmap.stroke_path(path, &paint, &stroke, Transform::identity(), None);
}

fn draw_speaker_icon(
    pixmap: &mut Pixmap,
    render_scale: f32,
    cx: f32,
    cy: f32,
    colors: &WidgetColors,
    active: bool,
) {
    // ----- altavoz compacto: caja + cono y UNA sola onda. Antes llevaba dos
    // ondas y era ~30% más ancho; así el espacio ahorrado rinde para el número -----
    // ----- `s` atado a VOLUME_ICON_R a propósito: el cuerpo llega a -0.95s y la
    // onda a ~+0.85s, o sea que el icono mide ~1.8s de ancho y tiene que entrar en
    // la caja de 2*VOLUME_ICON_R que le reservan el reparto y `draw_icon_label`. Con
    // el s fijo viejo (5.2) medía ~10 unidades contra las 6 reservadas y se salía
    // de la pastilla por la izquierda. -----
    let s = VOLUME_ICON_R * render_scale;
    let alpha = if active { 230 } else { 110 };
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        colors.text_rgb.0,
        colors.text_rgb.1,
        colors.text_rgb.2,
        alpha,
    );
    paint.anti_alias = true;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(cx - s * 0.95, cy - s * 0.34);
    pb.line_to(cx - s * 0.34, cy - s * 0.34);
    pb.line_to(cx + s * 0.26, cy - s * 0.92);
    pb.line_to(cx + s * 0.26, cy + s * 0.92);
    pb.line_to(cx - s * 0.34, cy + s * 0.34);
    pb.line_to(cx - s * 0.95, cy + s * 0.34);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    if active {
        // ----- una onda pegada al cono -----
        let mut arcs = tiny_skia::PathBuilder::new();
        let r = s * 0.55;
        let steps = 14;
        let sweep = 1.15;
        for i in 0..=steps {
            let t = -sweep + 2.0 * sweep * (i as f32 / steps as f32);
            let x = cx + s * 0.30 + r * t.cos();
            let y = cy + r * t.sin();
            if i == 0 {
                arcs.move_to(x, y);
            } else {
                arcs.line_to(x, y);
            }
        }
        if let Some(path) = arcs.finish() {
            let mut wave = Paint::default();
            wave.set_color_rgba8(colors.text_rgb.0, colors.text_rgb.1, colors.text_rgb.2, 200);
            wave.anti_alias = true;
            let stroke = tiny_skia::Stroke {
                width: 1.35 * render_scale,
                line_cap: tiny_skia::LineCap::Round,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &wave, &stroke, Transform::identity(), None);
        }
    }
}

fn draw_wifi_icon(
    pixmap: &mut Pixmap,
    render_scale: f32,
    cx: f32,
    cy: f32,
    colors: &WidgetColors,
    active: bool,
) {
    // ----- arcos concéntricos simétricos + punto circular -----
    let mut pb = tiny_skia::PathBuilder::new();
    for r in [4.0, 7.0, 10.0] {
        let r = r * render_scale;
        let steps = 16;
        for i in 0..=steps {
            let phi = (-50.0 + 100.0 * (i as f32 / steps as f32)).to_radians();
            let x = cx + r * phi.sin();
            let y = cy + 2.5 * render_scale - r * phi.cos();
            if i == 0 {
                pb.move_to(x, y);
            } else {
                pb.line_to(x, y);
            }
        }
    }
    if let Some(path) = pb.finish() {
        stroke(pixmap, &path, render_scale, colors, active);
    }
    let mut dot = Paint::default();
    let alpha = if active { 230 } else { 110 };
    dot.set_color_rgba8(
        colors.text_rgb.0,
        colors.text_rgb.1,
        colors.text_rgb.2,
        alpha,
    );
    dot.anti_alias = true;
    let mut db = tiny_skia::PathBuilder::new();
    db.push_circle(cx, cy + 5.5 * render_scale, 1.6 * render_scale);
    if let Some(path) = db.finish() {
        pixmap.fill_path(
            &path,
            &dot,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_volume_widget(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    colors: &WidgetColors,
    is_vertical: bool,
    hovered: bool,
) {
    let Some((pct, muted)) = widgets.volume else {
        return;
    };
    // ----- sin el "%": el número pelado alcanza y deja el botón compacto -----
    let label = if muted {
        "MUTE".to_string()
    } else {
        pct.to_string()
    };
    draw_widget_button_bg(pixmap, zx, zy, zw, zh, render_scale, colors, hovered);
    draw_icon_label(
        pixmap,
        text_cache,
        draw_speaker_icon,
        VOLUME_ICON_R * render_scale,
        !muted,
        &label,
        zx,
        zy,
        zw,
        zh,
        render_scale,
        colors,
        is_vertical,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_network_widget(
    pixmap: &mut Pixmap,
    _text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    colors: &WidgetColors,
    _is_vertical: bool,
    hovered: bool,
) {
    // ----- botón solo-icono; el nombre va en pill flotante al hover -----
    draw_widget_button_bg(pixmap, zx, zy, zw, zh, render_scale, colors, hovered);
    draw_wifi_icon(
        pixmap,
        render_scale,
        zx + zw / 2.0,
        zy + zh / 2.0,
        colors,
        widgets.network.online,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_kblayout_widget(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    colors: &WidgetColors,
    is_vertical: bool,
) {
    draw_centered_label(
        pixmap,
        text_cache,
        &widgets.kblayout.short,
        zx,
        zy,
        zw,
        zh,
        render_scale,
        colors,
        is_vertical,
    );
}
