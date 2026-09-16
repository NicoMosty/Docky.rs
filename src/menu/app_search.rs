use super::*;

pub const SEARCH_MAX_RESULTS: usize = 60;

pub const APP_CARD_W: f32 = 92.0;
pub const APP_CARD_GAP: f32 = 10.0;
pub const APP_CARD_ICON: f32 = 46.0;
/// Inset del icono dentro de la tarjeta (arriba y abajo) y separación entre el
/// icono y la etiqueta. La altura de la tarjeta SALE de estas: antes era 78 fijo,
/// o sea 6px arriba y 18 abajo, así que en una grilla de dos filas el ritmo
/// vertical no cerraba. El dibujo usa las mismas constantes.
pub const APP_CARD_PAD: f32 = 6.0;
pub const APP_CARD_LABEL_GAP: f32 = 4.0;
const APP_CARD_LABEL_H: f32 = 10.0;
pub const APP_CARD_ROW_H: f32 =
    APP_CARD_PAD + APP_CARD_ICON + APP_CARD_LABEL_GAP + APP_CARD_LABEL_H + APP_CARD_PAD;
/// Filas de la grilla del panel horizontal. El vertical sigue siendo una tira de
/// una tarjeta por fila (ese panel es angosto: `app_search_vertical_cross`).
pub const APP_SEARCH_ROWS: usize = 2;
pub const APP_SEARCH_VERTICAL_VISIBLE: f32 = 5.0;

pub fn build_app_search_controls(is_vertical: bool) -> (Vec<Control>, f32) {
    let top = MENU_PADDING + overlay_tabs_h(is_vertical);
    let controls = vec![Control {
        kind: ControlKind::SearchBox,
        y: top,
        height: SEARCH_BOX_HEIGHT,
    }];
    // ----- la separación caja -> tarjetas es la misma que la de banda -> caja:
    // el panel del launcher usa MENU_PADDING de punta a punta. -----
    let fixed_h = app_search_strip_y(is_vertical) + app_search_strip_h(is_vertical) + MENU_PADDING;
    (controls, fixed_h)
}

pub fn app_search_vertical_cross(dock_thickness: f32) -> f32 {
    dock_thickness.max(APP_CARD_ICON + MENU_PADDING * 2.0)
}

/// Alto del panel vertical: arranca donde arranca el contenido, así la banda de
/// pestañas (apilada, más alta) le come el lugar que ocupa.
pub fn app_search_vertical_along() -> f32 {
    app_search_strip_y(true) + app_search_strip_h(true) + MENU_PADDING
}

/// Dónde arranca la grilla de tarjetas: debajo de la banda, la caja de búsqueda
/// y el mismo padding en las cuatro puntas del panel.
pub fn app_search_strip_y(is_vertical: bool) -> f32 {
    MENU_PADDING + overlay_tabs_h(is_vertical) + SEARCH_BOX_HEIGHT + MENU_PADDING
}

/// Alto del strip: la grilla entera en el horizontal (no scrollea en el cross),
/// el tramo visible de la tira en el vertical.
pub fn app_search_strip_h(is_vertical: bool) -> f32 {
    if is_vertical {
        APP_SEARCH_VERTICAL_VISIBLE * (APP_CARD_W + APP_CARD_GAP) - APP_CARD_GAP
    } else {
        APP_SEARCH_ROWS as f32 * APP_CARD_ROW_H + (APP_SEARCH_ROWS - 1) as f32 * APP_CARD_GAP
    }
}

// ----- grilla del panel horizontal -----
/// Columnas que entran en el ancho útil. `OVERLAY_PANEL_W` (640) está elegido
/// para que entren 6: 6*92 + 5*10 = 602 contra los 620 disponibles.
pub fn app_search_cols(panel_w: f32) -> usize {
    let avail = (panel_w - MENU_PADDING * 2.0).max(1.0);
    (((avail + APP_CARD_GAP) / (APP_CARD_W + APP_CARD_GAP)).floor() as usize).max(1)
}

