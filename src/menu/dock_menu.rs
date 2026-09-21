use super::*;

pub fn build_category_controls(category: MenuCategory, settings: &DockSettings) -> Vec<Control> {
    let mut controls = Vec::new();
    let mut y = MENU_PADDING;
    match category {
        MenuCategory::Layout => {
            controls.push(Control {
                kind: ControlKind::EdgePicker,
                y,
                height: EDGE_PICKER_H,
            });
            y += EDGE_PICKER_H + ROW_GAP;
            controls.push(Control {
                kind: ControlKind::AlignPicker,
                y,
                height: ALIGN_PICKER_H,
            });
            y += ALIGN_PICKER_H + ROW_GAP;
            for id in LAYOUT_SETTINGS {
                push_setting_row(&mut controls, &mut y, id);
            }
        }
        MenuCategory::Appearance => {
            for id in APPEARANCE_SETTINGS {
                push_setting_row(&mut controls, &mut y, id);
            }
            y += ROW_GAP * 2.0;
            controls.push(Control {
                kind: ControlKind::Section("Dock Font"),
                y,
                height: SECTION_LABEL_HEIGHT,
            });
            y += SECTION_LABEL_HEIGHT;
            controls.push(Control {
                kind: ControlKind::DockFontDropdown,
                y,
                height: FONT_DROPDOWN_H,
            });
            y += FONT_DROPDOWN_H + ROW_GAP * 2.0;

            controls.push(Control {
                kind: ControlKind::Section("System Font"),
                y,
                height: SECTION_LABEL_HEIGHT,
            });
            y += SECTION_LABEL_HEIGHT;
            controls.push(Control {
                kind: ControlKind::SystemFontDropdown,
                y,
                height: FONT_DROPDOWN_H,
            });
            y += FONT_DROPDOWN_H + ROW_GAP * 2.0;

            controls.push(Control {
                kind: ControlKind::Section("Wallpaper Folder"),
                y,
                height: SECTION_LABEL_HEIGHT,
            });
            y += SECTION_LABEL_HEIGHT;
            controls.push(Control {
                kind: ControlKind::Button(ButtonKind::WallpaperDir),
                y,
                height: BUTTON_HEIGHT,
            });
        }
        MenuCategory::Colors => {
            controls.push(Control {
                kind: ControlKind::Section("Matugen Style"),
                y,
                height: SECTION_LABEL_HEIGHT,
            });
            y += SECTION_LABEL_HEIGHT;
            controls.push(Control {
                kind: ControlKind::SchemeDropdown,
                y,
                height: SCHEME_DROPDOWN_H,
            });
            y += SCHEME_DROPDOWN_H + ROW_GAP * 2.0;

            push_setting_row(&mut controls, &mut y, SettingId::MatugenApps);
            push_setting_row(&mut controls, &mut y, SettingId::MatugenLight);
            y += ROW_GAP;

            controls.push(Control {
                kind: ControlKind::Section("Color Theme"),
                y,
                height: SECTION_LABEL_HEIGHT,
            });
            y += SECTION_LABEL_HEIGHT;
            let theme_h = theme_picker_layout(settings, DOCK_MENU_RIGHT_COL_W, y).total_h - y;
            controls.push(Control {
                kind: ControlKind::ThemePicker,
                y,
                height: theme_h,
            });
            y += theme_h + ROW_GAP * 2.0;

            controls.push(Control {
                kind: ControlKind::Button(ButtonKind::CreatePalette),
                y,
                height: BUTTON_HEIGHT,
            });
        }
        MenuCategory::CustomPalette => {
            controls.push(Control {
                kind: ControlKind::Button(ButtonKind::Back),
                y,
                height: BUTTON_HEIGHT,
            });
            y += BUTTON_HEIGHT + ROW_GAP * 2.0;

            controls.push(Control {
                kind: ControlKind::Section("Create Your Own Palette"),
                y,
                height: SECTION_LABEL_HEIGHT,
            });
            y += SECTION_LABEL_HEIGHT;

            controls.push(Control {
                kind: ControlKind::NameField,
                y,
                height: HEX_FIELD_H,
            });
            y += HEX_FIELD_H + ROW_GAP * 2.0;

            controls.push(Control {
                kind: ControlKind::Section("Select Palette Mode"),
                y,
                height: SECTION_LABEL_HEIGHT,
            });
            y += SECTION_LABEL_HEIGHT;
            controls.push(Control {
                kind: ControlKind::PaletteModePicker,
                y,
                height: PALETTE_MODE_PICKER_H,
            });
            y += PALETTE_MODE_PICKER_H + ROW_GAP * 2.0;

            for i in 0..CUSTOM_HEX_LABELS.len() {
                controls.push(Control {
                    kind: ControlKind::HexField(i),
                    y,
                    height: HEX_FIELD_H,
                });
                y += HEX_FIELD_H + ROW_GAP;
                if i == 1 {
                    y += ROW_GAP;
                    controls.push(Control {
                        kind: ControlKind::Section("Panel Blend (base-accent)"),
                        y,
                        height: SECTION_LABEL_HEIGHT,
                    });
                    y += SECTION_LABEL_HEIGHT;
                    controls.push(Control {
                        kind: ControlKind::PanelBlendPicker,
                        y,
                        height: PANEL_BLEND_PICKER_H,
                    });
                    y += PANEL_BLEND_PICKER_H + ROW_GAP * 2.0;
                }
            }
            y += ROW_GAP;

            controls.push(Control {
                kind: ControlKind::Section("if you're unsure of what colors you put visit"),
                y,
                height: SECTION_LABEL_HEIGHT,
            });
            y += SECTION_LABEL_HEIGHT;
            controls.push(Control {
                kind: ControlKind::Section("https://coolors.co/ :D"),
                y,
                height: SECTION_LABEL_HEIGHT,
            });
            y += SECTION_LABEL_HEIGHT + ROW_GAP * 2.0;

            controls.push(Control {
                kind: ControlKind::Button(ButtonKind::SavePalette),
                y,
                height: BUTTON_HEIGHT,
            });
        }
        MenuCategory::Widgets => {
            push_setting_row(&mut controls, &mut y, SettingId::WidgetScale);
            y += ROW_GAP * 2.0;
            let layout = widget_chip_layout(settings, DOCK_MENU_RIGHT_COL_W, y);
            controls.push(Control {
                kind: ControlKind::WidgetChips,
                y,
                height: layout.total_h - y,
            });
            y = layout.total_h + ROW_GAP * 2.0;
            push_widget_editor_rows(&mut controls, &mut y, settings);
            y += ROW_GAP;
            push_setting_row(&mut controls, &mut y, SettingId::MediaSmoothScroll);
            push_note_row(
                &mut controls,
                &mut y,
                "turning this off might help if ur seeking 0 cpu usage><",
            );
            push_setting_row(&mut controls, &mut y, SettingId::MediaWidthScale);
            push_note_row(
                &mut controls,
                &mut y,
                "you might wanna be careful wtih this ... ",
            );
        }
        MenuCategory::System => {
            // ----- los cinco paneles del overlay, accesibles desde acá: el panel de
            // ajustes comparte la superficie y tapa la banda de pestañas, así que sin
            // esto no hay forma de abrir el launcher/ventanas/portapapeles/notifs/fondos
            // desde el propio panel -----
            for kind in [
                ButtonKind::OpenAppLauncher,
                ButtonKind::OpenWindowSwitcher,
                ButtonKind::OpenClipboard,
                ButtonKind::OpenNotifications,
                ButtonKind::OpenWallpapers,
                ButtonKind::AddApp,
            ] {
                controls.push(Control {
                    kind: ControlKind::Button(kind),
                    y,
                    height: BUTTON_HEIGHT,
                });
                y += BUTTON_HEIGHT + ROW_GAP;
            }
            controls.push(Control {
                kind: ControlKind::Button(ButtonKind::QuitDock),
                y,
                height: BUTTON_HEIGHT,
            });
        }
    }
    controls
}

