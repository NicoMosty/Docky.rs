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
        let label_len = text_width_estimate_render(label, 8.0 * render_scale);
        draw_text_rotated(
            pixmap,
            text_cache,
            label,
            zx + zw / 2.0,
            zy + zh / 2.0,
            8.0 * render_scale,
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
        let label_len = text_width_estimate_render(label, 8.0 * render_scale);
        let gap = 5.0 * render_scale;
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
            8.0 * render_scale,
            colors.text_dim_color,
            500,
        );
    } else {
        let label_w = text_width_estimate_render(label, 8.5 * render_scale);
        let gap = 6.0 * render_scale;
        let content_w = icon_r * 2.0 + gap + label_w;
        let block_x = zx + (zw - content_w) / 2.0;
        let cy = zy + zh / 2.0;
        let icon_cx = block_x + icon_r;
        draw_icon(pixmap, render_scale, icon_cx, cy, colors, active);
        if let Some(txt) = text_cache.get(label, 8.5 * render_scale, colors.text_dim_color, 500) {
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
    let s = 6.0 * render_scale;
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(cx - s, cy - s * 0.45);
    pb.line_to(cx - s * 0.25, cy - s * 0.45);
    pb.line_to(cx + s * 0.35, cy - s);
    pb.line_to(cx + s * 0.35, cy + s);
    pb.line_to(cx - s * 0.25, cy + s * 0.45);
    pb.line_to(cx - s, cy + s * 0.45);
    pb.close();
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        let alpha = if active { 230 } else { 110 };
        paint.set_color_rgba8(
            colors.text_rgb.0,
            colors.text_rgb.1,
            colors.text_rgb.2,
            alpha,
        );
        paint.anti_alias = true;
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    if active {
        let mut arcs = tiny_skia::PathBuilder::new();
        for (r, sweep) in [(s * 0.9, 0.9), (s * 1.5, 0.9)] {
            let steps = 10;
            for i in 0..=steps {
                let t = -sweep / 2.0 + sweep * (i as f32 / steps as f32);
                let x = cx + s * 0.55 + r * t.cos();
                let y = cy + r * t.sin();
                if i == 0 {
                    arcs.move_to(x, y);
                } else {
                    arcs.line_to(x, y);
                }
            }
        }
        if let Some(path) = arcs.finish() {
            stroke(pixmap, &path, render_scale, colors, true);
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
    let mut pb = tiny_skia::PathBuilder::new();
    for r in [3.0, 6.0, 9.0] {
        let r = r * render_scale;
        let steps = 12;
        for i in 0..=steps {
            let t = std::f32::consts::PI * (1.25 + 0.5 * (i as f32 / steps as f32));
            let x = cx + r * t.cos();
            let y = cy + r * 0.9 + r * t.sin() * 0.55 + 3.0 * render_scale;
            if i == 0 {
                pb.move_to(x, y);
            } else {
                pb.line_to(x, y);
            }
        }
    }
    // dot
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
    if let Some(rect) = tiny_skia::Rect::from_xywh(
        cx - 1.2 * render_scale,
        cy + 5.2 * render_scale,
        2.4 * render_scale,
        2.4 * render_scale,
    ) {
        let path = rounded_rect_path(
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height(),
            1.2 * render_scale,
        );
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
pub(super) fn draw_volume_widget(
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
    let Some((pct, muted)) = widgets.volume else {
        return;
    };
    let label = if muted {
        "MUTE".to_string()
    } else {
        format!("{pct}%")
    };
    draw_icon_label(
        pixmap,
        text_cache,
        draw_speaker_icon,
        6.0 * render_scale,
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
pub(super) fn draw_network_widget(
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
    let label = &widgets.network.label;
    draw_icon_label(
        pixmap,
        text_cache,
        draw_wifi_icon,
        6.0 * render_scale,
        widgets.network.online,
        label,
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
pub(super) fn draw_kblayout_widget(
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
