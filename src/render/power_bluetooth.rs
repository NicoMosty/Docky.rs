use super::*;

pub(super) fn draw_power_icon(
    pixmap: &mut Pixmap,
    render_scale: f32,
    cx: f32,
    cy: f32,
    colors: &WidgetColors,
) {
    let r = 7.0 * render_scale;
    let mut paint = Paint::default();
    paint.set_color_rgba8(colors.text_rgb.0, colors.text_rgb.1, colors.text_rgb.2, 230);
    paint.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width: 1.6 * render_scale,
        line_cap: tiny_skia::LineCap::Round,
        ..Default::default()
    };

    let gap = 27.0_f32.to_radians();
    let steps = 28;
    let mut ring = tiny_skia::PathBuilder::new();
    for i in 0..=steps {
        let t = gap + (std::f32::consts::TAU - 2.0 * gap) * (i as f32 / steps as f32);
        let x = cx + r * t.sin();
        let y = cy - r * t.cos();
        if i == 0 {
            ring.move_to(x, y);
        } else {
            ring.line_to(x, y);
        }
    }
    if let Some(path) = ring.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    let mut nub = tiny_skia::PathBuilder::new();
    nub.move_to(cx, cy - r - 1.6 * render_scale);
    nub.line_to(cx, cy - r * 0.45);
    if let Some(path) = nub.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

pub(super) fn draw_power_widget(
    pixmap: &mut Pixmap,
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    colors: &WidgetColors,
    _is_vertical: bool,
) {
    draw_power_icon(pixmap, render_scale, zx + zw / 2.0, zy + zh / 2.0, colors);
}

pub(super) fn draw_bluetooth_icon(
    pixmap: &mut Pixmap,
    render_scale: f32,
    cx: f32,
    cy: f32,
    powered: bool,
    in_use: bool,
    colors: &WidgetColors,
) {
    // ----- color por estado (sin palabras): verde si está en uso, color del
    // texto si está encendido, tenue si está apagado -----
    let (r, g, b, alpha) = if in_use {
        (74, 222, 128, 255)
    } else if powered {
        (colors.text_rgb.0, colors.text_rgb.1, colors.text_rgb.2, 235)
    } else {
        (colors.text_rgb.0, colors.text_rgb.1, colors.text_rgb.2, 90)
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, alpha);
    paint.anti_alias = true;
    // ----- mismo trazo que los arcos del icono de wifi -----
    let stroke = tiny_skia::Stroke {
        width: 1.6 * render_scale,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    };
    let s = 7.0 * render_scale;
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(cx, cy - s);
    pb.line_to(cx + s * 0.62, cy - s * 0.38);
    pb.line_to(cx - s * 0.62, cy + s * 0.38);
    pb.line_to(cx, cy + s);
    pb.line_to(cx, cy - s);
    pb.move_to(cx - s * 0.62, cy - s * 0.38);
    pb.line_to(cx + s * 0.62, cy + s * 0.38);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    // ----- apagado: barra diagonal, para que se lea sin la palabra "Off" -----
    if !powered {
        let mut slash = Paint::default();
        slash.set_color_rgba8(255, 69, 58, 215);
        slash.anti_alias = true;
        let mut sb = tiny_skia::PathBuilder::new();
        sb.move_to(cx - s * 0.8, cy + s * 0.8);
        sb.line_to(cx + s * 0.8, cy - s * 0.8);
        if let Some(path) = sb.finish() {
            pixmap.stroke_path(&path, &slash, &stroke, Transform::identity(), None);
        }
    }
}
