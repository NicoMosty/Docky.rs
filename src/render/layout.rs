use super::*;

pub(super) struct WidgetRect {
    pub(super) kind: crate::config::WidgetKind,
    pub(super) slot: crate::config::WidgetSlot,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

pub(super) const WIDGET_GAP: f32 = 6.0;
pub(super) const ZONE_GAP: f32 = 18.0;

// ----- flexible zones -----
pub(super) fn layout_widgets(
    settings: &crate::config::DockSettings,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    is_vertical: bool,
    w: f32,
    h: f32,
    render_scale: f32,
) -> Vec<WidgetRect> {
    use crate::config::WidgetSlot;
    let bar_len = if is_vertical { h } else { w };
    let cross_len = if is_vertical { w } else { h };
    let gap = WIDGET_GAP * render_scale;
    let zone_gap = ZONE_GAP * render_scale;
    let inset = 10.0 * render_scale;

    let members_of = |slot: WidgetSlot| -> Vec<crate::config::WidgetKind> {
        settings
            .widgets
            .iter()
            .filter(|p| p.slot == slot)
            .map(|p| p.kind)
            .collect()
    };
    let lens_of = |members: &[crate::config::WidgetKind]| -> Vec<f32> {
        members
            .iter()
            .map(|&kind| {
                widget_natural_len(
                    kind,
                    widgets,
                    tray_count,
                    is_vertical,
                    cross_len,
                    render_scale,
                    settings.media_width_scale,
                )
                .max(1.0)
            })
            .collect()
    };
    let block_len = |lens: &[f32]| -> f32 {
        lens.iter().sum::<f32>() + gap * (lens.len() as f32 - 1.0).max(0.0)
    };

    let place = |slot: WidgetSlot,
                 members: Vec<crate::config::WidgetKind>,
                 lens: Vec<f32>,
                 block_start: f32|
     -> Vec<WidgetRect> {
        let mut pos = block_start;
        members
            .into_iter()
            .zip(lens)
            .map(|(kind, len)| {
                let start = pos;
                pos += len + gap;
                if is_vertical {
                    WidgetRect {
                        kind,
                        slot,
                        x: 0.0,
                        y: start,
                        w,
                        h: len,
                    }
                } else {
                    WidgetRect {
                        kind,
                        slot,
                        x: start,
                        y: 0.0,
                        w: len,
                        h,
                    }
                }
            })
            .collect()
    };

    let left_members = members_of(WidgetSlot::Left);
    let left_lens = lens_of(&left_members);
    let left_len = block_len(&left_lens);
    let left_end = inset + left_len;

    let right_members = members_of(WidgetSlot::Right);
    let right_lens = lens_of(&right_members);
    let right_len = block_len(&right_lens);
    let right_start = bar_len - inset - right_len;

    let mid_members = members_of(WidgetSlot::Middle);
    let mid_lens = lens_of(&mid_members);
    let mid_len = block_len(&mid_lens);
    let segment_start = if left_len > 0.0 {
        left_end + zone_gap
    } else {
        inset
    };
    let segment_end = if right_len > 0.0 {
        right_start - zone_gap
    } else {
        bar_len - inset
    };
    // ----- zona media siempre centrada en su espacio libre -----
    let gap_center = segment_start + (segment_end - segment_start - mid_len) / 2.0;
    let mid_start = gap_center.clamp(
        segment_start.min(segment_end - mid_len),
        segment_end - mid_len,
    );

    let mut out = place(WidgetSlot::Left, left_members, left_lens, inset);
    out.extend(place(WidgetSlot::Middle, mid_members, mid_lens, mid_start));
    out.extend(place(
        WidgetSlot::Right,
        right_members,
        right_lens,
        right_start,
    ));
    out
}

// ----- mirrors layout -----
pub fn widget_bar_natural_len(
    settings: &crate::config::DockSettings,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    is_vertical: bool,
    cross_len: f32,
    render_scale: f32,
) -> f32 {
    use crate::config::WidgetSlot;
    if settings.widgets.is_empty() {
        return 0.0;
    }
    let gap = WIDGET_GAP * render_scale;
    let zone_gap = ZONE_GAP * render_scale;
    let inset = 10.0 * render_scale;

    let zone_len = |slot: WidgetSlot| -> f32 {
        let lens: Vec<f32> = settings
            .widgets
            .iter()
            .filter(|p| p.slot == slot)
            .map(|p| {
                widget_natural_len(
                    p.kind,
                    widgets,
                    tray_count,
                    is_vertical,
                    cross_len,
                    render_scale,
                    settings.media_width_scale,
                )
                .max(1.0)
            })
            .collect();
        if lens.is_empty() {
            0.0
        } else {
            lens.iter().sum::<f32>() + gap * (lens.len() as f32 - 1.0)
        }
    };

    let zones = [
        zone_len(WidgetSlot::Left),
        zone_len(WidgetSlot::Middle),
        zone_len(WidgetSlot::Right),
    ];
    let present: Vec<f32> = zones.into_iter().filter(|&len| len > 0.0).collect();
    present.iter().sum::<f32>() + zone_gap * (present.len() as f32 - 1.0).max(0.0) + inset * 2.0
}

pub(super) fn widget_natural_len(
    kind: crate::config::WidgetKind,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    is_vertical: bool,
    cross_len: f32,
    render_scale: f32,
    media_width_scale: f32,
) -> f32 {
    use crate::config::WidgetKind;
    match kind {
        WidgetKind::Clock => {
            if is_vertical {
                let time_w = text_width_estimate_render(&widgets.time, 11.0 * render_scale);
                let date_w = text_width_estimate_render(&widgets.date, 11.0 * render_scale);
                time_w.max(date_w)
            } else {
                let time_w = text_width_estimate_render(&widgets.time, 12.0 * render_scale);
                let date_w = text_width_estimate_render(&widgets.date, 12.0 * render_scale);
                time_w + 3.5 * render_scale + date_w
            }
        }
        WidgetKind::Battery => {
            let Some((pct, _)) = widgets.battery else {
                return 0.0;
            };
            let label = format!("{pct}%");
            if is_vertical {
                let label_len = text_width_estimate_render(&label, 9.5 * render_scale);
                9.0 * render_scale + 5.0 * render_scale + label_len
            } else {
                // ----- el porcentaje va dentro del icono: sólo el ancho de éste -----
                32.0 * render_scale
            }
        }
        WidgetKind::Media => media_ideal_len(is_vertical, render_scale, media_width_scale),
        WidgetKind::PowerMenu => 14.0 * render_scale,
        WidgetKind::Bluetooth => {
            // ----- sólo el icono: el estado se expresa con color, sin texto -----
            16.0 * render_scale
        }
        WidgetKind::Tray => {
            if tray_count == 0 {
                0.0
            } else {
                tray_geometry(tray_count, is_vertical, cross_len, cross_len, render_scale).2
            }
        }
        WidgetKind::Workspaces => workspaces_geometry(&widgets.workspaces, render_scale),
        WidgetKind::Cpu => percentage_widget_len(widgets.cpu, is_vertical, render_scale),
        WidgetKind::Ram => {
            let label = match widgets.ram_gb {
                Some((used, total)) => format!("{:.1} / {:.0} GB", used, total),
                None => match widgets.ram {
                    Some(pct) => format!("{pct}%"),
                    None => return 0.0,
                },
            };
            text_widget_len(&label, is_vertical, render_scale)
        }
        WidgetKind::Network => 24.0 * render_scale,
        WidgetKind::Volume => {
            let label = match widgets.volume {
                Some((_, true)) => "MUTE".to_string(),
                Some((pct, false)) => format!("{pct}%"),
                None => return 0.0,
            };
            // ----- icono compacto (4.7) + hueco corto (4) y número grande (10) -----
            9.4 * render_scale
                + 4.0 * render_scale
                + text_width_estimate_render(&label, 10.0 * render_scale)
        }
        WidgetKind::KbdLayout => {
            let w = text_width_estimate_render(&widgets.kblayout.short, 8.5 * render_scale);
            w + 10.0 * render_scale
        }
    }
}

pub(super) fn text_widget_len(label: &str, is_vertical: bool, render_scale: f32) -> f32 {
    let icon_side = 12.0 * render_scale;
    if is_vertical {
        icon_side + 5.0 * render_scale + text_width_estimate_render(label, 8.5 * render_scale)
    } else {
        icon_side + 6.0 * render_scale + text_width_estimate_render(label, 8.5 * render_scale)
    }
}
pub(super) fn percentage_widget_len(pct: Option<u8>, is_vertical: bool, render_scale: f32) -> f32 {
    let Some(pct) = pct else { return 0.0 };
    text_widget_len(&format!("{pct}%"), is_vertical, render_scale)
}
pub(super) fn draw_widgets(
    pixmap: &mut Pixmap,
    dock: &Dock,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    tray: &[crate::tray::TrayIcon],
    marquee: &mut MarqueeState,
    advance: bool,
    advance_ws: bool,
    render_scale: f32,
) -> bool {
    use crate::config::WidgetKind;

    let s = &dock.config.settings;
    let (base_w, base_h) = dock.base_size();
    let w = base_w as f32 * render_scale;
    let h = base_h as f32 * render_scale;
    // ----- widget scale -----
    let render_scale = render_scale * s.widget_scale;
    let palette = widget_palette(s);
    let colors = WidgetColors {
        accent: palette.accent,
        text_rgb: palette.text_rgb,
        text_color: &palette.text_color,
    };

    let key = widgets
        .media
        .as_ref()
        .map(|m| m.title.clone())
        .unwrap_or_else(|| "Nothing is playing".to_string());
    if marquee.key != key {
        marquee.key = key;
        marquee.title = MarqueeLane::default();
    }

    let is_vertical = dock.is_vertical();
    let tray_count = tray.len();
    let mut animating = false;
    for r in layout_widgets(s, widgets, tray_count, is_vertical, w, h, render_scale) {
        match r.kind {
            WidgetKind::Clock => draw_clock_widget(
                pixmap,
                text_cache,
                widgets,
                r.x,
                r.y,
                r.w,
                r.h,
                render_scale,
                &colors,
                is_vertical,
            ),
            WidgetKind::Battery => draw_battery_widget(
                pixmap,
                text_cache,
                widgets,
                r.x,
                r.y,
                r.w,
                r.h,
                render_scale,
                &colors,
                is_vertical,
            ),
            WidgetKind::Media => {
                let mirrored = r.slot == crate::config::WidgetSlot::Right;
                let bar_len = if is_vertical { h } else { w };
                animating |= draw_media_widget(
                    pixmap,
                    icon_cache,
                    text_cache,
                    widgets,
                    marquee,
                    advance,
                    r.x,
                    r.y,
                    r.w,
                    r.h,
                    render_scale,
                    &colors,
                    is_vertical,
                    mirrored,
                    bar_len,
                    s.media_smooth_scroll,
                    s.media_width_scale,
                );
            }
            WidgetKind::PowerMenu => draw_power_widget(
                pixmap,
                r.x,
                r.y,
                r.w,
                r.h,
                render_scale,
                &colors,
                is_vertical,
            ),
            WidgetKind::Bluetooth => {
                // ----- sin etiqueta: el estado se expresa con el color del icono -----
                let (powered, in_use) = match &widgets.bluetooth {
                    Some(bt) => (bt.powered, bt.powered && bt.connected.is_some()),
                    None => (false, false),
                };
                draw_bluetooth_icon(
                    pixmap,
                    render_scale,
                    r.x + r.w / 2.0,
                    r.y + r.h / 2.0,
                    powered,
                    in_use,
                    &colors,
                );
            }
            WidgetKind::Tray => draw_tray_widget(
                pixmap,
                icon_cache,
                tray,
                r.x,
                r.y,
                r.w,
                r.h,
                render_scale,
                is_vertical,
            ),
            WidgetKind::Workspaces => {
                draw_workspaces_widget(
                    pixmap,
                    &widgets.workspaces,
                    marquee,
                    advance_ws,
                    r.x,
                    r.y,
                    r.w,
                    r.h,
                    render_scale,
                    &colors,
                    is_vertical,
                );
            }
            WidgetKind::Cpu => draw_cpu_widget(
                pixmap,
                text_cache,
                widgets,
                r.x,
                r.y,
                r.w,
                r.h,
                render_scale,
                &colors,
                is_vertical,
            ),
            WidgetKind::Ram => draw_ram_widget(
                pixmap,
                text_cache,
                widgets,
                r.x,
                r.y,
                r.w,
                r.h,
                render_scale,
                &colors,
                is_vertical,
            ),
            WidgetKind::Network => draw_network_widget(
                pixmap,
                text_cache,
                widgets,
                r.x,
                r.y,
                r.w,
                r.h,
                render_scale,
                &colors,
                is_vertical,
                dock.hovered_widget == Some(WidgetKind::Network),
            ),
            WidgetKind::Volume => draw_volume_widget(
                pixmap,
                text_cache,
                widgets,
                r.x,
                r.y,
                r.w,
                r.h,
                render_scale,
                &colors,
                is_vertical,
            ),
            WidgetKind::KbdLayout => draw_kblayout_widget(
                pixmap,
                text_cache,
                widgets,
                r.x,
                r.y,
                r.w,
                r.h,
                render_scale,
                &colors,
                is_vertical,
            ),
        }
    }
    // ----- separadores sutiles entre zonas -----
    {
        use crate::config::WidgetSlot;
        let rects = layout_widgets(
            &dock.config.settings,
            widgets,
            tray_count,
            dock.is_vertical(),
            w,
            h,
            render_scale,
        );
        // (min, max) por zona sobre el eje principal; max < 0 = vacía
        let mut bounds = [(0.0f32, -1.0f32); 3];
        for r in &rects {
            let i = match r.slot {
                WidgetSlot::Left => 0,
                WidgetSlot::Middle => 1,
                WidgetSlot::Right => 2,
            };
            let (a, b) = if dock.is_vertical() {
                (r.y, r.y + r.h)
            } else {
                (r.x, r.x + r.w)
            };
            if bounds[i].1 < 0.0 {
                bounds[i] = (a, b);
            } else {
                bounds[i].0 = bounds[i].0.min(a);
                bounds[i].1 = bounds[i].1.max(b);
            }
        }
        let mut div_paint = Paint::default();
        div_paint.set_color_rgba8(colors.text_rgb.0, colors.text_rgb.1, colors.text_rgb.2, 35);
        div_paint.anti_alias = true;
        let mut prev: Option<(f32, f32)> = None;
        for i in [0, 1, 2] {
            if bounds[i].1 < 0.0 {
                continue;
            }
            if let Some((_, pmx)) = prev {
                let s = (pmx + bounds[i].0) / 2.0;
                let rc = if dock.is_vertical() {
                    let m = 4.0 * render_scale;
                    tiny_skia::Rect::from_xywh(m, s - 0.5, (w - 2.0 * m).max(0.0), 1.0)
                } else {
                    let m = 4.0 * render_scale;
                    tiny_skia::Rect::from_xywh(s - 0.5, m, 1.0, (h - 2.0 * m).max(0.0))
                };
                if let Some(rc) = rc {
                    pixmap.fill_rect(rc, &div_paint, Transform::identity(), None);
                }
            }
            prev = Some(bounds[i]);
        }
    }
    // ----- pill flotante con el SSID al pasar el mouse -----
    if dock.hovered_widget == Some(WidgetKind::Network) {
        draw_network_hover_pill(
            pixmap,
            dock,
            text_cache,
            widgets,
            tray_count,
            w,
            h,
            render_scale,
            &colors,
        );
    }
    animating
}

#[allow(clippy::too_many_arguments)]
fn draw_network_hover_pill(
    pixmap: &mut Pixmap,
    dock: &Dock,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    w: f32,
    h: f32,
    render_scale: f32,
    colors: &WidgetColors,
) {
    use crate::config::WidgetKind;
    let label = widgets.network.label.trim();
    if label.is_empty() {
        return;
    }
    let Some(rect) = layout_widgets(
        &dock.config.settings,
        widgets,
        tray_count,
        dock.is_vertical(),
        w,
        h,
        render_scale,
    )
    .into_iter()
    .find(|r| r.kind == WidgetKind::Network) else {
        return;
    };
    let fs = 8.5 * render_scale;
    let Some(txt) = text_cache.get(label, fs, colors.text_color, 500) else {
        return;
    };
    let pad_x = 7.0 * render_scale;
    let pad_y = 4.0 * render_scale;
    let pw = txt.width() as f32 + pad_x * 2.0;
    let ph = txt.height() as f32 + pad_y * 2.0;
    let margin = 3.0 * render_scale;
    let px = (rect.x + rect.w / 2.0 - pw / 2.0).clamp(margin, (w - pw - margin).max(margin));
    let py = (rect.y + rect.h / 2.0 - ph / 2.0).clamp(0.0, (h - ph).max(0.0));
    let path = rounded_rect_path(px, py, pw, ph, 6.0 * render_scale);
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
    pixmap.draw_pixmap(
        0,
        0,
        txt.as_ref().as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::from_translate(px + pad_x, py + pad_y),
        None,
    );
}

pub fn widget_hit_test(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    x: f64,
    y: f64,
) -> Option<crate::config::WidgetKind> {
    let (base_w, base_h) = dock.base_size();
    let is_vertical = dock.is_vertical();
    let rects = layout_widgets(
        &dock.config.settings,
        widgets,
        tray_count,
        is_vertical,
        base_w as f32,
        base_h as f32,
        1.0,
    );
    let (xf, yf) = (x as f32, y as f32);
    rects
        .into_iter()
        .find(|r| xf >= r.x && xf < r.x + r.w && yf >= r.y && yf < r.y + r.h)
        .map(|r| r.kind)
}

pub fn widget_center(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    kind: crate::config::WidgetKind,
) -> Option<(f32, f32)> {
    let (base_w, base_h) = dock.base_size();
    let is_vertical = dock.is_vertical();
    let rects = layout_widgets(
        &dock.config.settings,
        widgets,
        tray_count,
        is_vertical,
        base_w as f32,
        base_h as f32,
        1.0,
    );
    let r = rects.into_iter().find(|r| r.kind == kind)?;
    Some((r.x + r.w / 2.0, r.y + r.h / 2.0))
}
