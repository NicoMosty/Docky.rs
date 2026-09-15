//! Calendario del reloj: qué mes se muestra, aritmética de fechas y geometría
//! del panel. El dibujo vive en `menu_render::calendar` (se tocan de a pares).

use super::*;

/// Mes que muestra el panel (`month` en 1..=12, como `tm_mon + 1`).
///
/// El mes vive DENTRO del `Control` del panel (`ControlKind::Calendar`) y no en un
/// campo aparte de `DockPopupMode`: así el mes dibujado y el que las flechas
/// cambian son el mismo dato, sin un segundo estado que se pueda desincronizar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CalendarMonth {
    pub year: i32,
    pub month: u32,
}

/// Día local de hoy: sólo lo que el calendario necesita para resaltar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CalendarToday {
    pub month: CalendarMonth,
    pub day: u32,
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Iniciales de los días, empezando el lunes (la convención de `%u`, que es la
/// que devuelve `first_weekday`).
pub const CAL_WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// Semanas dibujadas siempre: un panel que cambia de alto al pasar de mes mueve
/// la superficie y su input region, así que el alto es fijo y las semanas que
/// sobran quedan vacías.
pub const CAL_ROWS: u32 = 6;
/// Banda del título (mes + año) y alto de la fila de iniciales.
pub const CAL_TITLE_H: f32 = 22.0;
pub const CAL_HEADER_H: f32 = 16.0;
/// Alto de cada fila de días.
pub const CAL_ROW_H: f32 = 24.0;

/// Un `tm` local de ahora. Si libc falla devuelve una fecha fija en vez de
/// dejar el panel sin dibujar.
fn local_tm() -> libc::tm {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        let mut t: libc::time_t = 0;
        libc::time(&mut t);
        if libc::localtime_r(&t, &mut tm).is_null() {
            tm = std::mem::zeroed();
            tm.tm_year = 70;
            tm.tm_mon = 0;
            tm.tm_mday = 1;
        }
    }
    tm
}

pub fn today() -> CalendarToday {
    let tm = local_tm();
    CalendarToday {
        month: CalendarMonth {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1).clamp(1, 12) as u32,
        },
        day: tm.tm_mday.max(1) as u32,
    }
}

impl CalendarMonth {
    /// "September 2026".
    pub fn label(&self) -> String {
        let name = MONTH_NAMES
            .get(self.month.saturating_sub(1) as usize)
            .copied()
            .unwrap_or("?");
        format!("{name} {}", self.year)
    }

    /// Día de la semana del día 1: 0 = lunes … 6 = domingo. `mktime` normaliza el
    /// `tm` y de paso calcula `tm_wday` (que viene con 0 = domingo).
    pub fn first_weekday(&self) -> u32 {
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_year = self.year - 1900;
        tm.tm_mon = self.month as i32 - 1;
        tm.tm_mday = 1;
        tm.tm_hour = 12;
        // ----- -1: que libc decida el horario de verano (a las 12 no cambia de día) -----
        tm.tm_isdst = -1;
        unsafe { libc::mktime(&mut tm) };
        ((tm.tm_wday as u32) + 6) % 7
    }

    pub fn days(&self) -> u32 {
        match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if self.leap() => 29,
            2 => 28,
            // ----- mes fuera de rango: no pasa, pero no puede romper el dibujo -----
            _ => 30,
        }
    }

    fn leap(&self) -> bool {
        self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0)
    }

    /// Un mes adelante (`dir` 1) o atrás (-1), dando la vuelta en el año.
    pub fn step(&mut self, dir: i32) {
        let m = self.month as i32 - 1 + dir;
        self.year += m.div_euclid(12);
        self.month = m.rem_euclid(12) as u32 + 1;
    }
}

/// Ancho de una de las 7 columnas.
pub fn cal_cell_w(panel_width: f32) -> f32 {
    (panel_width - 2.0 * MENU_PADDING) / 7.0
}

pub fn cal_cell_center_x(panel_width: f32, col: u32) -> f32 {
    MENU_PADDING + cal_cell_w(panel_width) * (col as f32 + 0.5)
}

