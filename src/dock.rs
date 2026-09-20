use crate::config::{Config, DockEdge, PinnedApp, WidgetKind};

const EASE_FACTOR: f32 = 0.35;
const SETTLE_EPSILON: f32 = 0.001;
pub const DRAG_THRESHOLD: f32 = 6.0;
const V_EDGE_PADDING: f32 = 12.0;
// ----- static base -----
const REFERENCE_ICON_SIZE: f32 = 44.0;
const REFERENCE_GAP: f32 = 8.0;
const MAX_DOCK_WIDTH: f32 = 2400.0;
// ----- nonzero size -----
const WIDGET_BAR_MIN: f32 = 60.0;

pub struct DockIcon {
    pub app: PinnedApp,
    pub scale: f32,
    pub target_scale: f32,
    pub x: f32,
    pub target_x: f32,
}

pub struct Dock {
    pub icons: Vec<DockIcon>,
    pub config: Config,
    pub pointer_pos: Option<(f64, f64)>,
    pub dragging_index: Option<usize>,
    pub hovered_widget: Option<WidgetKind>,
    // ----- caller syncs -----
    pub widget_bar_content_len: f32,
}

impl Dock {
    pub fn new(config: Config) -> Self {
        let icons: Vec<DockIcon> = config
            .apps
            .iter()
            .cloned()
            .map(|app| DockIcon {
                app,
                scale: 1.0,
                target_scale: 1.0,
                x: 0.0,
                target_x: 0.0,
            })
            .collect();

        let mut dock = Self {
            icons,
            config,
            pointer_pos: None,
            dragging_index: None,
            hovered_widget: None,
            widget_bar_content_len: 0.0,
        };
        dock.update_layout_targets();
        for icon in &mut dock.icons {
            icon.x = icon.target_x;
        }
        dock
    }

    fn effective_icon_size(&self) -> f32 {
        let s = &self.config.settings;
        s.icon_size * s.dock_scale
    }

    fn effective_gap(&self) -> f32 {
        let s = &self.config.settings;
        s.icon_gap * s.dock_scale
    }

    pub fn is_vertical(&self) -> bool {
        matches!(
            self.config.settings.dock_edge,
            DockEdge::Left | DockEdge::Right
        )
    }

    /// De qué lado cae la columna de pestañas de los paneles del overlay: el mismo
    /// lado del dock (`Left` la deja pegada borde de pantalla). En los paneles
    /// anchos no importa, la banda es una fila arriba.
    pub fn band_left(&self) -> bool {
        !matches!(self.config.settings.dock_edge, DockEdge::Right)
    }

    /// Caja del contenido de un panel del overlay de `panel_w` x `panel_h`: el
    /// panel menos la banda de pestañas. Es la única cuenta (el inverso de
    /// `menu::panel_size`) y sale del borde del dock: en el vertical la columna va
    /// pegada al mismo lado que la barra.
    pub fn panel_frame(&self, panel_w: f32, panel_h: f32) -> crate::menu::PanelFrame {
        let is_vertical = self.is_vertical();
        let (band_w, band_h) = if is_vertical {
            (crate::menu::OVERLAY_TABS_W, 0.0)
        } else {
            (0.0, crate::menu::OVERLAY_TABS_H)
        };
        crate::menu::PanelFrame {
            x: if is_vertical && self.band_left() {
                band_w
            } else {
                0.0
            },
            y: band_h,
            w: (panel_w - band_w).max(0.0),
            h: (panel_h - band_h).max(0.0),
        }
    }

    pub fn cross_len(&self) -> f32 {
        let s = &self.config.settings;
        let icon_size = REFERENCE_ICON_SIZE * s.dock_scale;
        icon_size * s.magnify_scale + V_EDGE_PADDING * s.dock_scale * 2.0
    }

    pub fn base_size(&self) -> (u32, u32) {
        let s = &self.config.settings;
        let cross_len = self.cross_len();
        if self.icons.is_empty() {
            let bar_len = (self.widget_bar_content_len + s.width_padding * s.widget_scale * 2.0)
                .max(WIDGET_BAR_MIN)
                .min(MAX_DOCK_WIDTH);
            return if self.is_vertical() {
                (cross_len.ceil() as u32, bar_len.ceil() as u32)
            } else {
                (bar_len.ceil() as u32, cross_len.ceil() as u32)
            };
        }
        let icon_size = REFERENCE_ICON_SIZE * s.dock_scale;
        let n = self.icons.len() as f32;
        let spacing = REFERENCE_GAP * s.dock_scale;
        let main_len =
            (n * icon_size + (n - 1.0).max(0.0) * spacing + s.width_padding * s.dock_scale * 2.0)
                .min(MAX_DOCK_WIDTH);
        if self.is_vertical() {
            (cross_len.ceil() as u32, main_len.ceil() as u32)
        } else {
            (main_len.ceil() as u32, cross_len.ceil() as u32)
        }
    }

