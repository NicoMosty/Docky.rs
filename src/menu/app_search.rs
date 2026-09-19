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
pub const APP_CARD_LABEL_H: f32 = 10.0;
pub const APP_CARD_ROW_H: f32 =
    APP_CARD_PAD + APP_CARD_ICON + APP_CARD_LABEL_GAP + APP_CARD_LABEL_H + APP_CARD_PAD;

/// Filas de la grilla del panel ancho (dock arriba/abajo).
pub const APP_SEARCH_ROWS: usize = 2;
/// Tarjeta del panel vertical: el panel es una columna, así que la tarjeta es más
/// ancha (la misma que la miniatura de los fondos) y van DOS por fila. El alto de
/// la tarjeta sigue siendo `APP_CARD_W`, así el icono y la etiqueta caen donde
/// caen en el panel ancho.
pub const APP_CARD_W_VERTICAL: f32 = 150.0;
pub const APP_SEARCH_COLS_VERTICAL: usize = 2;
/// Filas que se ven a la vez en el panel vertical (el resto scrollea).
pub const APP_SEARCH_VERTICAL_VISIBLE: f32 = 5.0;

/// Ancho de una tarjeta según la orientación. Es la única definición: el reparto
/// de la grilla, el ancho del panel vertical y el dibujo salen de acá.
pub fn app_search_card_w(is_vertical: bool) -> f32 {
    if is_vertical {
        APP_CARD_W_VERTICAL
    } else {
        APP_CARD_W
    }
}

/// Largo de una fila de la grilla en el eje del scroll (el alto de la tarjeta).
pub fn app_search_row_h(is_vertical: bool) -> f32 {
    if is_vertical {
        APP_CARD_W
    } else {
        APP_CARD_ROW_H
    }
}

/// Ancho del CONTENIDO del panel vertical del launcher: las dos columnas más los
/// insets. La banda de pestañas va aparte (`menu::frame_for`).
pub fn app_search_vertical_content_w() -> f32 {
    APP_SEARCH_COLS_VERTICAL as f32 * (APP_CARD_W_VERTICAL + APP_CARD_GAP) - APP_CARD_GAP
        + MENU_PADDING * 2.0
}

pub fn build_app_search_controls(frame: PanelFrame, is_vertical: bool) -> (Vec<Control>, f32) {
    let controls = vec![Control {
        kind: ControlKind::SearchBox,
        y: frame.y + MENU_PADDING,
        height: SEARCH_BOX_HEIGHT,
    }];
    // ----- la separación caja -> tarjetas es la misma que la del borde de arriba
    // a la caja: el panel del launcher usa MENU_PADDING de punta a punta. El alto
    // que devuelve es el del CONTENIDO (el frame), no el del panel. -----
    let fixed_h =
        (app_search_strip_y(frame) - frame.y) + app_search_strip_h(is_vertical) + MENU_PADDING;
    (controls, fixed_h)
}

/// Dónde arranca la grilla de tarjetas, en coordenadas del PANEL: debajo de la
/// caja de búsqueda y del padding. La banda no entra acá: el frame ya la dejó
/// afuera.
pub fn app_search_strip_y(frame: PanelFrame) -> f32 {
    frame.y + MENU_PADDING + SEARCH_BOX_HEIGHT + MENU_PADDING
}

/// Origen del viewport del strip (dónde cae la tarjeta con offset (0,0)). Lo usan
/// el dibujo y el hit test, así que no se pueden desincronizar.
pub fn app_search_strip_origin(frame: PanelFrame) -> (f32, f32) {
    (frame.x + MENU_PADDING, app_search_strip_y(frame))
}

/// Alto del strip: la grilla entera en el horizontal (no scrollea en el cross), el
/// tramo visible de la grilla en el vertical. No depende del frame: son las filas
/// que se ven.
pub fn app_search_strip_h(is_vertical: bool) -> f32 {
    let row_h = app_search_row_h(is_vertical);
    let rows = if is_vertical {
        APP_SEARCH_VERTICAL_VISIBLE
    } else {
        APP_SEARCH_ROWS as f32
    };
    rows * (row_h + APP_CARD_GAP) - APP_CARD_GAP
}