/// Tarjetas por página: el scroll mueve la grilla entera, no media.
pub fn app_search_page_len(panel_w: f32) -> usize {
    app_search_cols(panel_w) * APP_SEARCH_ROWS
}

/// Ancho de una página = el tramo visible, así cada página cae alineada con el
/// inset del panel y el scroll no queda a medio camino.
pub fn app_search_page_w(panel_w: f32) -> f32 {
    (panel_w - MENU_PADDING * 2.0).max(1.0)
}

/// Corrimiento de la grilla para centrarla en la página (6 columnas dejan 18px
/// de sobra en el panel de 640: 9 de cada lado).
fn app_search_grid_off(panel_w: f32) -> f32 {
    let cols = app_search_cols(panel_w) as f32;
    let grid_w = cols * (APP_CARD_W + APP_CARD_GAP) - APP_CARD_GAP;
    ((app_search_page_w(panel_w) - grid_w) * 0.5).max(0.0)
}

/// Cuántas tarjetas salta ↑/↓: una fila entera de la grilla. Es 1 en el panel
/// vertical, donde hay una sola columna. Con dos filas, ±1 arriba/abajo repetiría
/// el vecino de al lado en vez de cambiar de fila.
pub fn app_search_row_step(panel_w: f32, is_vertical: bool) -> isize {
    if is_vertical {
        1
    } else {
        app_search_cols(panel_w) as isize
    }
}

/// Desplazamiento de la tarjeta `index` dentro del contenido del strip, en
/// (x, y). Horizontal: grilla row-major (se llena la fila antes de bajar, así el
/// ranking se lee de izquierda a derecha) que scrollea de a páginas. Vertical:
/// una tarjeta por fila y la x la centra el dibujo (acá va 0).
///
/// Es la ÚNICA definición del reparto: la usan el dibujo y el hit test, así que
/// no se pueden desincronizar (trampa 10).
pub fn app_search_card_offset(index: usize, panel_w: f32, is_vertical: bool) -> (f32, f32) {
    if is_vertical {
        return (0.0, index as f32 * (APP_CARD_W + APP_CARD_GAP));
    }
    let cols = app_search_cols(panel_w);
    let per_page = cols * APP_SEARCH_ROWS;
    let slot = index % per_page;
    (
        (index / per_page) as f32 * app_search_page_w(panel_w)
            + app_search_grid_off(panel_w)
            + (slot % cols) as f32 * (APP_CARD_W + APP_CARD_GAP),
        (slot / cols) as f32 * (APP_CARD_ROW_H + APP_CARD_GAP),
    )
}

/// Posición de la tarjeta en el eje por el que scrollea el strip: la página
/// entera en el horizontal, la tarjeta en el vertical.
pub fn app_search_card_along(index: usize, panel_w: f32, is_vertical: bool) -> f32 {
    if is_vertical {
        index as f32 * (APP_CARD_W + APP_CARD_GAP)
    } else {
        (index / app_search_page_len(panel_w)) as f32 * app_search_page_w(panel_w)
    }
}

/// Cuánto ocupa una tarjeta (o su página) en ese mismo eje. Es lo que tiene que
/// entrar en el viewport para que la selección sea visible.
pub fn app_search_along_len(panel_w: f32, is_vertical: bool) -> f32 {
    if is_vertical {
        APP_CARD_W
    } else {
        app_search_page_w(panel_w)
    }
}

