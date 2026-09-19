use super::*;

pub const WALLPAPER_BACK_ZONE_W: f32 = 30.0;
/// El filmstrip usa el mismo inset y la misma separación que los otros paneles
/// del overlay (la caja de búsqueda del launcher y del portapapeles), y las
/// miniaturas el mismo radio (`OVERLAY_RADIUS`).
pub const WALLPAPER_PADDING: f32 = super::MENU_PADDING;
pub const WALLPAPER_GAP: f32 = super::MENU_PADDING;
pub const WALLPAPER_ASPECT: f32 = 1.6;

// ----- always landscape -----
pub fn wallpaper_thumb_size(frame: PanelFrame, is_vertical: bool) -> (f32, f32) {
    if is_vertical {
        let w = (frame.w - WALLPAPER_PADDING * 2.0).max(1.0);
        (w, w / WALLPAPER_ASPECT)
    } else {
        // ----- la miniatura entra en el alto del CONTENIDO: la banda de pestañas
        // ya quedó afuera del frame, así que no se descuenta dos veces -----
        let h = (frame.h - WALLPAPER_PADDING * 2.0).max(1.0);
        (h * WALLPAPER_ASPECT, h)
    }
}

pub fn wallpaper_content_len(count: usize, frame: PanelFrame, is_vertical: bool) -> f32 {
    let (tw, th) = wallpaper_thumb_size(frame, is_vertical);
    let along = if is_vertical { th } else { tw };
    (count as f32 * (along + WALLPAPER_GAP) - WALLPAPER_GAP).max(0.0)
}

pub fn wallpaper_max_scroll(count: usize, frame: PanelFrame, is_vertical: bool) -> f32 {
    let viewport_along = (wallpaper_along(frame, is_vertical) - WALLPAPER_BACK_ZONE_W).max(1.0);
    (wallpaper_content_len(count, frame, is_vertical) - viewport_along).max(0.0)
}

/// Largo del contenido en el eje del scroll (el filmstrip): el ancho del frame en
/// el panel ancho y el alto en el vertical. La zona de la flecha de volver está
/// adentro (es una franja de `WALLPAPER_BACK_ZONE_W`).
pub fn wallpaper_along(frame: PanelFrame, is_vertical: bool) -> f32 {
    if is_vertical { frame.h } else { frame.w }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallpaperHit {
    Back,
    Thumbnail(usize),
}

pub fn wallpaper_hit_test(
    count: usize,
    frame: PanelFrame,
    is_vertical: bool,
    scroll: f32,
    x: f32,
    y: f32,
) -> Option<WallpaperHit> {
    // ----- el puntero viene en coordenadas del PANEL: se lo lleva al frame (la
    // banda de pestañas puede estar a un costado) -----
    let (along_pos, cross_pos) = if is_vertical {
        (y - frame.y, x - frame.x)
    } else {
        (x - frame.x, y - frame.y)
    };
    if along_pos < 0.0 || cross_pos < 0.0 {
        return None;
    }
    if along_pos < WALLPAPER_BACK_ZONE_W {
        return Some(WallpaperHit::Back);
    }
    let (tw, th) = wallpaper_thumb_size(frame, is_vertical);
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

    /// El puntero se mide desde el frame (la banda de pestañas queda afuera): si la
    /// traducción se desincroniza del dibujo, la banda "come" la primera miniatura
    /// (o el click abre la de al lado). Los tamaños salen de las mismas funciones
    /// que el dibujo.
    fn casos(content_w: f32, content_h: f32, is_vertical: bool, band_left: bool) {
        let frame = frame_for(content_w, content_h, is_vertical, band_left);
        let count = 3;
        let (tw, th) = wallpaper_thumb_size(frame, is_vertical);
        let along_size = if is_vertical { th } else { tw };
        // ----- el helper toma coordenadas del CONTENIDO; el origen del frame se
        // suma acá, que es lo que hace el panel -----
        let hit = |along: f32, cross: f32| {
            let (x, y) = if is_vertical {
                (frame.x + cross, frame.y + along)
            } else {
                (frame.x + along, frame.y + cross)
            };
            wallpaper_hit_test(count, frame, is_vertical, 0.0, x, y)
        };
        // ----- dentro de la banda de pestañas no hay nada -----
        if is_vertical {
            assert_eq!(
                wallpaper_hit_test(count, frame, is_vertical, 0.0, frame.x - 1.0, 5.0),
                None
            );
        } else {
            assert_eq!(
                wallpaper_hit_test(count, frame, is_vertical, 0.0, 5.0, frame.y - 1.0),
                None
            );
        }
        // ----- la flecha de volver, justo adentro del contenido -----
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
                "i={i} vertical={is_vertical} band_left={band_left}"
            );
        }
    }

    #[test]
    fn el_hit_test_descuenta_la_banda() {
        // ----- panel ancho (dock arriba): la banda es la fila de arriba -----
        casos(1920.0, 170.0, false, true);
        // ----- panel vertical con la banda al costado del dock: la misma cuenta,
        // corrida en x, y el frame arranca en 26 -----
        casos(crate::menu::OVERLAY_PANEL_VERTICAL_W, 640.0, true, true);
        // ----- y con el dock a la derecha la banda va del otro lado: el contenido
        // arranca en 0 y el alto es el mismo -----
        casos(crate::menu::OVERLAY_PANEL_VERTICAL_W, 640.0, true, false);
    }
}
