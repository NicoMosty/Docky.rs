//! Dibujo del calendario (geometría y aritmética de fechas en `menu::calendar`).

use super::*;
use crate::menu::{
    CAL_HEADER_H, CAL_ROWS, CAL_TITLE_H, CAL_WEEKDAYS, CalendarMonth, cal_cell_center_x,
    cal_cell_center_y,
};

/// Título (mes + año) y grilla de 6 semanas. El mes viene en el propio control.
pub(crate) fn draw_calendar(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    control: &Control,
    month: CalendarMonth,
    args: &DrawArgs,
    _y: f32,
) {
    let s = args.render_scale;
    let settings = &args.dock.config.settings;
    let panel_width = args.panel_width;
    let today = crate::menu::today();

    let title = month.label();
    let title_size = 10.0;
    draw_text(
        pixmap,
        text_cache,
        &title,
        (panel_width - text_width_estimate(&title, title_size)) / 2.0 * s,
        (control.y + centered_text_y(CAL_TITLE_H, title_size)) * s,
        title_size * s,
        &text_hex(settings),
        700,
    );

    for (col, label) in CAL_WEEKDAYS.iter().enumerate() {
        let size = 7.5;
        let cx = cal_cell_center_x(panel_width, col as u32);
        draw_text(
            pixmap,
            text_cache,
            label,
            (cx - text_width_estimate(label, size) / 2.0) * s,
            (control.y + CAL_TITLE_H + centered_text_y(CAL_HEADER_H, size)) * s,
            size * s,
            &text_dim_hex(settings),
            600,
        );
    }

    let days = month.days();
    // ----- 0 = lunes -----
    let first = month.first_weekday() as i32;
    let es_el_mes_de_hoy = today.month == month;
    for row in 0..CAL_ROWS {
        for col in 0..7u32 {
            let day = row as i32 * 7 + col as i32 - first + 1;
            if day < 1 || day as u32 > days {
                continue;
            }
            let size = 9.5;
            let cx = cal_cell_center_x(panel_width, col) * s;
            let cy = cal_cell_center_y(control.y, row) * s;
            let hoy = es_el_mes_de_hoy && day as u32 == today.day;
            if hoy {
                fill_circle(pixmap, cx, cy, 8.0 * s, accent(settings));
            }
            let label = day.to_string();
            draw_text(
                pixmap,
                text_cache,
                &label,
                cx - text_width_estimate(&label, size) / 2.0 * s,
                cy - size * 0.75 * s,
                size * s,
                &if hoy {
                    on_accent_hex(settings)
                } else {
                    text_hex(settings)
                },
                if hoy { 700 } else { 500 },
            );
        }
    }
}