/// Índice de la tarjeta que está tapando la pastilla del resaltado: la más
/// cercana a `highlight` (durante la animación la pastilla va entre dos, así que
/// gana la más próxima y el color cambia una sola vez, al pasar el punto medio).
///
/// Es lo que decide en qué color va la etiqueta. Tiene que salir de acá y no del
/// hover: el hover se borra al salir de las tarjetas (banda, caja de búsqueda, el
/// hueco entre tarjetas) mientras la pastilla se queda donde estaba, así que la
/// etiqueta de la tarjeta seleccionada se encendía sola con la pastilla en otro
/// lado. Ese era el "titileo" del nombre.
pub fn app_search_hot_card(
    count: usize,
    panel_w: f32,
    is_vertical: bool,
    highlight: (f32, f32),
) -> Option<usize> {
    (0..count)
        .map(|i| {
            let (ox, oy) = app_search_card_offset(i, panel_w, is_vertical);
            ((ox - highlight.0).abs() + (oy - highlight.1).abs(), i)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i)| i)
}

pub fn app_search_strip_content_len(count: usize, panel_w: f32, is_vertical: bool) -> f32 {
    if is_vertical {
        (count as f32 * (APP_CARD_W + APP_CARD_GAP) - APP_CARD_GAP).max(0.0)
    } else {
        count.div_ceil(app_search_page_len(panel_w).max(1)) as f32 * app_search_page_w(panel_w)
    }
}

pub fn app_search_max_scroll(count: usize, panel_w: f32, panel_h: f32, is_vertical: bool) -> f32 {
    let viewport_along = app_search_viewport_along(panel_w, panel_h, is_vertical);
    (app_search_strip_content_len(count, panel_w, is_vertical) - viewport_along).max(0.0)
}

/// Tramo visible del strip. En el panel horizontal las tarjetas van insetadas
/// por `MENU_PADDING` como la caja de búsqueda, así que el viewport es el ancho
/// menos los dos insets (si no, al final del scroll la última tarjeta se corta
/// contra el borde del panel). En el vertical el inset va en el cross.
pub fn app_search_viewport_along(panel_w: f32, panel_h: f32, is_vertical: bool) -> f32 {
    if is_vertical {
        panel_h - app_search_strip_y(true)
    } else {
        app_search_page_w(panel_w)
    }
}