/// Filas del editor del tab "Widgets". La escala de letra vale para cualquier
/// widget; cada opción propia se suma acá (hoy sólo el formato del reloj).
///
/// Sin selección queda sólo la nota que explica cómo elegir: así el tab no
/// enseña filas que no hacen nada.
fn push_widget_editor_rows(controls: &mut Vec<Control>, y: &mut f32, settings: &DockSettings) {
    controls.push(Control {
        kind: ControlKind::Section("Selected Widget"),
        y: *y,
        height: SECTION_LABEL_HEIGHT,
    });
    *y += SECTION_LABEL_HEIGHT;
    let Some(kind) = settings.selected_widget else {
        push_note_row(controls, y, "click a chip above to edit that widget");
        return;
    };
    push_note_row(controls, y, widget_label(kind));
    // ----- cada fila aparece sólo donde el widget la mira: la letra en los que
    // dibujan texto, el formato en el reloj y la bocina en el volumen. Un
    // widget sólo-símbolo (Tray, Workspaces) no gana nada con un slider de
    // letra, así que no se le ofrece. -----
    if crate::widget::widget_has_text(kind) {
        push_setting_row(controls, y, SettingId::WidgetFontScale);
    }
    if matches!(kind, WidgetKind::Volume) {
        push_setting_row(controls, y, SettingId::WidgetIconScale);
    }
    if matches!(kind, WidgetKind::Clock) {
        push_setting_row(controls, y, SettingId::WidgetClockFormat);
    }
}

