use super::overlay_tabs_h;

pub const WALLPAPER_BACK_ZONE_W: f32 = 30.0;
/// El filmstrip usa el mismo inset y la misma separación que los otros paneles
/// del overlay (la caja de búsqueda del launcher y del portapapeles), y las
/// miniaturas el mismo radio (`OVERLAY_RADIUS`).
pub const WALLPAPER_PADDING: f32 = super::MENU_PADDING;
pub const WALLPAPER_GAP: f32 = super::MENU_PADDING;
pub const WALLPAPER_ASPECT: f32 = 1.6;

// ----- always landscape -----
pub fn wallpaper_thumb_size(panel_w: f32, panel_h: f32, is_vertical: bool) -> (f32, f32) {
    if is_vertical {
        let w = (panel_w - WALLPAPER_PADDING * 2.0).max(1.0);
        (w, w / WALLPAPER_ASPECT)
    } else {
        // ----- la banda de pestañas se come alto: el panel crece con ella al
        // abrirse, así que se descuenta y la miniatura queda del mismo tamaño -----
        let h = (panel_h - overlay_tabs_h(false) - WALLPAPER_PADDING * 2.0).max(1.0);
        (h * WALLPAPER_ASPECT, h)
    }
}

pub fn wallpaper_content_len(count: usize, panel_w: f32, panel_h: f32, is_vertical: bool) -> f32 {
    let (tw, th) = wallpaper_thumb_size(panel_w, panel_h, is_vertical);
    let along = if is_vertical { th } else { tw };
    (count as f32 * (along + WALLPAPER_GAP) - WALLPAPER_GAP).max(0.0)
}

pub fn wallpaper_max_scroll(count: usize, panel_w: f32, panel_h: f32, is_vertical: bool) -> f32 {
    // ----- en el panel vertical la banda (apilada) corre el filmstrip hacia
    // abajo, así que el tramo visible es más corto -----
    let along = if is_vertical {
        panel_h - overlay_tabs_h(true)
    } else {
        panel_w
    };
    let viewport_along = (along - WALLPAPER_BACK_ZONE_W).max(1.0);
    (wallpaper_content_len(count, panel_w, panel_h, is_vertical) - viewport_along).max(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallpaperHit {
    Back,
    Thumbnail(usize),
}

pub fn wallpaper_hit_test(
    count: usize,
    panel_w: f32,
    panel_h: f32,
    is_vertical: bool,
    scroll: f32,
    x: f32,
    y: f32,
) -> Option<WallpaperHit> {
    // ----- la banda de pestañas ocupa el borde de arriba del panel: las
    // coordenadas del puntero se miden desde abajo de ella, igual que el panel
    // debajo del dock -----
    let band = overlay_tabs_h(is_vertical);
    let (along_pos, cross_pos) = if is_vertical {
        (y - band, x)
    } else {
        (x, y - band)
    };
    if along_pos < 0.0 || cross_pos < 0.0 {
        return None;
    }
    if along_pos < WALLPAPER_BACK_ZONE_W {
        return Some(WallpaperHit::Back);
    }
    let (tw, th) = wallpaper_thumb_size(panel_w, panel_h, is_vertical);
    let (along_size, cross_size) = if is_vertical { (th, tw) } else { (tw, th) };
    if cross_pos < WALLPAPER_PADDING || cross_pos > WALLPAPER_PADDING + cross_size {
        return None;
    }
    let local_along = along_pos - WALLPAPER_BACK_ZONE_W + scroll;
    for i in 0..count {
        let c = i as f32 * (along_size + WALLPAPER_GAP);
        if local_along >= c && local_along <= c + along_size {
            return Some(WallpaperHit::Thumbnail(i));
        }
    }
    None
}

#[cfg(test)]
mod hit_tests {
    use super::*;

    /// El puntero se mide desde abajo de la banda de pestañas: si la traducción se
    /// desincroniza del dibujo, la banda "come" la primera miniatura (o el click
    /// abre la de al lado). Los tamaños salen de las mismas funciones que el dibujo.
    fn casos(panel_w: f32, panel_h: f32, is_vertical: bool) {
        let band = overlay_tabs_h(is_vertical);
        let (tw, th) = wallpaper_thumb_size(panel_w, panel_h, is_vertical);
        let along_size = if is_vertical { th } else { tw };
        let count = 3;
        // ----- el helper toma coordenadas del contenido; la banda se suma acá
        // donde le toca (cross en el panel ancho, along en el vertical) -----
        let hit = |along: f32, cross: f32| {
            let (x, y) = if is_vertical {
                (cross, band + along)
            } else {
                (along, band + cross)
            };
            wallpaper_hit_test(count, panel_w, panel_h, is_vertical, 0.0, x, y)
        };
        // ----- dentro de la banda no hay nada -----
        assert_eq!(
            wallpaper_hit_test(count, panel_w, panel_h, is_vertical, 0.0, 5.0, band * 0.5),
            None
        );
        // ----- la flecha de volver, justo debajo -----
        assert_eq!(
            hit(WALLPAPER_BACK_ZONE_W * 0.5, WALLPAPER_PADDING + 1.0),
            Some(WallpaperHit::Back)
        );
        // ----- primera y segunda miniatura, alineadas con el dibujo -----
        for i in 0..2 {
            let along = WALLPAPER_BACK_ZONE_W + i as f32 * (along_size + WALLPAPER_GAP) + 1.0;
            assert_eq!(
                hit(along, WALLPAPER_PADDING + 1.0),
                Some(WallpaperHit::Thumbnail(i)),
                "i={i} vertical={is_vertical}"
            );
        }
    }

    #[test]
    fn el_hit_test_descuenta_la_banda() {
        casos(1920.0, 196.0, false);
        casos(WALLPAPER_PANEL_H_TEST, 744.0, true);
    }

    /// El ancho del panel vertical: el mismo que le da `open_wallpaper_picker`
    /// (`WALLPAPER_PANEL_H`, el cross de siempre).
    const WALLPAPER_PANEL_H_TEST: f32 = 170.0;
}
