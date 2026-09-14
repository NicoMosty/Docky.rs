use super::*;

pub const WIDGET_CHIP_W: f32 = 100.0;
pub const WIDGET_CHIP_H: f32 = 24.0;
pub const WIDGET_CHIP_GAP: f32 = 6.0;
pub const WIDGET_COL_GAP: f32 = 10.0;
pub const WIDGET_GROUP_LABEL_H: f32 = 16.0;
pub const WIDGET_SECTION_GAP: f32 = 10.0;

// ----- La lista y las etiquetas salen de la tabla (`src/widget.rs`): antes eran
// una tercera y una cuarta copia a mano de los 12 variantes. Se re-exportan para
// que `menu_render` siga entrando por `crate::menu::…` -----
pub(crate) use crate::widget::{widget_kind_order, widget_label};

fn widget_available(settings: &DockSettings) -> Vec<WidgetKind> {
    widget_kind_order()
        .into_iter()
        .filter(|k| !settings.widgets.iter().any(|p| p.kind == *k))
        .collect()
}

fn widget_in_slot(settings: &DockSettings, slot: WidgetSlot) -> Vec<WidgetKind> {
    settings
        .widgets
        .iter()
        .filter(|p| p.slot == slot)
        .map(|p| p.kind)
        .collect()
}

/// Fila donde se soltó, como índice de inserción dentro de la columna
/// (0 = primero). Se cuenta sobre los chips que ya están en la columna, sin
/// descontar el arrastrado: el clamp deja el índice máximo en "al final".
fn drop_index_in_slot(first_chip_y: f32, y: f32, members: usize) -> usize {
    if y <= first_chip_y {
        return 0;
    }
    let row_h = WIDGET_CHIP_H + WIDGET_CHIP_GAP;
    (((y - first_chip_y) / row_h).floor() as usize).min(members)
}

// ----- none removes -----
pub fn place_widget(
    settings: &mut DockSettings,
    kind: WidgetKind,
    slot: Option<WidgetSlot>,
    index: usize,
) {
    settings.widgets.retain(|p| p.kind != kind);
    let Some(slot) = slot else {
        return;
    };
    // ----- insertar en la fila pedida, no al final: con push, cualquier drop
    // terminaba en la última posición del slot, así que de última a primera no
    // se movía nada (volvía al mismo lugar). -----
    let at = settings
        .widgets
        .iter()
        .enumerate()
        .filter(|(_, p)| p.slot == slot)
        .nth(index)
        .map(|(i, _)| i)
        .unwrap_or(settings.widgets.len());
    settings.widgets.insert(at, WidgetPlacement { kind, slot });
}

pub struct WidgetChipRect {
    pub kind: WidgetKind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub assigned: bool,
}

pub struct WidgetChipLayout {
    pub rects: Vec<WidgetChipRect>,
    pub available_label_y: f32,
    pub columns_label_y: f32,
    pub col_w: f32,
    pub total_h: f32,
}