pub fn dock_menu_content_height(category: MenuCategory, settings: &DockSettings) -> f32 {
    if category == MenuCategory::Widgets {
        // ----- el editor del widget elegido agrega filas, así que el alto sale
        // del contenido. El piso es el de siempre para que el área de chips no
        // se encoja al aparecer o desaparecer el editor. -----
        let controls = build_category_controls(category, settings);
        let natural = controls.last().map(|c| c.y + c.height).unwrap_or(0.0);
        return natural.max(WIDGETS_TAB_FIXED_H) + MENU_PADDING;
    }
    let controls = build_category_controls(category, settings);
    let natural = controls.last().map(|c| c.y + c.height).unwrap_or(0.0);
    // ----- dropdown clipping -----
    let dropdown_bottom = controls
        .iter()
        .filter_map(|ctl| {
            let overlay_h = match ctl.kind {
                ControlKind::SchemeDropdown => {
                    scheme_picker_layout(DOCK_MENU_RIGHT_COL_W, 0.0).total_h
                }
                ControlKind::DockFontDropdown | ControlKind::SystemFontDropdown => {
                    FONT_LIST_MAX_ROWS as f32 * FONT_ROW_H
                }
                _ => return None,
            };
            Some(ctl.y + ctl.height + 4.0 + overlay_h)
        })
        .fold(0.0_f32, f32::max);
    let sidebar_min = MENU_PADDING * 2.0 + MENU_CATEGORIES.len() as f32 * DOCK_MENU_TAB_H;
    natural.max(dropdown_bottom).max(sidebar_min) + MENU_PADDING
}

#[cfg(test)]
mod widget_editor_tests {
    use super::*;

    fn filas(kind: Option<WidgetKind>) -> Vec<SettingId> {
        let settings = DockSettings {
            selected_widget: kind,
            ..Default::default()
        };
        build_category_controls(MenuCategory::Widgets, &settings)
            .into_iter()
            .filter_map(|c| match c.kind {
                ControlKind::Slider(id) | ControlKind::Toggle(id) => Some(id),
                _ => None,
            })
            .collect()
    }

    /// Cada opción por widget sale SÓLO para el widget que la mira: el reloj tiene
    /// la letra y el formato, el volumen la letra y la bocina. Es lo que evita
    /// mostrar filas que no hacen nada.
    #[test]
    fn cada_widget_muestra_sus_filas() {
        let sin_elegir = filas(None);
        assert!(
            !sin_elegir.contains(&SettingId::WidgetFontScale),
            "sin widget elegido no hay a quién editarle la letra"
        );

        let reloj = filas(Some(WidgetKind::Clock));
        assert!(reloj.contains(&SettingId::WidgetFontScale));
        assert!(reloj.contains(&SettingId::WidgetClockFormat));
        assert!(
            !reloj.contains(&SettingId::WidgetIconScale),
            "el reloj no tiene símbolo propio"
        );

        let volumen = filas(Some(WidgetKind::Volume));
        assert!(volumen.contains(&SettingId::WidgetFontScale));
        assert!(volumen.contains(&SettingId::WidgetIconScale));
        assert!(
            !volumen.contains(&SettingId::WidgetClockFormat),
            "el formato de 24 h es sólo del reloj"
        );

        // ----- el caso que lo motivó: Workspaces son puntos, así que un slider de
        // letra no le cambia nada -----
        let workspaces = filas(Some(WidgetKind::Workspaces));
        assert!(
            !workspaces.contains(&SettingId::WidgetFontScale),
            "Workspaces no dibuja texto: no tiene que ofrecer la fila de letra"
        );
    }

