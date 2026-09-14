//! Dibujo del panel de volumen (geometría en `menu::volume_panel`).

use super::*;
use crate::menu::{
    VOLUME_ICON_ZONE, VOLUME_ROW_H, volume_track_cy, volume_track_x0, volume_track_x1,
};

/// Fila de la salida o de un stream: icono (click = mute), etiqueta, % y barra.
pub(crate) fn draw_volume_row(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    control: &Control,
    index: usize,
    args: &DrawArgs,
    y: f32,
) {
    let s = args.render_scale;
    let settings = &args.dock.config.settings;
    let Some(row) = args.volume_rows.get(index) else {
        return;
    };
    let row = row.clone();

    let icon_cx = (MENU_PADDING + VOLUME_ICON_ZONE / 2.0) * s;
    let icon_cy = (control.y + VOLUME_ROW_H * 0.28) * s;
    if matches!(args.hovered, Some(HitTarget::VolumeMute(i)) if i == index) {
        fill_circle(
            pixmap,
            icon_cx,
            icon_cy,
            VOLUME_ICON_ZONE * 0.5 * s,
            HOVER_SOFT,
        );
    }
    let icon_color = if row.muted {
        (settings.text_r, settings.text_g, settings.text_b, 120)
    } else {
        accent(settings)
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(icon_color.0, icon_color.1, icon_color.2, icon_color.3);
    paint.anti_alias = true;
    draw_volume_icon(pixmap, icon_cx, icon_cy, 5.0 * s, &paint, row.muted, s);

    draw_text(
        pixmap,
        text_cache,
        &row.label,
        (MENU_PADDING + VOLUME_ICON_ZONE + 4.0) * s,
        y,
        9.0 * s,
        &text_hex(settings),
        600,
    );
    draw_value_rtl(
        pixmap,
        text_cache,
        &format!("{}%", row.pct),
        (args.panel_width - MENU_PADDING) * s,
        y,
        9.0 * s,
        &text_dim_hex(settings),
        500,
    );

    let cy = volume_track_cy(control.y) * s;
    let x0 = volume_track_x0() * s;
    let x1 = volume_track_x1(args.panel_width) * s;
    let track_h = 3.0 * s;
    fill_rrect(
        pixmap,
        x0,
        cy - track_h / 2.0,
        x1 - x0,
        track_h,
        track_h / 2.0,
        track_bg(settings),
    );
    let t = (row.pct as f32 / 100.0).clamp(0.0, 1.0);
    let fill_w = (x1 - x0) * t;
    fill_rrect(
        pixmap,
        x0,
        cy - track_h / 2.0,
        fill_w,
        track_h,
        track_h / 2.0,
        accent(settings),
    );
    let hot = matches!(args.hovered, Some(HitTarget::VolumeTrack(i)) if i == index);
    let thumb_r = if hot { 6.5 } else { 5.5 } * s;
    fill_circle(pixmap, x0 + fill_w, cy, thumb_r + 1.0 * s, KNOB_BORDER);
    fill_circle(pixmap, x0 + fill_w, cy, thumb_r, (255, 255, 255, 255));
}

/// Fila del selector de salida: el dispositivo por defecto va en acento con un
/// punto a la derecha (no un "✓": no dependemos del glifo de la fuente).
pub(crate) fn draw_volume_device(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    control: &Control,
    index: usize,
    args: &DrawArgs,
    y: f32,
) {
    let s = args.render_scale;
    let settings = &args.dock.config.settings;
    let Some(dev) = args.volume_devices.get(index) else {
        return;
    };
    let dev = dev.clone();
    let hot = matches!(args.hovered, Some(HitTarget::VolumeDevice(i)) if i == index);
    if hot {
        fill_rrect(
            pixmap,
            MENU_PADDING * s,
            y,
            (args.panel_width - 2.0 * MENU_PADDING) * s,
            control.height * s,
            6.0 * s,
            HOVER_SOFT,
        );
    }
    let color = if dev.default {
        accent_hex(settings)
    } else {
        text_dim_hex(settings)
    };
    draw_text(
        pixmap,
        text_cache,
        &dev.label,
        MENU_PADDING * s,
        y + centered_text_y(control.height, 9.0) * s,
        9.0 * s,
        &color,
        if dev.default { 700 } else { 500 },
    );
    if dev.default {
        fill_circle(
            pixmap,
            (args.panel_width - MENU_PADDING - 3.0) * s,
            y + control.height * 0.5 * s,
            3.0 * s,
            accent(settings),
        );
    }
}