    pub fn thickness(&self) -> u32 {
        let (w, h) = self.base_size();
        if self.is_vertical() { w } else { h }
    }

    fn main_axis_len(&self) -> f32 {
        let (w, h) = self.base_size();
        if self.is_vertical() {
            h as f32
        } else {
            w as f32
        }
    }

    fn rest_centers(&self) -> impl Iterator<Item = f32> + use<> {
        let icon_size = self.effective_icon_size();
        let spacing = self.effective_gap();
        let total = self.main_axis_len();
        let count = self.icons.len();
        let content = count as f32 * icon_size + (count as f32 - 1.0).max(0.0) * spacing;
        let start = (total - content) / 2.0;
        (0..count).map(move |i| start + i as f32 * (icon_size + spacing) + icon_size / 2.0)
    }

    pub fn relayout(&mut self) {
        self.update_layout_targets();
        for icon in &mut self.icons {
            if self.dragging_index.is_none() {
                icon.x = icon.target_x;
            }
        }
    }

    pub fn add_app(&mut self, app: PinnedApp) {
        let x = self.base_size().0 as f32;
        self.icons.push(DockIcon {
            app,
            scale: 1.0,
            target_scale: 1.0,
            x,
            target_x: x,
        });
        self.relayout();
        self.config.apps = self.icons.iter().map(|i| i.app.clone()).collect();
    }

    pub fn remove_icon(&mut self, index: usize) {
        if index < self.icons.len() {
            self.icons.remove(index);
            self.relayout();
            self.config.apps = self.icons.iter().map(|i| i.app.clone()).collect();
        }
    }

    pub fn set_icon(&mut self, index: usize, icon_name: String) {
        if let Some(icon) = self.icons.get_mut(index) {
            icon.app.icon = icon_name;
            self.config.apps = self.icons.iter().map(|i| i.app.clone()).collect();
        }
    }

    fn update_layout_targets(&mut self) {
        let centers = self.rest_centers();
        let dragging = self.dragging_index;
        for (i, (icon, cx)) in self.icons.iter_mut().zip(centers).enumerate() {
            if Some(i) == dragging {
                continue;
            }
            icon.target_x = cx;
        }
    }

    pub fn update_magnification(&mut self) {
        let s = &self.config.settings;
        let (base_scale, radius, max_scale) = (1.0_f32, s.magnify_radius, s.magnify_scale);
        let centers = self.rest_centers();
        let vertical = self.is_vertical();
        match self.pointer_pos {
            Some((px, py)) => {
                let main = if vertical { py as f32 } else { px as f32 };
                for (icon, cx) in self.icons.iter_mut().zip(centers) {
                    let dist = (main - cx).abs();
                    let t = (dist / radius).min(1.0);
                    let falloff = 0.5 * (1.0 + (std::f32::consts::PI * t).cos());
                    icon.target_scale = base_scale + (max_scale - base_scale) * falloff;
                }
            }
            None => {
                for icon in &mut self.icons {
                    icon.target_scale = base_scale;
                }
            }
        }
    }

    pub fn set_pointer(&mut self, pos: Option<(f64, f64)>) {
        self.pointer_pos = pos;
        self.update_magnification();
    }

    pub fn start_drag(&mut self, index: usize) {
        self.dragging_index = Some(index);
    }

    pub fn drag_to(&mut self, cursor_x: f32, cursor_y: f32) {
        let Some(index) = self.dragging_index else {
            return;
        };
        let cursor_main = if self.is_vertical() {
            cursor_y
        } else {
            cursor_x
        };
        self.icons[index].x = cursor_main;
        self.icons[index].target_x = cursor_main;

        let centers = self.rest_centers();
        let mut nearest = 0usize;
        let mut best = f32::MAX;
        for (i, c) in centers.enumerate() {
            let d = (cursor_main - c).abs();
            if d < best {
                best = d;
                nearest = i;
            }
        }
        if nearest != index {
            let icon = self.icons.remove(index);
            self.icons.insert(nearest, icon);
            self.dragging_index = Some(nearest);
            self.update_layout_targets();
        }
    }

    pub fn end_drag(&mut self) {
        self.dragging_index = None;
        self.update_layout_targets();
        self.config.apps = self.icons.iter().map(|i| i.app.clone()).collect();
    }

    pub fn step_animation(&mut self) -> bool {
        let mut animating = false;
        for icon in &mut self.icons {
            let dscale = icon.target_scale - icon.scale;
            if dscale.abs() > SETTLE_EPSILON {
                icon.scale += dscale * EASE_FACTOR;
                animating = true;
            } else {
                icon.scale = icon.target_scale;
            }

            let dx = icon.target_x - icon.x;
            if dx.abs() > SETTLE_EPSILON {
                icon.x += dx * EASE_FACTOR;
                animating = true;
            } else {
                icon.x = icon.target_x;
            }
        }
        animating
    }

    pub fn elevate_dir(&self) -> (f32, f32) {
        match self.config.settings.dock_edge {
            DockEdge::Bottom => (0.0, -1.0),
            DockEdge::Top => (0.0, 1.0),
            DockEdge::Left => (1.0, 0.0),
            DockEdge::Right => (-1.0, 0.0),
        }
    }