/// El `panel_h` no entra acá: el borde de abajo del strip ya lo fija
/// `app_search_strip_h`, y para el panel vertical ese alto es el tramo visible
/// (no el alto total, que incluye el padding de abajo).
pub fn app_search_strip_hit_test(
    count: usize,
    panel_w: f32,
    is_vertical: bool,
    scroll: f32,
    x: f32,
    y: f32,
) -> Option<usize> {
    let strip_y = app_search_strip_y(is_vertical);
    let strip_h = app_search_strip_h(is_vertical);
    let inside = if is_vertical {
        x >= 0.0 && x <= panel_w
    } else {
        x >= MENU_PADDING && x <= panel_w - MENU_PADDING
    };
    if !inside || y < strip_y || y > strip_y + strip_h {
        return None;
    }
    // ----- se recorre con la MISMA función que dibuja -----
    for i in 0..count {
        let (ox, oy) = app_search_card_offset(i, panel_w, is_vertical);
        let (card_w, card_h, cx, cy) = if is_vertical {
            (
                (panel_w - MENU_PADDING * 2.0).max(1.0),
                APP_CARD_W,
                MENU_PADDING,
                strip_y + oy - scroll,
            )
        } else {
            (
                APP_CARD_W,
                APP_CARD_ROW_H,
                MENU_PADDING + ox - scroll,
                strip_y + oy,
            )
        };
        if x >= cx && x <= cx + card_w && y >= cy && y <= cy + card_h {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod strip_tests {
    use super::*;

    /// El ancho del panel (640) está elegido para que la grilla de dos filas dé
    /// 6 columnas. Si alguien lo toca o cambia la tarjeta, este test avisa antes
    /// de que la grilla quede desalineada.
    #[test]
    fn el_panel_de_640_da_6_columnas_y_2_filas() {
        assert_eq!(app_search_cols(OVERLAY_PANEL_W), 6);
        assert_eq!(app_search_page_len(OVERLAY_PANEL_W), 12);
        // ----- ↑/↓ saltan una fila de la grilla -----
        assert_eq!(app_search_row_step(OVERLAY_PANEL_W, false), 6);
        assert_eq!(app_search_row_step(OVERLAY_PANEL_W, true), 1);
        // ----- la grilla entra en el ancho útil y la sobra se reparte igual -----
        let grid_w = 6.0 * (APP_CARD_W + APP_CARD_GAP) - APP_CARD_GAP;
        assert!(grid_w <= app_search_page_w(OVERLAY_PANEL_W));
        let off = app_search_grid_off(OVERLAY_PANEL_W);
        assert!((0.0..APP_CARD_GAP).contains(&off), "off={off}");
    }

    /// Row-major: la 7ª cae justo debajo de la 1ª (misma x, la fila de abajo), y
    /// la 13ª abre la página siguiente alineada con el borde del viewport.
    #[test]
    fn la_grilla_se_llena_por_fila_y_scrollea_de_a_paginas() {
        let w = OVERLAY_PANEL_W;
        let (x0, y0) = app_search_card_offset(0, w, false);
        let (x6, y6) = app_search_card_offset(6, w, false);
        let (x1, y1) = app_search_card_offset(1, w, false);
        assert_eq!(x6, x0, "la 7ª va en la misma columna que la 1ª");
        assert_eq!(y6, APP_CARD_ROW_H + APP_CARD_GAP);
        assert_eq!(y0, 0.0);
        assert_eq!(y1, 0.0, "la 2ª va en la fila de arriba");
        assert_eq!(x1 - x0, APP_CARD_W + APP_CARD_GAP);

        let (x12, y12) = app_search_card_offset(12, w, false);
        assert_eq!(x12, app_search_page_w(w) + x0);
        assert_eq!(y12, 0.0);

        // ----- 30 apps = 3 páginas, y el tope del scroll deja la última pegada -----
        assert_eq!(
            app_search_max_scroll(30, w, 240.0, false),
            2.0 * app_search_page_w(w)
        );
    }

    /// El hit test y el dibujo salen de la misma función: el centro de cada
    /// tarjeta visible tiene que devolver su índice, y los huecos del inset, del
    /// gap y del borde de la página, nada.
    #[test]
    fn el_click_cae_en_la_tarjeta_que_dibuja() {
        let w = OVERLAY_PANEL_W;
        let count = 30;
        let center = |i: usize, scroll: f32| {
            let (ox, oy) = app_search_card_offset(i, w, false);
            (
                MENU_PADDING + ox + APP_CARD_W * 0.5 - scroll,
                app_search_strip_y(false) + oy + APP_CARD_ROW_H * 0.5,
            )
        };
        for i in 0..12 {
            let (x, y) = center(i, 0.0);
            assert_eq!(
                app_search_strip_hit_test(count, w, false, 0.0, x, y),
                Some(i),
                "i={i}"
            );
        }
        // ----- la segunda página, con el scroll alineado a la página -----
        let page_scroll = app_search_page_w(w);
        for i in 12..24 {
            let (x, y) = center(i, page_scroll);
            assert_eq!(
                app_search_strip_hit_test(count, w, false, page_scroll, x, y),
                Some(i),
                "página 2, i={i}"
            );
        }
        // ----- con 30 apps hay 3 páginas: el tope del scroll muestra la última y
        // la #29 (el final de la lista) sigue siendo alcanzable -----
        let max_scroll = app_search_max_scroll(count, w, 240.0, false);
        assert_eq!(max_scroll, 2.0 * app_search_page_w(w));
        let (x29, y29) = center(29, max_scroll);
        assert_eq!(
            app_search_strip_hit_test(count, w, false, max_scroll, x29, y29),
            Some(29)
        );

        let (x0, _) = center(0, 0.0);
        let y0 = app_search_strip_y(false) + APP_CARD_ROW_H * 0.5;
        // ----- el inset lateral, el gap entre filas y el borde de arriba -----
        assert_eq!(
            app_search_strip_hit_test(count, w, false, 0.0, x0 - APP_CARD_W * 0.6, y0),
            None
        );
        assert_eq!(
            app_search_strip_hit_test(
                count,
                w,
                false,
                0.0,
                x0,
                app_search_strip_y(false) + APP_CARD_ROW_H + APP_CARD_GAP * 0.5
            ),
            None
        );
        assert_eq!(
            app_search_strip_hit_test(count, w, false, 0.0, x0, app_search_strip_y(false) - 1.0),
            None
        );
        assert_eq!(
            app_search_strip_hit_test(count, w, false, 0.0, 1.0, y0),
            None
        );
        // ----- una tarjeta más allá de la última no existe -----
        let (x5, y5) = center(5, 0.0);
        assert_eq!(app_search_strip_hit_test(5, w, false, 0.0, x5, y5), None);
    }

    /// El color de la etiqueta sigue a la **pastilla**, no al hover: si sale del
    /// hover, al salir de las tarjetas se enciende la etiqueta de la seleccionada
    /// mientras la pastilla queda en otro lado (el titileo que se veía en la 1ª).
    #[test]
    fn el_color_de_la_etiqueta_sigue_a_la_pastilla() {
        let w = OVERLAY_PANEL_W;
        // ----- con la pastilla sobre una tarjeta, esa tarjeta -----
        for i in [0, 3, 7, 11] {
            let (ox, oy) = app_search_card_offset(i, w, false);
            assert_eq!(
                app_search_hot_card(30, w, false, (ox, oy)),
                Some(i),
                "la pastilla esta sobre la {i}"
            );
        }
        // ----- a mitad de camino entre dos VECINAS (de fila: 2 y 3; de columna: 2 y
        // 8, que es la de abajo), gana una de las dos: nunca vuelve a la 1ª por estar
        // seleccionada -----
        let medio_entre = |a: usize, b: usize| {
            let (ax, ay) = app_search_card_offset(a, w, false);
            let (bx, by) = app_search_card_offset(b, w, false);
            app_search_hot_card(30, w, false, ((ax + bx) / 2.0, (ay + by) / 2.0))
        };
        assert!(
            matches!(medio_entre(2, 3), Some(2) | Some(3)),
            "vecinas de fila: {:?}",
            medio_entre(2, 3)
        );
        assert!(
            matches!(medio_entre(2, 8), Some(2) | Some(8)),
            "vecinas de columna: {:?}",
            medio_entre(2, 8)
        );
        // ----- sin tarjetas no hay nada que resaltar -----
        assert_eq!(app_search_hot_card(0, w, false, (0.0, 0.0)), None);
        // ----- y en el vertical la x es 0 en todas: gana la de la y mas cercana -----
        let (_, vy) = app_search_card_offset(4, 66.0, true);
        assert_eq!(app_search_hot_card(30, 66.0, true, (0.0, vy)), Some(4));
    }

    /// El vertical sigue siendo una tira de una tarjeta por fila: la 2ª va
    /// debajo de la 1ª y el hit test la sigue.
    #[test]
    fn el_vertical_sigue_siendo_una_tira() {
        let panel_w = app_search_vertical_cross(26.0);
        let count = 8;
        let (_, y0) = app_search_card_offset(0, panel_w, true);
        let (_, y1) = app_search_card_offset(1, panel_w, true);
        assert_eq!(y1 - y0, APP_CARD_W + APP_CARD_GAP);
        for i in 0..3 {
            let (_, oy) = app_search_card_offset(i, panel_w, true);
            let y = app_search_strip_y(true) + oy + APP_CARD_W * 0.5;
            assert_eq!(
                app_search_strip_hit_test(count, panel_w, true, 0.0, panel_w * 0.5, y),
                Some(i),
                "i={i}"
            );
        }
    }
}