pub fn widget_chip_layout(
    settings: &DockSettings,
    panel_width: f32,
    start_y: f32,
) -> WidgetChipLayout {
    let content_w = panel_width - MENU_PADDING * 2.0;
    let mut y = start_y;
    let mut rects = Vec::new();

    let available_label_y = y;
    y += WIDGET_GROUP_LABEL_H;
    let avail = widget_available(settings);
    let cols = ((content_w + WIDGET_CHIP_GAP) / (WIDGET_CHIP_W + WIDGET_CHIP_GAP))
        .floor()
        .max(1.0) as usize;
    for (i, kind) in avail.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = MENU_PADDING + col as f32 * (WIDGET_CHIP_W + WIDGET_CHIP_GAP);
        let cy = y + row as f32 * (WIDGET_CHIP_H + WIDGET_CHIP_GAP);
        rects.push(WidgetChipRect {
            kind: *kind,
            x,
            y: cy,
            w: WIDGET_CHIP_W,
            h: WIDGET_CHIP_H,
            assigned: false,
        });
    }
    let avail_rows = if avail.is_empty() {
        0
    } else {
        avail.len().div_ceil(cols)
    };
    if avail_rows > 0 {
        y += avail_rows as f32 * (WIDGET_CHIP_H + WIDGET_CHIP_GAP) - WIDGET_CHIP_GAP;
    }
    y += WIDGET_SECTION_GAP;

    let columns_label_y = y;
    y += WIDGET_GROUP_LABEL_H;
    let col_w = (content_w - WIDGET_COL_GAP * 2.0) / 3.0;
    let slots = [WidgetSlot::Left, WidgetSlot::Middle, WidgetSlot::Right];
    let mut max_rows = 0usize;
    for (ci, slot) in slots.iter().enumerate() {
        let members = widget_in_slot(settings, *slot);
        let col_x = MENU_PADDING + ci as f32 * (col_w + WIDGET_COL_GAP);
        for (ri, kind) in members.iter().enumerate() {
            let cy = y + ri as f32 * (WIDGET_CHIP_H + WIDGET_CHIP_GAP);
            rects.push(WidgetChipRect {
                kind: *kind,
                x: col_x,
                y: cy,
                w: col_w,
                h: WIDGET_CHIP_H,
                assigned: true,
            });
        }
        max_rows = max_rows.max(members.len());
    }
    y += max_rows.max(1) as f32 * WIDGET_CHIP_H
        + max_rows.saturating_sub(1) as f32 * WIDGET_CHIP_GAP;

    WidgetChipLayout {
        rects,
        available_label_y,
        columns_label_y,
        col_w,
        total_h: y,
    }
}

pub fn widget_chip_hit_test(
    settings: &DockSettings,
    control_y: f32,
    panel_width: f32,
    x: f32,
    y: f32,
) -> Option<WidgetKind> {
    let layout = widget_chip_layout(settings, panel_width, control_y);
    layout
        .rects
        .into_iter()
        .find(|r| x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h)
        .map(|r| r.kind)
}

/// none means dropped available
///
/// Devuelve el slot destino **y la fila** dentro de él (0 = primero de la
/// columna). Antes sólo devolvía el slot, y por eso el reordenamiento vertical
/// dependía del `push` de `place_widget`.
pub fn widget_drop_target(
    settings: &DockSettings,
    control_y: f32,
    panel_width: f32,
    x: f32,
    y: f32,
) -> Option<(WidgetSlot, usize)> {
    let layout = widget_chip_layout(settings, panel_width, control_y);
    if y < layout.columns_label_y {
        return None;
    }
    let content_w = panel_width - MENU_PADDING * 2.0;
    let col_w = (content_w - WIDGET_COL_GAP * 2.0) / 3.0;
    let rel_x = x - MENU_PADDING;
    let slot = if rel_x < col_w + WIDGET_COL_GAP {
        WidgetSlot::Left
    } else if rel_x < (col_w + WIDGET_COL_GAP) * 2.0 {
        WidgetSlot::Middle
    } else {
        WidgetSlot::Right
    };
    let first_chip_y = layout.columns_label_y + WIDGET_GROUP_LABEL_H;
    let index = drop_index_in_slot(first_chip_y, y, widget_in_slot(settings, slot).len());
    Some((slot, index))
}

#[cfg(test)]
mod drag_tests {
    use super::*;
    use crate::config::{DockSettings, WidgetKind, WidgetPlacement, WidgetSlot};

    fn settings_with(widgets: &[(WidgetKind, WidgetSlot)]) -> DockSettings {
        let mut s = DockSettings::default();
        s.widgets = widgets
            .iter()
            .map(|(kind, slot)| WidgetPlacement {
                kind: *kind,
                slot: *slot,
            })
            .collect();
        s
    }