    /// La fila de letra sale EXACTAMENTE donde la tabla dice que hay texto. Si
    /// alguien agrega un widget con texto y se olvida del flag, esto lo delata.
    #[test]
    fn la_fila_de_letra_sigue_a_la_tabla() {
        for kind in crate::widget::widget_kind_order()
            .into_iter()
            .chain([WidgetKind::Custom(0), WidgetKind::Custom(9)])
        {
            assert_eq!(
                filas(Some(kind)).contains(&SettingId::WidgetFontScale),
                crate::widget::widget_has_text(kind),
                "{kind:?}: la fila de letra no coincide con la tabla"
            );
        }
        // ----- y la tabla cubre los dos casos, o el test de arriba no probaría nada -----
        assert!(crate::widget::widget_has_text(WidgetKind::Clock));
        assert!(!crate::widget::widget_has_text(WidgetKind::Workspaces));
    }

    /// El editor agranda el panel: si el alto no lo acompaña, las filas nuevas
    /// quedan fuera de la superficie (el panel no scrollea).
    #[test]
    fn el_editor_agranda_el_panel() {
        let alto = |kind: Option<WidgetKind>| {
            dock_menu_content_height(
                MenuCategory::Widgets,
                &DockSettings {
                    selected_widget: kind,
                    ..Default::default()
                },
            )
        };
        assert!(
            alto(Some(WidgetKind::Volume)) > alto(None),
            "elegir el volumen no agrandó el panel"
        );
        assert!(
            alto(Some(WidgetKind::Clock)) > alto(None),
            "elegir el reloj no agrandó el panel"
        );
    }

    /// El tab arma sus filas para CUALQUIER widget, incluido un `Custom` fuera
    /// de rango: el editor no puede paniquear al dibujar el panel.
    #[test]
    fn el_tab_se_arma_para_todos_los_widgets() {
        for kind in crate::widget::widget_kind_order()
            .into_iter()
            .chain([WidgetKind::Custom(0), WidgetKind::Custom(9)])
        {
            let settings = DockSettings {
                selected_widget: Some(kind),
                ..Default::default()
            };
            assert!(
                !build_category_controls(MenuCategory::Widgets, &settings).is_empty(),
                "{kind:?} quedó sin controles"
            );
            assert!(
                dock_menu_content_height(MenuCategory::Widgets, &settings) > 0.0,
                "{kind:?} quedó sin alto"
            );
        }
    }
}

#[cfg(test)]
mod system_tab_tests {
    use super::*;

    fn botones() -> Vec<ButtonKind> {
        build_category_controls(MenuCategory::System, &DockSettings::default())
            .into_iter()
            .filter_map(|c| match c.kind {
                ControlKind::Button(b) => Some(b),
                _ => None,
            })
            .collect()
    }

    /// El panel de ajustes comparte la superficie y tapa la banda de pestañas del
    /// overlay, así que los cinco paneles tienen que poder abrirse desde acá: si
    /// falta uno, no hay ninguna otra forma de llegar (era el caso de Clipboard,
    /// Notifs y Wallpapers).
    #[test]
    fn el_tab_system_expone_los_cinco_paneles_del_overlay() {
        let b = botones();
        for kind in [
            ButtonKind::OpenAppLauncher,
            ButtonKind::OpenWindowSwitcher,
            ButtonKind::OpenClipboard,
            ButtonKind::OpenNotifications,
            ButtonKind::OpenWallpapers,
        ] {
            assert!(b.contains(&kind), "falta {kind:?}: {b:?}");
        }
        // ----- y siguen estando los de siempre -----
        assert!(b.contains(&ButtonKind::AddApp));
        assert!(b.contains(&ButtonKind::QuitDock));
    }

    /// `label()` es un `match` sin `_`, así que compilar ya obliga a darle texto a
    /// cada variante nueva; acá se fija el contrato y que ninguno sea destructivo.
    #[test]
    fn los_botones_nuevos_tienen_label() {
        for kind in [
            ButtonKind::OpenAppLauncher,
            ButtonKind::OpenWindowSwitcher,
            ButtonKind::OpenClipboard,
            ButtonKind::OpenNotifications,
            ButtonKind::OpenWallpapers,
        ] {
            assert!(!kind.label().is_empty(), "{kind:?} sin label");
            assert!(!kind.is_destructive(), "{kind:?} no es destructivo");
        }
    }
}
