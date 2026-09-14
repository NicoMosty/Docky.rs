use super::*;

/// Índice del icono del tray para una coordenada a lo largo del bloque. Los
/// huecos entre iconos caen al más cercano: antes un click en el hueco no
/// devolvía nada y el click derecho sobre el tray parecía roto.
fn nearest_tray_index(rel: f32, per: f32, count: usize) -> usize {
    ((rel.max(0.0) / per).floor() as usize).min(count - 1)
}

pub fn tray_icon_hit(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    x: f64,
    y: f64,
) -> Option<usize> {
    if tray_count == 0 {
        return None;
    }
    let is_vertical = dock.is_vertical();
    let rects = hit_layout(dock, widgets, tray_count);
    let r = rects
        .into_iter()
        .find(|r| r.kind == crate::config::WidgetKind::Tray)?;
    let (_, per, content_len) = tray_geometry(
        tray_count,
        is_vertical,
        r.w,
        r.h,
        hit_scale(&dock.config.settings),
    );
    let (xf, yf) = (x as f32, y as f32);
    if is_vertical {
        if xf < r.x || xf >= r.x + r.w {
            return None;
        }
        let block_start = r.y + (r.h - content_len) / 2.0;
        Some(nearest_tray_index(yf - block_start, per, tray_count))
    } else {
        if yf < r.y || yf >= r.y + r.h {
            return None;
        }
        let block_start = r.x + (r.w - content_len) / 2.0;
        Some(nearest_tray_index(xf - block_start, per, tray_count))
    }
}

pub fn tray_icon_center(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    idx: usize,
) -> Option<(f32, f32)> {
    if idx >= tray_count {
        return None;
    }
    let is_vertical = dock.is_vertical();
    let rects = hit_layout(dock, widgets, tray_count);
    let r = rects
        .into_iter()
        .find(|r| r.kind == crate::config::WidgetKind::Tray)?;
    let (icon_side, per, content_len) = tray_geometry(
        tray_count,
        is_vertical,
        r.w,
        r.h,
        hit_scale(&dock.config.settings),
    );
    if is_vertical {
        let block_start = r.y + (r.h - content_len) / 2.0;
        Some((
            r.x + r.w / 2.0,
            block_start + idx as f32 * per + icon_side / 2.0,
        ))
    } else {
        let block_start = r.x + (r.w - content_len) / 2.0;
        Some((
            block_start + idx as f32 * per + icon_side / 2.0,
            r.y + r.h / 2.0,
        ))
    }
}
pub(crate) fn tray_geometry(
    tray_count: usize,
    is_vertical: bool,
    zw: f32,
    zh: f32,
    render_scale: f32,
) -> (f32, f32, f32) {
    let icon_side = (16.0 * render_scale).min(if is_vertical { zw } else { zh } * 0.7);
    let gap = 10.0 * render_scale;
    let per = icon_side + gap;
    let content_len = per * tray_count as f32 - gap;
    (icon_side, per, content_len)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_tray_widget(
    pixmap: &mut Pixmap,
    icon_cache: &mut IconCache,
    tray: &[crate::tray::TrayIcon],
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    is_vertical: bool,
) {
    if tray.is_empty() {
        return;
    }
    let (icon_side, per, content_len) =
        tray_geometry(tray.len(), is_vertical, zw, zh, render_scale);
    if is_vertical {
        let block_start = zy + (zh - content_len) / 2.0;
        let cx = zx + zw / 2.0;
        for (i, item) in tray.iter().enumerate() {
            let cy = block_start + i as f32 * per + icon_side / 2.0;
            draw_tray_icon(pixmap, icon_cache, item, cx, cy, icon_side);
        }
    } else {
        let block_start = zx + (zw - content_len) / 2.0;
        let cy = zy + zh / 2.0;
        for (i, item) in tray.iter().enumerate() {
            let cx = block_start + i as f32 * per + icon_side / 2.0;
            draw_tray_icon(pixmap, icon_cache, item, cx, cy, icon_side);
        }
    }
}

pub(super) fn draw_tray_icon(
    pixmap: &mut Pixmap,
    icon_cache: &mut IconCache,
    item: &crate::tray::TrayIcon,
    cx: f32,
    cy: f32,
    icon_side: f32,
) {
    let side_i = icon_side.round().max(1.0) as u32;
    if !item.icon_name.is_empty()
        && let Some(icon) = icon_cache.get(&item.icon_name, side_i)
    {
        let x = cx - side_i as f32 / 2.0;
        let y = cy - side_i as f32 / 2.0;
        pixmap.draw_pixmap(
            0,
            0,
            icon.as_ref().as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::from_translate(x, y),
            None,
        );
        return;
    }
    if let Some(raw) = &item.icon_pixmap {
        let scale = icon_side / raw.width().max(1) as f32;
        let x = cx - icon_side / 2.0;
        let y = cy - icon_side / 2.0;
        pixmap.draw_pixmap(
            0,
            0,
            raw.as_ref().as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::from_translate(x, y).pre_scale(scale, scale),
            None,
        );
        return;
    }
    draw_placeholder(pixmap, &item.service, cx, cy, icon_side);
}

#[cfg(test)]
mod tray_hit_tests {
    use super::nearest_tray_index;

    const ICON: f32 = 16.0;
    const PER: f32 = ICON + 10.0;

    #[test]
    fn dentro_de_cada_icono() {
        // centro del icono 0, 1 y 2
        assert_eq!(nearest_tray_index(0.0, PER, 3), 0);
        assert_eq!(nearest_tray_index(PER + 8.0, PER, 3), 1);
        assert_eq!(nearest_tray_index(PER * 2.0 + ICON - 0.1, PER, 3), 2);
    }

    #[test]
    fn el_hueco_cae_al_icono_mas_cercano() {
        // 24 está en el hueco entre el icono 0 (0..16) y el 1 (26..42):
        // antes devolvía None y el click se perdía
        assert_eq!(nearest_tray_index(24.0, PER, 3), 0);
        // 27 también es hueco; cae al 1 porque ya pasó el inicio de su celda
        assert_eq!(nearest_tray_index(27.0, PER, 3), 1);
        // y por izquierda del bloque, al primero
        assert_eq!(nearest_tray_index(-5.0, PER, 3), 0);
    }

    #[test]
    fn el_extremo_derecho_no_se_pasa() {
        assert_eq!(nearest_tray_index(PER * 5.0, PER, 3), 2);
    }
}