    pub fn layout(&self) -> Vec<(f32, f32, f32)> {
        let s = &self.config.settings;
        let icon_size = self.effective_icon_size();
        let cross_total = self.thickness() as f32;
        let edge_padding = V_EDGE_PADDING * s.dock_scale;
        let baseline = match s.dock_edge {
            DockEdge::Bottom | DockEdge::Right => cross_total - edge_padding,
            DockEdge::Top | DockEdge::Left => edge_padding,
        };
        let grows_positive = matches!(s.dock_edge, DockEdge::Top | DockEdge::Left);
        let vertical = self.is_vertical();

        self.icons
            .iter()
            .map(|icon| {
                let w = icon_size * icon.scale;
                let cross = if grows_positive {
                    baseline + w / 2.0
                } else {
                    baseline - w / 2.0
                };
                if vertical {
                    (cross, icon.x, w)
                } else {
                    (icon.x, cross, w)
                }
            })
            .collect()
    }

    pub fn icon_at(&self, x: f64, y: f64) -> Option<usize> {
        for (i, (cx, cy, w)) in self.layout().iter().enumerate() {
            let half = w / 2.0;
            if (x as f32) >= cx - half
                && (x as f32) <= cx + half
                && (y as f32) >= cy - half
                && (y as f32) <= cy + half
            {
                return Some(i);
            }
        }
        None
    }
}

#[cfg(test)]
mod icon_at_y_drag_tests {
    use super::*;
    use crate::config::{DockEdge, PinnedApp};

    /// Dock con tres íconos fijos en posiciones conocidas.
    fn dock_con_tres_iconos() -> Dock {
        let mut config = Config::default();
        config.settings.dock_edge = DockEdge::Bottom;
        let mut dock = Dock::new(config);
        dock.icons = [("a", 50.0), ("b", 150.0), ("c", 250.0)]
            .into_iter()
            .map(|(name, x)| DockIcon {
                app: PinnedApp {
                    name: name.to_string(),
                    icon: name.to_string(),
                    exec: String::new(),
                },
                scale: 1.0,
                target_scale: 1.0,
                x,
                target_x: x,
            })
            .collect();
        dock
    }

    fn nombres(dock: &Dock) -> Vec<&str> {
        dock.icons.iter().map(|i| i.app.name.as_str()).collect()
    }

    /// C5 (trampa 10): el centro que dibuja `layout()` tiene que ser el que devuelve
    /// `icon_at`, que es el hit test del click y del arrastre. El hueco entre dos íconos no
    /// es de ninguno (devolver el vecino cambiaría el gesto de "soltar en el hueco").
    #[test]
    fn el_centro_de_cada_icono_cae_en_su_indice() {
        let dock = dock_con_tres_iconos();
        for (i, (cx, cy, _)) in dock.layout().iter().enumerate() {
            assert_eq!(dock.icon_at(*cx as f64, *cy as f64), Some(i), "ícono {i}");
        }
        let (_, cy, _) = dock.layout()[0];
        assert_eq!(
            dock.icon_at(100.0, cy as f64),
            None,
            "el hueco entre el 0 y el 1 no es de nadie"
        );
        assert_eq!(dock.icon_at(5000.0, 5000.0), None, "y bien lejos tampoco");
    }

    /// C5: `drag_to` mueve el ícono arrastrado y REORDENA al slot cuyo centro está más
    /// cerca del puntero, dejando `dragging_index` apuntando al ícono movido (si no, el
    /// movimiento siguiente arrastraría otro). Los centros de los slots salen de
    /// `rest_centers()`, que es la misma cuenta que usa el reordenamiento: el test no puede
    /// inventarse la geometría.
    #[test]
    fn arrastrar_reordena_y_el_indice_sigue_al_icono() {
        let mut dock = dock_con_tres_iconos();
        // ----- sin arrastre en curso no hace nada -----
        dock.drag_to(999.0, 999.0);
        assert_eq!(nombres(&dock), vec!["a", "b", "c"]);

        let centros: Vec<f32> = dock.rest_centers().collect();
        let (_, cy, _) = dock.layout()[0];
        // ----- arrastro "a" hasta el slot de más a la derecha -----
        let ultimo = centros.len() - 1;
        dock.dragging_index = Some(0);
        dock.drag_to(centros[ultimo], cy);
        assert_eq!(
            dock.dragging_index,
            Some(ultimo),
            "cae en el slot del cursor"
        );
        assert_eq!(
            nombres(&dock),
            vec!["b", "c", "a"],
            "el arrastrado va al final y los otros conservan su orden"
        );
        // ----- y traerlo de vuelta lo devuelve al principio -----
        dock.drag_to(centros[0], cy);
        assert_eq!(dock.dragging_index, Some(0));
        assert_eq!(nombres(&dock), vec!["a", "b", "c"]);
    }
}
