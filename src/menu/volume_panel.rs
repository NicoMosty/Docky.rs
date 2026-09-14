//! Panel de volumen: geometría y hit tests. El dibujo vive en
//! `menu_render::volume_panel` (se tocan de a pares).

use super::*;
use crate::widgets::{AudioDevice, VolumeRow};

/// Alto de una fila de volumen: etiqueta + % arriba, barra abajo.
pub const VOLUME_ROW_H: f32 = 32.0;
/// Zona clickeable del icono de la fila, que mutea ese stream.
pub const VOLUME_ICON_ZONE: f32 = 16.0;
/// Alto de una fila del selector de dispositivo de salida.
pub const VOLUME_DEVICE_H: f32 = 22.0;
/// Techo de dispositivos listados: con más, el panel se come la pantalla.
/// `ponytail:` sin scroll; si alguna vez hacen falta más de 4, listarlos en un
/// segundo nivel en vez de agregar scroll al popup.
pub const VOLUME_MAX_DEVICES: usize = 4;

/// Centro vertical de la barra dentro de la fila.
pub fn volume_track_cy(row_y: f32) -> f32 {
    row_y + 22.0
}

pub fn volume_track_x0() -> f32 {
    MENU_PADDING
}

pub fn volume_track_x1(panel_width: f32) -> f32 {
    panel_width - MENU_PADDING
}

/// Valor que corresponde a un x de la barra: se usa en el click y en el arrastre.
pub fn volume_pct_from_x(panel_width: f32, x: f32) -> u8 {
    let x0 = volume_track_x0();
    let x1 = volume_track_x1(panel_width);
    let t = ((x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0);
    (t * 100.0).round() as u8
}

/// Filas + separador + dispositivos, con el alto total del panel.
pub fn build_volume_controls(rows: &[VolumeRow], devices: &[AudioDevice]) -> (Vec<Control>, f32) {
    let mut controls = Vec::new();
    let mut y = MENU_PADDING;
    for i in 0..rows.len() {
        controls.push(Control {
            kind: ControlKind::VolumeRow(i),
            y,
            height: VOLUME_ROW_H,
        });
        y += VOLUME_ROW_H;
    }
    if !devices.is_empty() {
        y += ROW_GAP;
        controls.push(Control {
            kind: ControlKind::Section("Output Device"),
            y,
            height: SECTION_LABEL_HEIGHT,
        });
        y += SECTION_LABEL_HEIGHT;
        for i in 0..devices.len().min(VOLUME_MAX_DEVICES) {
            controls.push(Control {
                kind: ControlKind::VolumeDevice(i),
                y,
                height: VOLUME_DEVICE_H,
            });
            y += VOLUME_DEVICE_H;
        }
    }
    (controls, y + MENU_PADDING)
}

/// El icono mutea y el resto de la fila ajusta: así clickear la etiqueta no
/// cierra el panel (todo lo que no es el icono es zona de barra).
pub fn volume_hit_test(control: &Control, x: f32, y: f32) -> Option<HitTarget> {
    // ----- como los demás hit tests de este módulo, no confía en que el que
    // llama ya haya filtrado la fila -----
    if y < control.y || y > control.y + control.height {
        return None;
    }
    match control.kind {
        ControlKind::VolumeRow(i) => {
            if x <= MENU_PADDING + VOLUME_ICON_ZONE && y <= control.y + 18.0 {
                return Some(HitTarget::VolumeMute(i));
            }
            Some(HitTarget::VolumeTrack(i))
        }
        ControlKind::VolumeDevice(i) => Some(HitTarget::VolumeDevice(i)),
        _ => None,
    }
}

#[cfg(test)]
mod volume_panel_tests {
    use super::*;

    fn row(i: usize, y: f32) -> Control {
        Control {
            kind: ControlKind::VolumeRow(i),
            y,
            height: VOLUME_ROW_H,
        }
    }

    #[test]
    fn el_icono_mutea_y_el_resto_ajusta() {
        let r = row(0, 10.0);
        assert_eq!(
            volume_hit_test(&r, 12.0, 15.0),
            Some(HitTarget::VolumeMute(0))
        );
        assert_eq!(
            volume_hit_test(&r, 120.0, 32.0),
            Some(HitTarget::VolumeTrack(0))
        );
        // la etiqueta también ajusta: si no, clickearla cerraba el panel
        assert_eq!(
            volume_hit_test(&r, 120.0, 15.0),
            Some(HitTarget::VolumeTrack(0))
        );
        // la fila de abajo no roba clicks de la de arriba
        assert_eq!(volume_hit_test(&row(1, 42.0), 120.0, 32.0), None);
    }

    #[test]
    fn el_valor_de_la_barra_va_de_0_a_100_y_recorta() {
        let w = MENU_WIDTH;
        assert_eq!(volume_pct_from_x(w, volume_track_x0()), 0);
        assert_eq!(volume_pct_from_x(w, volume_track_x1(w)), 100);
        assert_eq!(volume_pct_from_x(w, 0.0), 0);
        assert_eq!(volume_pct_from_x(w, 9999.0), 100);
        // el medio, con la tolerancia del redondeo
        let mid = (volume_track_x0() + volume_track_x1(w)) / 2.0;
        assert!((volume_pct_from_x(w, mid) as i32 - 50).abs() <= 1);
    }

    #[test]
    fn filas_y_dispositivos_entran_en_el_alto_del_panel() {
        let rows = vec![
            VolumeRow {
                target: crate::widgets::VolumeTarget::Output,
                label: "Output".into(),
                pct: 50,
                muted: false,
            },
            VolumeRow {
                target: crate::widgets::VolumeTarget::Stream(7),
                label: "mpv".into(),
                pct: 100,
                muted: false,
            },
        ];
        let devices = vec![
            AudioDevice {
                name: "a".into(),
                label: "A".into(),
                default: true,
            },
            AudioDevice {
                name: "b".into(),
                label: "B".into(),
                default: false,
            },
        ];
        let (controls, height) = build_volume_controls(&rows, &devices);
        // 2 filas + sección + 2 dispositivos
        assert_eq!(controls.len(), 5);
        let last = controls.last().unwrap();
        assert!(last.y + last.height <= height);
        // el alto total cubre todo lo que se dibuja
        assert!(height >= 2.0 * VOLUME_ROW_H + SECTION_LABEL_HEIGHT + 2.0 * VOLUME_DEVICE_H);
    }
}
