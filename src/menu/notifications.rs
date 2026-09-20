//! Panel de notificaciones: datos, geometría y hit test. Sin dibujo (eso es
//! `menu_render/notifications.rs`), igual que el resto de `menu/`.
//!
//! Es el panel MÁS simple del overlay a propósito: filas de alto fijo, sin buscador
//! (no hay nada que filtrar en un puñado de avisos), sin acciones y sin agrupar. Lo
//! único que guarda el dock es el historial de lo que ya mostró como pill.

use super::PanelFrame;

/// Alto de una fila. Igual que las del portapapeles: los dos paneles de lista se leen
/// con el mismo ritmo.
pub const NOTIF_ROW_H: f32 = 34.0;
/// Cuántos avisos guarda el historial. Es un tope, no una cola que crece sola: el más
/// viejo se cae. 50 avisos de dos líneas son ~5 KB.
pub const NOTIF_HISTORY_CAP: usize = 50;

/// Un aviso guardado. `at` es la hora del reloj del dock **sin AM/PM** (`11:38`), la
/// misma cadena que la isla: sale de `WidgetSnapshot::time_short` cuando llega.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotifyEntry {
    pub title: String,
    pub body: String,
    pub at: String,
}

/// Rect de la fila `i` en píxeles lógicos, relativo al panel y con el scroll aplicado.
pub fn notif_row_rect(frame: PanelFrame, i: usize, scroll: f32) -> (f32, f32, f32, f32) {
    let top = frame.y + super::MENU_PADDING - scroll;
    (frame.x, top + i as f32 * NOTIF_ROW_H, frame.w, NOTIF_ROW_H)
}

/// Fila bajo el puntero (`x`, `y` en coordenadas del panel). `None` fuera del frame o
/// más allá de `count`. Es la inversa exacta de `notif_row_rect` (trampa 10).
pub fn notif_row_at(frame: PanelFrame, scroll: f32, x: f32, y: f32, count: usize) -> Option<usize> {
    if x < frame.x || x > frame.x + frame.w || y < frame.y || y > frame.y + frame.h {
        return None;
    }
    let rel = (y - frame.y - super::MENU_PADDING + scroll) / NOTIF_ROW_H;
    if rel < 0.0 {
        return None;
    }
    let i = rel.floor() as usize;
    (i < count).then_some(i)
}

/// Scroll máximo: lo que sobra de las filas por debajo del área visible.
pub fn notif_max_scroll(frame: PanelFrame, count: usize) -> f32 {
    let viewport = notif_visible_rows(frame) as f32 * NOTIF_ROW_H;
    (count as f32 * NOTIF_ROW_H - viewport).max(0.0)
}

/// Filas que entran en el panel, del mismo alto del frame que el clamp del scroll:
/// una sola cuenta para el dibujo, el hit test y la página de PageUp/PageDown (si se
/// despegan, el scroll deja filas cortadas o clicleables de más).
pub fn notif_visible_rows(frame: PanelFrame) -> usize {
    (((frame.h - 2.0 * super::MENU_PADDING) / NOTIF_ROW_H).floor()).max(1.0) as usize
}

#[cfg(test)]
mod notif_tests {
    use super::*;
    use crate::menu::PanelFrame;

    fn frame() -> PanelFrame {
        PanelFrame {
            x: 10.0,
            y: 36.0,
            w: 300.0,
            h: 3.0 * NOTIF_ROW_H + 2.0 * crate::menu::MENU_PADDING,
        }
    }

    /// El hit test tiene que caer en la fila que el dibujo ubica ahí, con scroll y
    /// todo: es la cuenta que se desincroniza sola si se escribe dos veces.
    #[test]
    fn el_hit_test_cae_en_la_fila_que_dibuja_el_rect() {
        let f = frame();
        for scroll in [0.0, 17.0, 34.0] {
            for i in 0..3 {
                let (x, y, w, h) = notif_row_rect(f, i, scroll);
                // ----- el centro de una fila scrolleada puede quedar FUERA del frame
                // (ahí no hay nada que clickear), así que se pide el punto medio de la
                // parte visible -----
                let top = y.max(f.y);
                let bottom = (y + h).min(f.y + f.h);
                if bottom - top < 2.0 {
                    continue;
                }
                let p = (x + w / 2.0, (top + bottom) / 2.0);
                assert_eq!(
                    notif_row_at(f, scroll, p.0, p.1, 3),
                    Some(i),
                    "fila {i} con scroll {scroll} en {p:?}"
                );
            }
        }
    }

    #[test]
    fn fuera_del_frame_o_mas_alla_del_final_no_hay_fila() {
        let f = frame();
        assert_eq!(notif_row_at(f, 0.0, f.x - 5.0, f.y + 10.0, 3), None);
        assert_eq!(notif_row_at(f, 0.0, f.x + 5.0, f.y + f.h + 5.0, 3), None);
        // ----- más filas pedidas que avisos: no hay fila 9 -----
        let y = notif_row_rect(f, 9, 0.0).1 + NOTIF_ROW_H / 2.0;
        assert_eq!(notif_row_at(f, 0.0, f.x + 5.0, y, 3), None);
    }

    /// El scroll máximo es lo que sobra del área visible (con pocas filas, cero).
    #[test]
    fn el_alto_se_acota_y_el_scroll_no_pasa_de_lo_que_sobra() {
        let f = frame();
        assert_eq!(notif_max_scroll(f, 3), 0.0, "tres filas entran justas");
        assert!(notif_max_scroll(f, 10) > 0.0, "diez filas no entran");
        assert_eq!(notif_max_scroll(f, 0), 0.0);
    }
}