/// Columnas que entran en el ancho útil del frame. `OVERLAY_PANEL_W` (640) está
/// elegido para que la grilla ancha dé 6: 6*92 + 5*10 = 602 contra los 620
/// disponibles. En el vertical el frame mide 330, o sea dos tarjetas de 150.
pub fn app_search_cols(frame: PanelFrame, is_vertical: bool) -> usize {
    let card_w = app_search_card_w(is_vertical);
    let avail = (frame.w - MENU_PADDING * 2.0).max(1.0);
    (((avail + APP_CARD_GAP) / (card_w + APP_CARD_GAP)).floor() as usize).max(1)
}

/// Tarjetas por página del panel ancho: el scroll mueve la grilla entera, no
/// media. En el vertical el scroll es por fila, así que no se usa.
pub fn app_search_page_len(frame: PanelFrame, is_vertical: bool) -> usize {
    app_search_cols(frame, is_vertical) * APP_SEARCH_ROWS
}

/// Ancho de una página = el tramo visible, así cada página cae alineada con el
/// inset del panel y el scroll no queda a medio camino.
pub fn app_search_page_w(frame: PanelFrame) -> f32 {
    (frame.w - MENU_PADDING * 2.0).max(1.0)
}

/// Corrimiento de la grilla para centrarla en el viewport (6 columnas dejan 18px
/// de sobra en el panel de 640: 9 de cada lado; en el vertical no sobra nada).
fn app_search_grid_off(frame: PanelFrame, is_vertical: bool) -> f32 {
    let cols = app_search_cols(frame, is_vertical) as f32;
    let grid_w = cols * (app_search_card_w(is_vertical) + APP_CARD_GAP) - APP_CARD_GAP;
    ((app_search_page_w(frame) - grid_w) * 0.5).max(0.0)
}

/// Cuántas tarjetas salta ↑/↓: una fila entera de la grilla. Con una sola
/// columna ±1 arriba/abajo repetiría el vecino de al lado.
pub fn app_search_row_step(frame: PanelFrame, is_vertical: bool) -> isize {
    app_search_cols(frame, is_vertical) as isize
}

/// Desplazamiento de la tarjeta `index` dentro del viewport del strip, en (x, y),
/// relativo a `app_search_strip_origin`. Las dos grillas son row-major (se llena
/// la fila antes de bajar, así el ranking se lee de izquierda a derecha): el panel
/// ancho scrollea de a páginas en x y el vertical de a filas en y.
///
/// Es la ÚNICA definición del reparto: la usan el dibujo y el hit test, así que
/// no se pueden desincronizar (trampa 10).
pub fn app_search_card_offset(index: usize, frame: PanelFrame, is_vertical: bool) -> (f32, f32) {
    let cols = app_search_cols(frame, is_vertical);
    let card_w = app_search_card_w(is_vertical);
    let row_h = app_search_row_h(is_vertical);
    if is_vertical {
        return (
            app_search_grid_off(frame, true) + (index % cols) as f32 * (card_w + APP_CARD_GAP),
            (index / cols) as f32 * (row_h + APP_CARD_GAP),
        );
    }
    let per_page = cols * APP_SEARCH_ROWS;
    let slot = index % per_page;
    (
        (index / per_page) as f32 * app_search_page_w(frame)
            + app_search_grid_off(frame, false)
            + (slot % cols) as f32 * (card_w + APP_CARD_GAP),
        (slot / cols) as f32 * (row_h + APP_CARD_GAP),
    )
}

/// Posición de la tarjeta en el eje por el que scrollea el strip: la página
/// entera en el horizontal, la fila en el vertical.
pub fn app_search_card_along(index: usize, frame: PanelFrame, is_vertical: bool) -> f32 {
    if is_vertical {
        let cols = app_search_cols(frame, true).max(1);
        (index / cols) as f32 * (app_search_row_h(true) + APP_CARD_GAP)
    } else {
        (index / app_search_page_len(frame, false)) as f32 * app_search_page_w(frame)
    }
}