/// Centro vertical de la fila `row` dentro del panel del calendario.
pub fn cal_cell_center_y(panel_y: f32, row: u32) -> f32 {
    panel_y + CAL_TITLE_H + CAL_HEADER_H + CAL_ROW_H * (row as f32 + 0.5)
}

/// El panel del calendario es un solo `Control` que lleva el mes: no tiene nada
/// clickeable, las flechas lo cambian.
pub fn calendar_controls(month: CalendarMonth) -> (Vec<Control>, f32) {
    let y = MENU_PADDING;
    let height = CAL_TITLE_H + CAL_HEADER_H + CAL_ROW_H * CAL_ROWS as f32;
    (
        vec![Control {
            kind: ControlKind::Calendar(month),
            y,
            height,
        }],
        y + height + MENU_PADDING,
    )
}

/// El mes que está mostrando el panel (la copia que dejó `calendar_controls`).
pub fn calendar_month(controls: &[Control]) -> Option<CalendarMonth> {
    controls.iter().find_map(|c| match c.kind {
        ControlKind::Calendar(m) => Some(m),
        _ => None,
    })
}

#[cfg(test)]
mod calendar_tests {
    use super::*;

    fn month(year: i32, month: u32) -> CalendarMonth {
        CalendarMonth { year, month }
    }

    #[test]
    fn da_la_vuelta_el_ano_en_los_dos_sentidos() {
        let mut m = month(2026, 12);
        m.step(1);
        assert_eq!(m, month(2027, 1));
        m.step(-1);
        assert_eq!(m, month(2026, 12));
        let mut m = month(2026, 1);
        m.step(-1);
        assert_eq!(m, month(2025, 12));
        // doce pasos vuelven al mismo mes
        let mut m = month(2026, 3);
        for _ in 0..12 {
            m.step(1);
        }
        assert_eq!(m, month(2027, 3));
    }

    #[test]
    fn febrero_sigue_la_regla_de_ano_bisiesto() {
        assert_eq!(month(2024, 2).days(), 29);
        assert_eq!(month(2026, 2).days(), 28);
        // 2100 no es bisiesto, 2000 sí
        assert_eq!(month(2100, 2).days(), 28);
        assert_eq!(month(2000, 2).days(), 29);
        assert_eq!(month(2026, 9).days(), 30);
        assert_eq!(month(2026, 8).days(), 31);
    }

    #[test]
    fn el_dia_uno_sabe_que_dia_de_la_semana_es() {
        // 2024-01-01 lunes y 2026-09-01 martes (verificado con `date +%u`)
        assert_eq!(month(2024, 1).first_weekday(), 0);
        assert_eq!(month(2026, 9).first_weekday(), 1);
        assert_eq!(month(2026, 1).first_weekday(), 3);
        // el 1 de septiembre de 2026 cae en la segunda columna: 1 + (offset - 1)
        assert_eq!(
            cal_cell_center_x(MENU_WIDTH, month(2026, 9).first_weekday() as u32),
            cal_cell_center_x(MENU_WIDTH, 1)
        );
    }

    #[test]
    fn el_alto_del_panel_cubre_las_seis_semanas() {
        let (controls, height) = calendar_controls(month(2026, 9));
        assert_eq!(controls.len(), 1);
        let c = controls[0];
        assert_eq!(calendar_month(&controls).unwrap(), month(2026, 9));
        // la última fila entra completa
        let last = cal_cell_center_y(c.y, CAL_ROWS - 1);
        assert!(last + CAL_ROW_H / 2.0 <= c.y + c.height);
        assert!(c.y + c.height <= height);
        // el mismo alto para cualquier mes: la superficie no se redimensiona
        let (_, otro) = calendar_controls(month(2027, 2));
        assert_eq!(height, otro);
    }

    #[test]
    fn la_etiqueta_arma_mes_y_ano() {
        assert_eq!(month(2026, 9).label(), "September 2026");
        assert_eq!(month(2026, 1).label(), "January 2026");
    }
}