    /// El caso reportado: el último de una columna no se podía subir al primero.
    #[test]
    fn subir_el_ultimo_al_principio() {
        use WidgetKind::{Battery, Clock, Volume};
        let mut s = settings_with(&[
            (Clock, WidgetSlot::Middle),
            (Battery, WidgetSlot::Middle),
            (Volume, WidgetSlot::Middle),
        ]);
        place_widget(&mut s, Volume, Some(WidgetSlot::Middle), 0);
        assert_eq!(
            widget_in_slot(&s, WidgetSlot::Middle),
            vec![Volume, Clock, Battery]
        );
    }

    /// De la última columna a la primera fila de otra.
    #[test]
    fn cruzar_a_la_primera_fila_de_otra_columna() {
        use WidgetKind::{Clock, Network, Volume};
        let mut s = settings_with(&[
            (Volume, WidgetSlot::Left),
            (Network, WidgetSlot::Left),
            (Clock, WidgetSlot::Right),
        ]);
        place_widget(&mut s, Clock, Some(WidgetSlot::Left), 0);
        assert_eq!(
            widget_in_slot(&s, WidgetSlot::Left),
            vec![Clock, Volume, Network]
        );
    }

    /// Soltar pasado el final de la columna lo deja último (lo que antes hacía
    /// SIEMPRE, hiciera lo que hiciera).
    #[test]
    fn soltar_al_final_lo_deja_ultimo() {
        use WidgetKind::{Clock, Volume};
        let mut s = settings_with(&[(Clock, WidgetSlot::Middle), (Volume, WidgetSlot::Middle)]);
        place_widget(&mut s, Clock, Some(WidgetSlot::Middle), 2);
        assert_eq!(widget_in_slot(&s, WidgetSlot::Middle), vec![Volume, Clock]);
    }

    /// La fila del drop es el índice de inserción.
    #[test]
    fn la_fila_del_drop_da_el_indice() {
        let first = 200.0;
        let row = WIDGET_CHIP_H + WIDGET_CHIP_GAP;
        assert_eq!(drop_index_in_slot(first, first - 50.0, 3), 0);
        assert_eq!(drop_index_in_slot(first, first + 1.0, 3), 0);
        assert_eq!(drop_index_in_slot(first, first + row + 1.0, 3), 1);
        assert_eq!(drop_index_in_slot(first, first + row * 2.0 + 1.0, 3), 2);
        assert_eq!(drop_index_in_slot(first, first + row * 99.0, 3), 3);
    }

    /// El caso general: soltando en la fila `r`, la tarjeta queda en la fila `r`
    /// (antes caía siempre en la última). Recorre la columna real del usuario, con
    /// las filas calculadas por el MISMO layout que usa el drop.
    #[test]
    fn soltar_en_cualquier_fila_la_deja_en_esa_fila() {
        use WidgetKind::{Battery, Bluetooth, KbdLayout, Network, PowerMenu, Volume};
        let columna = [
            (Network, WidgetSlot::Left),
            (Bluetooth, WidgetSlot::Left),
            (Volume, WidgetSlot::Left),
            (PowerMenu, WidgetSlot::Left),
            (Battery, WidgetSlot::Left),
            (KbdLayout, WidgetSlot::Left),
        ];
        let chips_y = 50.0;
        let dragged = KbdLayout; // la última de la columna
        let row_h = WIDGET_CHIP_H + WIDGET_CHIP_GAP;
        for r in 0..columna.len() {
            let mut s = settings_with(&columna);
            let layout = widget_chip_layout(&s, 400.0, chips_y);
            let first_chip_y = layout.columns_label_y + WIDGET_GROUP_LABEL_H;
            let y = first_chip_y + r as f32 * row_h + 2.0;
            let Some((slot, index)) = widget_drop_target(&s, chips_y, 400.0, 60.0, y) else {
                panic!("fila {r}: debía dar columna");
            };
            assert_eq!(slot, WidgetSlot::Left, "fila {r}: columna");
            assert_eq!(index, r, "fila {r}: índice de inserción");
            place_widget(&mut s, dragged, Some(slot), index);
            let orden = widget_in_slot(&s, WidgetSlot::Left);
            assert_eq!(orden[r], dragged, "fila {r}: quedó en {orden:?}");
        }
    }
}