/// Cuánto ocupa una tarjeta (o su página) en ese mismo eje. Es lo que tiene que
/// entrar en el viewport para que la selección sea visible.
pub fn app_search_along_len(frame: PanelFrame, is_vertical: bool) -> f32 {
    if is_vertical {
        app_search_row_h(true)
    } else {
        app_search_page_w(frame)
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
    frame: PanelFrame,
    is_vertical: bool,
    highlight: (f32, f32),
) -> Option<usize> {
    (0..count)
        .map(|i| {
            let (ox, oy) = app_search_card_offset(i, frame, is_vertical);
            ((ox - highlight.0).abs() + (oy - highlight.1).abs(), i)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i)| i)
}

pub fn app_search_strip_content_len(count: usize, frame: PanelFrame, is_vertical: bool) -> f32 {
    if is_vertical {
        let cols = app_search_cols(frame, true).max(1);
        let rows = count.div_ceil(cols);
        (rows as f32 * (app_search_row_h(true) + APP_CARD_GAP) - APP_CARD_GAP).max(0.0)
    } else {
        count.div_ceil(app_search_page_len(frame, false).max(1)) as f32 * app_search_page_w(frame)
    }
}

pub fn app_search_max_scroll(count: usize, frame: PanelFrame, is_vertical: bool) -> f32 {
    let viewport_along = app_search_viewport_along(frame, is_vertical);
    (app_search_strip_content_len(count, frame, is_vertical) - viewport_along).max(0.0)
}

/// Tramo visible del strip. En el panel ancho las tarjetas van insetadas por
/// `MENU_PADDING` como la caja de búsqueda, así que el viewport es el ancho menos
/// los dos insets (si no, al final del scroll la última tarjeta se corta contra el
/// borde del panel). En el vertical el inset va en el cross y el tramo visible es
/// el alto que le queda al frame debajo de la caja de búsqueda.
pub fn app_search_viewport_along(frame: PanelFrame, is_vertical: bool) -> f32 {
    if is_vertical {
        frame.h - (app_search_strip_y(frame) - frame.y)
    } else {
        app_search_page_w(frame)
    }
}

/// El alto del frame no entra acá: el borde de abajo del strip ya lo fija
/// `app_search_strip_h`, y para el panel vertical ese alto es el tramo visible
/// (no el alto total, que incluye el padding de abajo).
pub fn app_search_strip_hit_test(
    count: usize,
    frame: PanelFrame,
    is_vertical: bool,
    scroll: f32,
    x: f32,
    y: f32,
) -> Option<usize> {
    let (sx, sy) = app_search_strip_origin(frame);
    let strip_h = app_search_strip_h(is_vertical);
    let x1 = frame.x + frame.w - MENU_PADDING;
    if x < sx || x > x1 || y < sy || y > sy + strip_h {
        return None;
    }
    let card_w = app_search_card_w(is_vertical);
    let card_h = app_search_row_h(is_vertical);
    // ----- se recorre con la MISMA función que dibuja -----
    for i in 0..count {
        let (ox, oy) = app_search_card_offset(i, frame, is_vertical);
        let (cx, cy) = if is_vertical {
            (sx + ox, sy + oy - scroll)
        } else {
            (sx + ox - scroll, sy + oy)
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

    /// Frame del panel ancho con el contenido de siempre (640 de ancho).
    fn ancho() -> PanelFrame {
        frame_for(OVERLAY_PANEL_W, 210.0, false, true)
    }

    /// Frame del panel vertical: dos columnas de tarjeta y la banda al costado.
    fn vertical(band_left: bool) -> PanelFrame {
        frame_for(
            app_search_vertical_content_w(),
            app_search_strip_h(true) + 46.0 + MENU_PADDING,
            true,
            band_left,
        )
    }

    /// El ancho del panel (640) está elegido para que la grilla de dos filas dé
    /// 6 columnas. Si alguien lo toca o cambia la tarjeta, este test avisa antes
    /// de que la grilla quede desalineada.
    #[test]
    fn el_panel_de_640_da_6_columnas_y_2_filas() {
        let f = ancho();
        assert_eq!(app_search_cols(f, false), 6);
        assert_eq!(app_search_page_len(f, false), 12);
        // ----- ↑/↓ saltan una fila de la grilla -----
        assert_eq!(app_search_row_step(f, false), 6);
        // ----- la grilla entra en el ancho útil y la sobra se reparte igual -----
        let grid_w = 6.0 * (APP_CARD_W + APP_CARD_GAP) - APP_CARD_GAP;
        assert!(grid_w <= app_search_page_w(f));
        let off = app_search_grid_off(f, false);
        assert!((0.0..APP_CARD_GAP).contains(&off), "off={off}");
    }

    /// Row-major: la 7ª cae justo debajo de la 1ª (misma x, la fila de abajo), y
    /// la 13ª abre la página siguiente alineada con el borde del viewport.
    #[test]
    fn la_grilla_se_llena_por_fila_y_scrollea_de_a_paginas() {
        let f = ancho();
        let (x0, y0) = app_search_card_offset(0, f, false);
        let (x6, y6) = app_search_card_offset(6, f, false);
        let (x1, y1) = app_search_card_offset(1, f, false);
        assert_eq!(x6, x0, "la 7ª va en la misma columna que la 1ª");
        assert_eq!(y6, APP_CARD_ROW_H + APP_CARD_GAP);
        assert_eq!(y0, 0.0);
        assert_eq!(y1, 0.0, "la 2ª va en la fila de arriba");
        assert_eq!(x1 - x0, APP_CARD_W + APP_CARD_GAP);

        let (x12, y12) = app_search_card_offset(12, f, false);
        assert_eq!(x12, app_search_page_w(f) + x0);
        assert_eq!(y12, 0.0);

        // ----- 30 apps = 3 páginas, y el tope del scroll deja la última pegada -----
        assert_eq!(
            app_search_max_scroll(30, f, false),
            2.0 * app_search_page_w(f)
        );
    }

    /// El grid del panel vertical es de DOS columnas: la 2ª va al lado de la 1ª,
    /// la 3ª abre la fila de abajo, y las dos entran en el ancho del contenido.
    #[test]
    fn el_vertical_es_una_grilla_de_dos_columnas() {
        let f = vertical(true);
        assert_eq!(app_search_cols(f, true), 2);
        let (x0, y0) = app_search_card_offset(0, f, true);
        let (x1, y1) = app_search_card_offset(1, f, true);
        let (x2, y2) = app_search_card_offset(2, f, true);
        assert_eq!(y1, y0, "la 2ª va al lado de la 1ª");
        assert_eq!(x1 - x0, APP_CARD_W_VERTICAL + APP_CARD_GAP);
        assert_eq!(x2, x0, "la 3ª vuelve a la primera columna");
        assert_eq!(y2 - y0, APP_CARD_W + APP_CARD_GAP);
        // ----- las dos columnas entran en el ancho útil del frame -----
        let right = x1 + APP_CARD_W_VERTICAL;
        assert!(
            right <= app_search_page_w(f) + 0.01,
            "la columna derecha se sale del frame: {right} contra {}",
            app_search_page_w(f)
        );
        // ----- ↑/↓ saltan una fila entera (las dos columnas) -----
        assert_eq!(app_search_row_step(f, true), 2);
        // ----- y el scroll del vertical es por fila, no por página -----
        assert_eq!(app_search_card_along(2, f, true), APP_CARD_W + APP_CARD_GAP);
        assert_eq!(app_search_along_len(f, true), APP_CARD_W);
    }

    /// El hit test y el dibujo salen de la misma función: el centro de cada
    /// tarjeta visible tiene que devolver su índice, y los huecos del inset, del
    /// gap y del borde de la página, nada.
    #[test]
    fn el_click_cae_en_la_tarjeta_que_dibuja() {
        let f = ancho();
        let (sx, sy) = app_search_strip_origin(f);
        let count = 30;
        let center = |i: usize, scroll: f32| {
            let (ox, oy) = app_search_card_offset(i, f, false);
            (
                sx + ox + APP_CARD_W * 0.5 - scroll,
                sy + oy + APP_CARD_ROW_H * 0.5,
            )
        };
        for i in 0..12 {
            let (x, y) = center(i, 0.0);
            assert_eq!(
                app_search_strip_hit_test(count, f, false, 0.0, x, y),
                Some(i),
                "i={i}"
            );
        }
        // ----- la segunda página, con el scroll alineado a la página -----
        let page_scroll = app_search_page_w(f);
        for i in 12..24 {
            let (x, y) = center(i, page_scroll);
            assert_eq!(
                app_search_strip_hit_test(count, f, false, page_scroll, x, y),
                Some(i),
                "página 2, i={i}"
            );
        }
        // ----- con 30 apps hay 3 páginas: el tope del scroll muestra la última y
        // la #29 (el final de la lista) sigue siendo alcanzable -----
        let max_scroll = app_search_max_scroll(count, f, false);
        assert_eq!(max_scroll, 2.0 * app_search_page_w(f));
        let (x29, y29) = center(29, max_scroll);
        assert_eq!(
            app_search_strip_hit_test(count, f, false, max_scroll, x29, y29),
            Some(29)
        );

        let (x0, _) = center(0, 0.0);
        let y0 = sy + APP_CARD_ROW_H * 0.5;
        // ----- el inset lateral, el gap entre filas y el borde de arriba -----
        assert_eq!(
            app_search_strip_hit_test(count, f, false, 0.0, x0 - APP_CARD_W * 0.6, y0),
            None
        );
        assert_eq!(
            app_search_strip_hit_test(
                count,
                f,
                false,
                0.0,
                x0,
                sy + APP_CARD_ROW_H + APP_CARD_GAP * 0.5
            ),
            None
        );
        assert_eq!(
            app_search_strip_hit_test(count, f, false, 0.0, x0, sy - 1.0),
            None
        );
        assert_eq!(
            app_search_strip_hit_test(count, f, false, 0.0, 1.0, y0),
            None
        );
        // ----- una tarjeta más allá de la última no existe -----
        let (x5, y5) = center(5, 0.0);
        assert_eq!(app_search_strip_hit_test(5, f, false, 0.0, x5, y5), None);
    }

    /// En el panel vertical el hit test tiene que caer en las DOS columnas, con la
    /// x medida desde el frame (la banda al costado corre todo) y sin devolver
    /// nada ni en la banda ni en el hueco entre columnas.
    #[test]
    fn el_click_del_vertical_sigue_las_dos_columnas() {
        let f = vertical(true);
        let (sx, sy) = app_search_strip_origin(f);
        let count = 8;
        for i in 0..6 {
            let (ox, oy) = app_search_card_offset(i, f, true);
            let x = sx + ox + APP_CARD_W_VERTICAL * 0.5;
            let y = sy + oy + APP_CARD_W * 0.5 - 10.0;
            assert_eq!(
                app_search_strip_hit_test(count, f, true, 10.0, x, y),
                Some(i),
                "i={i}"
            );
        }
        // ----- la banda de pestañas (a la izquierda del frame) no es de nadie -----
        let (ox, oy) = app_search_card_offset(0, f, true);
        let y = sy + oy + APP_CARD_W * 0.5;
        assert_eq!(
            app_search_strip_hit_test(count, f, true, 0.0, f.x - 1.0, y),
            None
        );
        // ----- el hueco entre las dos columnas, tampoco -----
        let hueco = sx + ox + APP_CARD_W_VERTICAL + APP_CARD_GAP * 0.5;
        assert_eq!(
            app_search_strip_hit_test(count, f, true, 0.0, hueco, y),
            None
        );
        // ----- con la banda a la derecha el frame arranca en 0 y el dibujo se
        // corre con él: el mismo click cae en la misma tarjeta -----
        let f2 = vertical(false);
        assert_eq!(f2.x, 0.0);
        let (sx2, _) = app_search_strip_origin(f2);
        assert_eq!(sx2, MENU_PADDING);
        let (ox2, _) = app_search_card_offset(1, f2, true);
        assert_eq!(
            app_search_strip_hit_test(
                count,
                f2,
                true,
                0.0,
                sx2 + ox2 + APP_CARD_W_VERTICAL * 0.5,
                y
            ),
            Some(1)
        );
    }

    /// El color de la etiqueta sigue a la **pastilla**, no al hover: si sale del
    /// hover, al salir de las tarjetas se enciende la etiqueta de la seleccionada
    /// mientras la pastilla queda en otro lado (el titileo que se veía en la 1ª).
    #[test]
    fn el_color_de_la_etiqueta_sigue_a_la_pastilla() {
        let f = ancho();
        // ----- con la pastilla sobre una tarjeta, esa tarjeta -----
        for i in [0, 3, 7, 11] {
            let (ox, oy) = app_search_card_offset(i, f, false);
            assert_eq!(
                app_search_hot_card(30, f, false, (ox, oy)),
                Some(i),
                "la pastilla esta sobre la {i}"
            );
        }
        // ----- a mitad de camino entre dos VECINAS (de fila: 2 y 3; de columna: 2 y
        // 8, que es la de abajo), gana una de las dos: nunca vuelve a la 1ª por estar
        // seleccionada -----
        let medio_entre = |a: usize, b: usize, f: PanelFrame, v: bool| {
            let (ax, ay) = app_search_card_offset(a, f, v);
            let (bx, by) = app_search_card_offset(b, f, v);
            app_search_hot_card(30, f, v, ((ax + bx) / 2.0, (ay + by) / 2.0))
        };
        assert!(
            matches!(medio_entre(2, 3, f, false), Some(2) | Some(3)),
            "vecinas de fila: {:?}",
            medio_entre(2, 3, f, false)
        );
        assert!(
            matches!(medio_entre(2, 8, f, false), Some(2) | Some(8)),
            "vecinas de columna: {:?}",
            medio_entre(2, 8, f, false)
        );
        // ----- sin tarjetas no hay nada que resaltar -----
        assert_eq!(app_search_hot_card(0, f, false, (0.0, 0.0)), None);
        // ----- y en el vertical la pastilla va sobre la tarjeta exacta: la 2ª está
        // al lado de la 1ª, así que la x ahora también decide -----
        let v = vertical(true);
        for i in 0..6 {
            let (ox, oy) = app_search_card_offset(i, v, true);
            assert_eq!(
                app_search_hot_card(30, v, true, (ox, oy)),
                Some(i),
                "la pastilla esta sobre la {i} del vertical"
            );
        }
    }

    /// El ancho del CONTENIDO del panel vertical del launcher es el compartido con el
    /// portapapeles (`OVERLAY_PANEL_VERTICAL_WIDE`, 330): el launcher lo saca de sus dos
    /// columnas de tarjeta y el portapapeles lo usa tal cual para que los títulos no se
    /// elidan a la mitad. Si alguien toca la tarjeta o el gap, esto avisa antes de que
    /// los dos paneles queden de anchos distintos sin querer.
    #[test]
    fn el_ancho_del_launcher_es_el_compartido() {
        assert_eq!(app_search_vertical_content_w(), OVERLAY_PANEL_VERTICAL_WIDE);
        // ----- dos columnas de 150 con gap, más los insets de MENU_PADDING -----
        let esperado =
            2.0 * (APP_CARD_W_VERTICAL + APP_CARD_GAP) - APP_CARD_GAP + MENU_PADDING * 2.0;
        assert_eq!(esperado, app_search_vertical_content_w());
    }

    /// El frame y el tamaño del panel son inversos: el mismo contenido da el mismo
    /// panel y volver a sacar el frame lo deja igual (si se despegan, la banda se
    /// cuenta dos veces o se pierde).
    #[test]
    fn el_frame_y_el_panel_son_inversos() {
        for (w, h, v) in [(640.0, 210.0, false), (330.0, 556.0, true)] {
            for band_left in [true, false] {
                let f = frame_for(w, h, v, band_left);
                assert_eq!(f.w, w);
                assert_eq!(f.h, h);
                let (panel_w, panel_h) = panel_size(f, v);
                assert_eq!(panel_w, w + if v { OVERLAY_TABS_W } else { 0.0 });
                assert_eq!(panel_h, h + if v { 0.0 } else { OVERLAY_TABS_H });
                // ----- y el origen del frame cae del lado del dock -----
                if v {
                    assert_eq!(f.x, if band_left { OVERLAY_TABS_W } else { 0.0 });
                    assert_eq!(
                        panel_size(f, v).0 - f.w - f.x,
                        if band_left { 0.0 } else { OVERLAY_TABS_W }
                    );
                } else {
                    assert_eq!(f.x, 0.0);
                    assert_eq!(f.y, OVERLAY_TABS_H);
                }
            }
        }
    }
}
