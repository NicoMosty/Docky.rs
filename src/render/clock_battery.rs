use super::*;

// ----- color por estado: verde cargando, naranja en conservacion,
// rojo si queda poca, tenue si descarga con normalidad. Tiñe contorno,
// relleno y numero para que el estado se lea de un vistazo -----
fn battery_tone(pct: u8, state: BatteryState, colors: &WidgetColors) -> ((u8, u8, u8), u8) {
    match state {
        BatteryState::Charging => ((74, 222, 128), 150),
        BatteryState::Conserving => ((255, 159, 64), 150),
        BatteryState::Discharging if pct <= 20 => ((255, 69, 58), 150),
        BatteryState::Discharging => (
            (colors.text_rgb.0, colors.text_rgb.1, colors.text_rgb.2),
            70,
        ),
    }
}

fn draw_battery_icon(
    pixmap: &mut Pixmap,
    render_scale: f32,
    bx: f32,
    by: f32,
    bw: f32,
    bh: f32,
    pct: u8,
    tone: (u8, u8, u8),
    fill_alpha: u8,
) {
    // ----- contorno y nub en el color del estado -----
    let mut outline = Paint::default();
    outline.set_color_rgba8(tone.0, tone.1, tone.2, 235);
    outline.anti_alias = true;
    let path = rounded_rect_path(bx, by, bw, bh, 3.0 * render_scale);
    let stroke = tiny_skia::Stroke {
        width: 1.4 * render_scale,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &outline, &stroke, Transform::identity(), None);
    // ----- nub del polo positivo -----
    let nub_h = bh * 0.42;
    if let Some(rect) = Rect::from_xywh(
        bx + bw + 1.0 * render_scale,
        by + (bh - nub_h) / 2.0,
        2.0 * render_scale,
        nub_h,
    ) {
        pixmap.fill_rect(rect, &outline, Transform::identity(), None);
    }

    // ----- relleno proporcional con el mismo tono -----
    let inset = 2.6 * render_scale;
    let fill_w = ((bw - inset * 2.0) * (pct as f32 / 100.0)).max(0.0);
    let inner_h = bh - inset * 2.0;
    if fill_w > 0.6 && inner_h > 0.0 {
        let mut fill = Paint::default();
        fill.set_color_rgba8(tone.0, tone.1, tone.2, fill_alpha);
        fill.anti_alias = true;
        let fill_path =
            rounded_rect_path(bx + inset, by + inset, fill_w, inner_h, 1.5 * render_scale);
        pixmap.fill_path(
            &fill_path,
            &fill,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

pub(crate) fn draw_clock_widget(
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
    if is_vertical {
        let time_size = 11.0 * render_scale;
        let date_size = time_size;
        let clock_gap = 3.0 * render_scale;
        let time_px = text_cache.get(&widgets.time, time_size, colors.text_color, 700);
        let date_px = text_cache.get_with_family(
            &widgets.date,
            date_size,
            colors.text_color,
            500,
            DATE_FONT_FAMILY,
        );
        // ----- rotation swap -----
        let time_cross = time_px.as_ref().map(|p| p.height() as f32).unwrap_or(0.0);
        let date_cross = date_px.as_ref().map(|p| p.height() as f32).unwrap_or(0.0);
        let total_cross = time_cross + clock_gap + date_cross;
        let block_start = zx + (zw - total_cross) / 2.0;
        let cy = zy + zh / 2.0;
        let time_cx = block_start + time_cross / 2.0;
        let date_cx = block_start + time_cross + clock_gap + date_cross / 2.0;
        draw_text_rotated(
            pixmap,
            text_cache,
            &widgets.time,
            time_cx,
            cy,
            time_size,
            colors.text_color,
            700,
        );
        draw_text_rotated_family(
            pixmap,
            text_cache,
            &widgets.date,
            date_cx,
            cy,
            date_size,
            colors.text_color,
            500,
            DATE_FONT_FAMILY,
        );
    } else {
        let time_size = 12.0 * render_scale;
        let date_size = time_size;
        let clock_gap = 3.5 * render_scale;
        let time_px = text_cache.get(&widgets.time, time_size, colors.text_color, 700);
        let date_px = text_cache.get_with_family(
            &widgets.date,
            date_size,
            colors.text_color,
            500,
            DATE_FONT_FAMILY,
        );
        let cx = zx + zw / 2.0;
        let cy = zy + zh / 2.0;
        // ----- una sola línea: hora + fecha lado a lado -----
        let time_w = time_px.as_ref().map(|p| p.width() as f32).unwrap_or(0.0);
        let date_w = date_px.as_ref().map(|p| p.width() as f32).unwrap_or(0.0);
        let mut dx = cx - (time_w + clock_gap + date_w) / 2.0;
        if let Some(time_px) = time_px {
            let ty = cy - time_px.height() as f32 / 2.0;
            pixmap.draw_pixmap(
                0,
                0,
                time_px.as_ref().as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::from_translate(dx, ty),
                None,
            );
            dx += time_px.width() as f32 + clock_gap;
        }
        if let Some(date_px) = date_px {
            let dy = cy - date_px.height() as f32 / 2.0;
            pixmap.draw_pixmap(
                0,
                0,
                date_px.as_ref().as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::from_translate(dx, dy),
                None,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_battery_widget(
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
    let Some((pct, state)) = widgets.battery else {
        return;
    };
    let (tone, fill_alpha) = battery_tone(pct, state, colors);
    let tone_hex = format!("#{:02x}{:02x}{:02x}", tone.0, tone.1, tone.2);
    let label = format!("{pct}%");
    if is_vertical {
        let bw = 18.0 * render_scale;
        let bh = 9.0 * render_scale;
        let label_len = text_width_estimate_render(&label, 9.5 * render_scale);
        let gap = 5.0 * render_scale;
        let total = bh + gap + label_len;
        let by = zy + (zh - total) / 2.0;
        let bx = zx + zw / 2.0 - bw / 2.0;
        draw_battery_icon(pixmap, render_scale, bx, by, bw, bh, pct, tone, fill_alpha);
        let ty = by + bh + gap + label_len / 2.0;
        draw_text_rotated(
            pixmap,
            text_cache,
            &label,
            zx + zw / 2.0,
            ty,
            9.5 * render_scale,
            &tone_hex,
            600,
        );
    } else {
        // ----- el porcentaje va DENTRO del icono -----
        let bw = 30.0 * render_scale;
        let bh = 15.0 * render_scale;
        let bx = zx + (zw - bw) / 2.0;
        let by = zy + zh / 2.0 - bh / 2.0;
        draw_battery_icon(pixmap, render_scale, bx, by, bw, bh, pct, tone, fill_alpha);
        let fs = 9.0 * render_scale;
        if let Some(txt) = text_cache.get(&label, fs, &tone_hex, 700) {
            let tx = bx + (bw - txt.width() as f32) / 2.0;
            let ty = by + (bh - txt.height() as f32) / 2.0;
            let paint = tiny_skia::PixmapPaint::default();
            // ----- contorno oscuro: el % se lee tambien sobre el relleno verde -----
            if let Some(shadow) = text_cache.get(&label, fs, "#25212B", 700) {
                for (ox, oy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    pixmap.draw_pixmap(
                        0,
                        0,
                        shadow.as_ref().as_ref(),
                        &paint,
                        Transform::from_translate(tx + ox, ty + oy),
                        None,
                    );
                }
            }
            pixmap.draw_pixmap(
                0,
                0,
                txt.as_ref().as_ref(),
                &paint,
                Transform::from_translate(tx, ty),
                None,
            );
        }
    }
}

#[cfg(test)]
mod battery_tone_tests {
    use super::*;

    fn tone(pct: u8, state: BatteryState) -> (u8, u8, u8) {
        let colors = WidgetColors {
            accent: (0, 0, 0, 0),
            text_rgb: (235, 235, 240),
            text_color: "#ebebf0",
        };
        battery_tone(pct, state, &colors).0
    }

    /// El color del icono y del numero depende del estado de carga.
    #[test]
    fn color_por_estado() {
        assert_eq!(tone(58, BatteryState::Charging), (74, 222, 128));
        assert_eq!(tone(58, BatteryState::Conserving), (255, 159, 64));
        assert_eq!(tone(15, BatteryState::Discharging), (255, 69, 58));
        assert_eq!(tone(58, BatteryState::Discharging), (235, 235, 240));
    }
}
