# AGENTS.md

Instrucciones para agentes que trabajen en este repo. El **"Qué se hizo"** del
final resume el trabajo de la sesión que originó este archivo.

## Qué es

Dock/barra para Wayland en Rust. Este repo es un fork de `Volcha-Z/Docky.rs`: el
compositor de referencia acá es **niri**; Hyprland sigue soportado
(`src/compositor.rs`).

Setup con el que se desarrolló: niri 26.04, una salida `eDP-1` 1920x1080 (scale 1),
Lenovo, proyecto en `~/Projects/Docky.rs`.

## Comandos

```sh
# compilar — SIEMPRE antes de reiniciar la app (ver trampa 5)
cargo build --release

# correr (setsid: la app sobrevive a la shell que la lanza)
pkill -x dockyrs
setsid ./target/release/dockyrs >/tmp/dockyrs.log 2>&1 </dev/null &

# con logs (RUST_LOG=debug suelto inunda zbus)
RUST_LOG="debug,zbus=warn,tracing=warn,sctk=info" \
  ./target/release/dockyrs >/tmp/menu.log 2>&1

# tests y lints
cargo test --release
cargo clippy --release --all-targets
```

El perfil dev tiene `opt-level = 1` con las dependencias en 3: el debug crudo era
50-100x más lento (10-20 ms por frame contra 2 ms).

IPC: `./target/release/dockyrs --toggle-dock-menu` (también `--toggle-search`,
`--osd-volume`, `--osd-brightness`, `--toggle-wallpaper`, `--toggle-clipboard`,
`--screenshot-full`, `--screenshot-region`, `--test-notification`). Abrir y cerrar
el panel por IPC es más confiable que hacerlo con puntero sintético.

## Sincronizar con el upstream

Este repo es un fork: `origin` es el fork (`NicoMosty/Docky.rs`) y `upstream` el
original (`Volcha-Z/Docky.rs`). `master` se mantiene como **espejo exacto** de
`upstream/master` y `niri-backend` es la rama de trabajo. Para traer lo del original:

```sh
git fetch upstream
git checkout master && git merge --ff-only upstream/master   # master = espejo
git checkout niri-backend && git merge master                # traer a la rama de trabajo
git push                                                     # publicar el fork
```

Si el `--ff-only` rechaza, es porque alguien commiteó en `master` por error: eso rompe
la propiedad de espejo, conviene revisarlo antes de mergear.

## Trampas (todas costaron sangre)

1. **Una superficie compartida para el dock y sus modos.** El dock y cada modo (panel de ajustes,
   OSD, HUD) usan `self.layer`; el popup del tray/volumen (`dock_popup.rs:200`) y
   el selector de screenshot (`screenshot/mod.rs:79`) tienen superficie propia. Si un modo cambia el tamaño TIENE que
   anotarlo en `applied_size`/`applied_geom` (`apply_panel_size`) o restaurarlo con
   `relayout_dock` al cerrar: `sync_autohide_surfaces` reaplica anchor/margin pero
   **nunca** el tamaño. De acá salieron dos bugs de "dock descentrado".
2. **Nunca desmapear la superficie compartida del dock.** `attach(NULL)` hace que niri resetee el
   tamaño de la layer a 0 y el cliente muere por error de protocolo. Ocultar =
   buffer transparente (`draw_hidden`). El popup y el selector SÍ se desmapean al cerrar
   (`dock_popup.rs:256,418`, `screenshot/region.rs:175`): son superficies descartables
   que se re-crean al abrirse.
3. **La superficie del dock es el disparador del autohide**: no hay hot corner ni
   superficie aparte. Con la input region activa, `Enter`/`Motion` revela y `Leave`
   arma el ocultado corto (`LEAVE_HIDE_MS = 150`), que cualquier `Enter`/`Motion`
   cancela.
4. **Al mover el panel debajo del dock** la superficie pasó a ser
   `[dock][8px][panel]`: las coordenadas del puntero son de la SUPERFICIE, así que
   el handler las traduce con `panel_local()` o los controles quedan muertos.
5. **`cargo test` y `cargo clippy` NO regeneran `target/release/dockyrs`.** Si se
   reinicia sin `cargo build --release` se prueba el binario viejo y se reporta
   como arreglado algo que nunca corrió. Verificar fechas:
   `ls -l --time-style=+%H:%M:%S target/release/dockyrs` contra las fuentes, y
   `ps -o lstart= -p $(pgrep -x dockyrs)` contra el binario. Caso real 13-sep: el
   binario de las 11:32 corrió con fuentes del panel de volumen editadas a las 11:36
   y niri mató al cliente al abrir el popup (`error in client communication`); con
   rebuild no se reproduce.
6. **El orden de los widgets** vive en un único `settings.widgets`: el orden dentro
   de cada slot (Left/Middle/Right) es el de ese vector. `place_widget` inserta en
   la fila pedida; un `push` mandaba todo al final.
7. **pi-lens a veces sirve diagnósticos cacheados.** `cargo build`/`clippy` son la
   autoridad; editar código para contentar un blocker viejo ya causó churn.
8. **No editar la config de niri del usuario** salvo pedido explícito.
9. **El teclado de la superficie compartida es global mientras dura un modo.**
   `Exclusive` es lo que hace que Escape funcione, pero si un camino cierra un modo
   sin soltarlo, NINGUNA tecla llega a ninguna aplicación. `enforce_keyboard()`
   (llamado desde `draw_ex`) re-aplica la interactividad en cada frame según los
   modos abiertos y los menús: no agregues caminos que la seteen a mano sin pasar
   por ahí.
10. **Los hit tests reparten los widgets con OTRA escala que el dibujo** (costó una
   sesión entera y dos diagnósticos equivocados). `draw_widgets` hace
   `render_scale = render_scale * s.widget_scale` antes de `layout_widgets`, y
   `base_size()` también cuenta `widget_scale` (viene de
   `widget_bar_natural_len(..., widget_scale)`). Los hit tests reciben coordenadas
   **lógicas** del puntero (sin `output_scale`), así que les toca `widget_scale`
   pelado. Por eso para pedir el layout de un hit test se usa
   **`render::hit_layout()`**, no `layout_widgets` directo (si necesitás la escala
   suelta, `render::hit_scale()`); con el `1.0` que tenían cinco lugares, los rects
   quedaban corridos: con `widget_scale = 1.2166` el click caía ~30px a la
   izquierda del icono de WiFi, ~46px del de "EN", y en el clúster derecho
   (tray/reloj) al revés porque ese bloque se ancla al borde — o sea que el click
   derecho sobre el tray caía fuera del ícono y no abría nada. Ojo: **con
   `widget_scale = 1.0` (el default) el bug es invisible**, solo aparece al
   tocarlo.
   Guard: `render::layout::hit_layout_tests` fija el contrato (los rects del hit
   test son iguales a los del dibujo, y con escala 1.0 se corren >10px). Los tres
   tests fallan si `hit_scale` vuelve a `1.0` — verificado revirtiéndolo a mano.
   Diagnóstico: `dock: configure new_size=… base=…` + `popup: … center=…` en el log,
   contra la posición del icono medida en un screenshot; el log solo NO alcanza,
   porque devuelve el hit que la propia app calculó.

11. **La rueda del volumen: dos trampas de signo y de métrica.**

- El eje vertical de `wl_pointer` es **positivo hacia abajo**, así que
     `discrete` negativo es rueda ARRIBA. Con el signo al revés la rueda hace lo
     contrario de lo que espera el usuario; se verifica con
     `pointer.py --scroll up` (que manda `REL_WHEEL +1`). Guard:
     `app::pointer::wheel_tests`.
- `f64::signum` **no sirve** para descartar el cero: `signum(+0.0)` es `1.0`, así
     que un evento de touchpad en cero movía el volumen. El cero se chequea aparte.
- `text_width_estimate_render` (0.56 em/char) subestima ~20% las mayúsculas
     anchas: "MUTE" medía ~23px contra los 19 estimados y se salía de la pastilla.
     Las etiquetas del volumen usan otro factor (`volume_label_len`).

12. **La geometría de un widget que la calculan el reparto y el dibujo va en UNA
   función** (`volume_content_len`, `VOLUME_ICON_*`), no duplicada: el ancho que
   reserva `widget_natural_len` y el content que centra `draw_icon_label` tienen
   que coincidir o el contenido se sale de la pastilla (le pasó al altavoz: medía
   ~10 unidades contra las 6 reservadas porque su `s` interno no estaba atado a
   `VOLUME_ICON_R`). Mismo criterio que `hit_scale` (trampa 10).

## Mapa del código

- `src/app/mod.rs` — `App`: campos de modo (`dock_menu_mode`, `ws_flash_mode`,
  `osd_mode`, `popup_mode`, …), geometría (`applied_size`, `applied_geom`), autohide
  (`last_ptr_event`, `ptr_left_at`, `autohide_armed`).
- `src/app/pointer.rs`, `handlers.rs` — despacho por superficie: cada modo
  intercepta antes de la rama general (`if self.x_mode.is_some() { …; return; }`).
- `src/app/draw.rs` — `draw_ex` elige el modo, `relayout_dock` fija el tamaño,
  `draw_icons` dibuja el dock normal.
13. **La tabla de widgets: la exhaustividad del compilador pasó a ser un test.**
   `src/render/widget_spec.rs` tiene `WIDGETS`, la única lista de widgets del
   render: un `WidgetSpec` por widget con su `natural_len` y su `draw`. Antes había
   dos `match` de 12 ramas (en `widget_natural_len` y en `draw_widgets`); ahora hay
   `spec_for(kind)` y un `unreachable!` con el nombre si falta. Agregar un widget
   son **4 lugares**: el enum, `WIDGETS`, `WIDGET_KIND_ORDER` y `widget_label` (los
   dos últimos en `menu/widget_chips.rs`). Antes eran 16 sitios en 7 archivos
   (medido con `KbdLayout`, commit `c458821`).
   - **Lo que dejó de estar garantizado:** sin `match` no hay exhaustividad, así que
     si el enum crece y la tabla no, nada avisa. Lo cubre
     `render::layout::hit_layout_tests::la_tabla_cubre_todos_los_widgets_del_panel`,
     que itera `WIDGET_KIND_ORDER` **y** exige que `WIDGETS` y el panel de ajustes
     tengan el mismo tamaño. Los otros dos lados ya estaban cubiertos:
     `widget_label` es un `match` sin `_`, así que sumar una variante es error de
     compilación, y el `assert_eq!` de tamaños agarra la desincronización en los
     dos sentidos.
   - **El dibujo recibe el `WidgetRect` que produjo `layout_widgets`**, así que la
     trampa 10 pasa a ser estructural en vez de sostenida por un test.
   - **`WidgetSnapshot` no se toca.** Derivar `PartialEq` para colapsar
     `refresh_sys`/`refresh_clock` obligaría a clonar el struct entero (12 campos
     con `Vec` y `String`) cada 2 s sólo para comparar; el diff a mano compara los
     campos que acaba de releer.
   - **`Canvas` lleva DOS flags de animación**: `advance` (marquee de Media) y
     `advance_ws` (animación de Workspaces). No son el mismo tick.
   - Las funciones de `render/` **siguen** recibiendo `(zx, zy, zw, zh)` desarmado
     (19 firmas, 152 líneas de plumbing). Unificarlas a `&WidgetRect` ahorra ~95
     líneas pero toca ~57 sitios y no previene ningún fallo — la invariante ya vive
     en el adaptador. Medido y descartado: no repetir el análisis.

14. **El workspace: `cargo test` en la raíz NO testea el crate.**
   Con `crates/dockyrs-canvas` como miembro, `cargo test --release` en la raíz corre
   sólo el paquete raíz, así que los guards del crate quedan invisibles para el
   comando que documenta este archivo. Lo arregla
   `default-members = [".", "crates/dockyrs-canvas"]` en el `Cargo.toml` raíz. Dos
   más del mismo commit: **NO** poner `panic = "abort"` en `[profile.release]`
   (rompe `catch_unwind`, que es la red de seguridad del reexec A5/A6; `strip = true`
   sí es seguro y baja 2,8 MB medidos), y el `*` de `[profile.dev.package]` **no
   alcanza a los miembros del workspace**, así que el crate del raster se compila en
   `opt-level = 1`.


- `src/app/dock_menu.rs`, `dock_menu_input.rs` — panel de ajustes: `panel_layout`
  (reparto superficie/dock/panel), `panel_local` (traducción de coordenadas),
- `src/render/widget_spec.rs` — la tabla `WIDGETS`: la única lista de widgets del
  render, con la medida y el dibujo de cada uno (trampa 13).
- `crates/dockyrs-canvas/` — raster por CPU sobre tiny-skia: texto (`text.rs`),
  iconos (`icon_cache.rs`) y miniaturas (`thumbnail_cache.rs`). Sin GPU ni contexto
  GL: el costo de rasterizar por CPU sólo se paga cuando hay algo que redibujar,
  que es por qué el reposo está en el piso (1 tick de sistema cada 5 s).

  hit tests y el drag de widgets (`drop_widget_chip`).
- `src/app/ws_flash.rs` — HUD de workspaces.
- `src/app/volume_panel.rs` — panel de volumen: se abre en la superficie del popup
  y aplica los cambios (arrastre de la barra, mute por fila, cambio de salida).
- `src/menu/volume_panel.rs` — geometría y hit tests del panel de volumen.
  `src/menu_render/volume_panel.rs` — su dibujo.
- `src/menu/` — lógica, geometría y hit tests (sin dibujo). `src/menu_render/` — el
  dibujo. Se tocan de a pares.
- `src/render/` — widgets de la barra (reloj/batería, workspaces, red, bluetooth,
  volumen, tray).
- `src/widgets.rs` — lecturas del sistema y `WidgetSnapshot`.
- `src/config.rs` — `DockSettings`, `default_widgets()`, load/save.
- `scripts/pointer.py` — puntero virtual por uinput para tests.

## Convenciones

- Comentarios y mensajes de commit en español, descriptivos, sin prefijos tipo
  `feat:` (`git log` es la referencia).
- `~/.config/dockyrs/config.json` tiene prioridad sobre `default_widgets()`:
  cambiar un valor por defecto implica tocar los dos.
- Comentario `ponytail:` para simplificaciones deliberadas con techo conocido.

## Testear sin mouse

`scripts/pointer.py` inyecta puntero por uinput y usa el propio log como sensor: se
detiene cuando aparece `reveal`, o sea cuando el puntero ya entró en la superficie
del dock, así que no necesita saber la geometría.

- `UI_DEV_SETUP = 0x405c5503`: el tamaño de `struct uinput_setup` es 92, no 4. Con
  el número mal el ioctl falla y no se emite nada; si redirigís stderr, es silencioso.
- ~3.05 px reales por paso de 3 px (aceleración de libinput, medido).
- `--extra N` recorre la barra, `--then-dx/--then-dy` encadena un segundo click,
  `--y N` fija la altura.
- **Para clickear dentro de un panel de abajo** (popup: menús del tray, panel de
  volumen) el primer click tiene que caer en el widget del dock (eso es lo que
  abre el panel) y el segundo ya puede bajar al panel con `--then-dy`. Ojo con la
  cuenta: el popup arranca debajo del dock, así que su `y` local es
  `y_dock - (thickness + MENU_GAP)` (con thickness 26 y gap 0, restá 26). Con
  `--then-dx` negativo se vuelve hacia la izquierda (zona del icono = mute).
  Arrastre (press → move → release) no lo hace `pointer.py`: está
  `/tmp/drag_test.py` (verificado con eso: el valor final es el que se soltó).
- `--shift-arrow left|right [--times N]` en vez de clickear teclea Shift+←/→ por un
  **teclado** virtual (otro id de producto, `0x5679`): es la única forma de ciclar
  los modos del overlay sin tocar el teclado real. Necesita un modo abierto (la
  superficie tiene que estar con el teclado en `Exclusive`) e imprime la línea
  `overlay: X -> Y` del log, que es la confirmación.
- `--scroll up|down [--times N]` gira la **rueda** en el punto alcanzado
  (`REL_WHEEL`): es lo que ejercita el volumen del dock (rueda = ±5% sobre el
  widget de volumen). Sin efecto si el puntero no está sobre el volumen, así que
  sirve también para barrer la barra y ubicar ese widget por su log.
- Ojo con la convención de signo: `REL_WHEEL +1` (rueda arriba) llega a la app como
  `discrete = -1`, porque en `wl_pointer` el eje vertical es positivo hacia abajo.
  Con el signo invertido la rueda hace lo contrario (ver trap 11).
- Requiere la app con `RUST_LOG=debug` y que el dock pueda ocultarse: si el panel
  quedó abierto no hay transición oculto→visible y el sensor no dispara.

Líneas de log útiles: `dock: click derecho (x,y) -> Some(Kind)`,
`dock: click (x,y) -> Some(Kind)` (izquierdo; es lo que ubica un widget por su log),
`volumen: click panel (x,y) -> Some(VolumeTrack|VolumeMute|VolumeDevice)`,
`dock: rueda (x,y) subir=bool -> Some(Kind)`,
`autohide:N reveal|hide|leave (franja)|timeout … borrowed=`,
`wsflash: show … visible= placed= borrowed=`, `popup: t=… frame=Xms`.

Para el estado de las superficies: `niri msg --json layers` expone por layer el
`namespace`, el `layer` y el `keyboard_interactivity` (sin `--json` sale en texto).
Es lo que permitió verificar que el teclado no quede preso: abrir un modo ⇒
`Exclusive`, cerrarlo ⇒ `None`.

## Qué se hizo

Commits `2ec0ed1` (barra, HUD y autohide), `bd8e40e` (panel debajo del dock +
reordenamiento de widgets) y lo posterior:

- Autohide sin reservar espacio: se eliminó la layer de reserva, queda
  `set_exclusive_zone(-1)` y la superficie propia como disparador.
- HUD de workspaces al cambiar de workspace: mismo panel y posición con sólo el
  indicador; clickeable, y al pasarle el mouse revela el dock completo.
- Panel de ajustes: sólo abre con click derecho sobre el indicador de workspaces;
  cierra con Escape o click derecho; al cerrar restaura tamaño, anclaje y el timing
  de ocultado normal.
- Panel de ajustes **debajo del dock** para ver los cambios en vivo (superficie
  compuesta dock+panel y traducción de coordenadas).
- Arreglo del reordenamiento de widgets (columna + fila) con 5 tests nuevos.
- Widgets: reloj `11-Sept`, batería con % dentro del icono (verde cargando, naranja
  conservación, rojo ≤20%), volumen compacto, bluetooth por color sin texto, red con
  pill de SSID; fuera CPU/RAM/Media.
- **Panel de volumen** (click izquierdo en el widget de volumen): se abre debajo del
  widget, en la superficie del popup (la misma de los menús del tray), y trae la
  fila `Output` + una fila por app que esté sonando + el selector de salida.
  Cada fila: icono = mute de esa fila, y el resto de la fila ajusta (click o
  **arrastre** — press + motion + release, con el valor final aplicado al soltar).
  La salida por defecto sigue con wpctl (es lo que mide el widget) y los streams
  con `pactl -f json list sink-inputs` / `set-sink-input-volume` / `-mute`: wpctl no
  tiene forma estable de agrupar las patas del mismo stream. El arrastre manda un
  cambio cada 50ms (`VOLUME_APPLY_MS`) y el valor dibujado se actualiza en cada
  paso. La rueda sobre el widget sigue siendo ±5% y el click derecho pavucontrol.
  Se relee cada 2s desde el tick de sistema (`refresh_volume_panel`), porque las
  apps aparecen y desaparecen sin avisar. Tests: `widgets::volume_panel_tests`
  (parseo de los JSON de pactl), `menu::volume_panel::volume_panel_tests`
  (geometría, hit tests, `volume_pct_from_x`).
- Perf: caché de familias de fuentes, opt-levels en dev, redibujado sólo cuando
  cambia el hover, una conexión D-Bus compartida para el tray.
- Doble click izquierdo en el dock abre el selector de fondos **sólo en la banda
  central** (220px, `WALLPAPER_BAND` en `pointer.rs`): en los extremos se disparaba
  sin querer. Hay además un keybind de niri (`Mod+Shift+W`) en la config del usuario.
- Fondo de pantalla: `apply_wallpaper` ya no asume swww/awww. `wallpaper_program()`
  lee la config del compositor (`swaybg`, `awww`, `swww`, `hyprpaper`) y actúa en
  consecuencia: con swaybg escribe `~/.config/wallpaper-path` y arranca
  `wallpaper-change.service` (el pipeline típico: reinicia swaybg y corre matugen);
  si ese pipeline no existe, reinicia swaybg a mano. `current_wallpaper_path()` lee
  ese archivo, así el resaltado del fondo actual funciona también con swaybg.
- El selector de fondos cierra **inmediato** (antes dependía del tick de la
  animación, y si el tick no corría Escape no cerraba) y `enforce_keyboard()` es
  idempotente: re-setear la interactividad en cada frame podía perder el foco.
- Escaneo del selector: escanea **una** carpeta, la del ajuste `wallpaper_dir`
  (vacío = `~/Pictures/Wallpapers`, la de fábrica). Se elige desde el panel, en
  Appearance: el botón `ButtonKind::WallpaperDir` cicla entre `WALLPAPER_DIRS` y
  guarda la elegida; `wallpaper_dir` acepta `~/ruta` o una ruta absoluta si se edita
  a mano el config. Ojo con agregar carpetas genéricas (`~/Pictures`, `~/Downloads`):
  traen logos, bocetos y fotos sueltas que, ordenados por nombre, tapan los fondos de
  verdad — probado, el selector abría mostrando "A", "B", "C". Tests:
  `wallpaper::scan_tests`.
- Launcher de apps, con el comportamiento de rofi: historial de uso (`src/usage.rs`,
  `~/.cache/dockyrs/app-usage.json`) que ordena por frecuencia cuando la caja está
  vacía, matching difuso (prefijo > inicio de palabra > contiene > subsecuencia),
  `Keywords=`, `Name[es]`, `Terminal=true` (lanza dentro de `$TERMINAL`) y filtro
  `OnlyShowIn`/`NotShowIn`. Tests: `desktop::entry_tests`, `usage::usage_tests`.
- Modos del overlay: `Shift+←/→` cicla launcher → portapapeles → fondos → ventanas
  (`cycle_overlay` en `app/mod.rs`, interceptado **antes** de armar `held_key` en
  `handlers.rs`; aunque eso NO frena el auto-repeat: cada repetición vuelve a entrar a la rama y cicla en bucle — pendiente en AUDIT.md B1). Los tres
  primeros reusan sus modos existentes y se cierran por su propio camino; "ventanas"
  es la misma lista del launcher con `SearchList::Windows`, y Enter enfoca por id de
  niri en vez de lanzar.
- Los cierres del launcher y del portapapeles también son **inmediatos** (mismo
  criterio que el panel de ajustes y el selector de fondos): así un cambio de modo no
  cruza la animación de salida con la de entrada.
- Barra de pestañas del overlay: `OVERLAY_TABS` + `OVERLAY_TABS_H` en `menu/mod.rs`,
  dibujada por `menu_render/tabs.rs`
  (`draw_overlay_tabs`, llamada desde `draw_content`/`draw_app_search`/`draw_clipboard`
  cuando `DrawArgs.overlay_tabs`/`ClipArgs.overlay_tabs` trae el índice de
  `OverlayMode::tab_index()`, o sea la posición en `OVERLAY_ORDER`). Es una banda
  arriba de cada panel (pestaña activa con pill de acento) y el contenido arranca
  debajo: los offsets salen de las mismas funciones que usan los hit tests, así que
  dibujo y click no se pueden desincronizar — `app_search_strip_y()`,
  `clip_content_y()` y `wallpaper_hit_test()` (que traduce el puntero con
  `overlay_tabs_h()`). En el panel vertical angosto las cuatro etiquetas van
  apiladas (`overlay_tabs_h(true)` = 4 filas) y la banda es más alta. Los paneles
  crecen con la banda al abrirse (`build_app_search_controls`, `clip_panel_h`,
  `open_wallpaper_picker`). Tests: `menu_render::tabs`, `menu::wallpaper_picker::hit_tests`,
  `app::overlay_tabs_tests`. La banda **no es clickeable**: los modos se cambian con
  Shift+←/→ (sumarle click es el hit test de la banda + `cycle_overlay` desde el
  handler).
- `scripts/pointer.py`.
- Autostart en niri: `spawn-at-startup` en `~/.config/niri/config/5_autostart.kdl`.
  Con una sola salida, sin `--output` ni `--profile`. No arrancar
  `dockyrs-notifyd` si ya hay otro daemon de notificaciones (mako, dunst): pelean
  por `org.freedesktop.Notifications`.
- **Desalineado dibujo/clicks** (el bug de fondo de todo esto): cinco hit tests
  (`widget_hit_test`, `widget_center`, `tray_icon_hit`, `tray_icon_center`,
  `workspace_dot_hit`) y `tray_geometry` repartían con `render_scale = 1.0`
  mientras el dibujo usa `widget_scale` (ver trampa 10). Casi todos pasan por
  `render::hit_scale()` — incluido `workspace_dot_hit`, que ya no tiene su propia
  geometría en `1.0`: el reparto salió a `workspaces::slot_at()` y le pasa
  `hit_scale()` (era AUDIT.md D1, **resuelto**). Guard:
  `render::workspaces::workspace_hit_tests` — con `widget_scale = 1.2166` y 8
  workspaces el extremo se corre >10 px, el click en el centro dibujado cae en el
  slot correcto y la tolerancia queda tangente al paso, sin robarle el click al
  vecino. `ws_flash_dot_hit` reusa `slot_at` con `1.0` a propósito: el panel del
  HUD se dimensiona con `workspaces_geometry(.., 1.0)` y `draw_ws_flash` mete el
  `output_scale` en las dos puntas, así que se cancela.
  Verificado contra pixeles: el centro que el app cree que
  tiene cada widget coincide con el icono dibujado (wifi local 152 vs 151.5,
  tray 465 vs 464.5) y clickear el centro visual de cada uno da el widget
  correcto (power 23, battery 55, wifi 150, bluetooth 180, EN 210, tray 458).
- Menús del tray por click derecho (tres bugs distintos que se veían todos como
  "el click derecho no funciona"):
  - `nearest_tray_index()` en `render/tray.rs`: un click en el **hueco** entre
    iconos no devolvía nada (`rel - idx*per <= icon_side` fallaba en el gap) y se
    perdía. Ahora el hueco cae al icono más cercano.
  - `open_tray_menu_for()` en `dock_popup.rs` (por service/path, no por índice) y
    fallback: si el item no tiene `Menu` o `GetLayout` vuelve vacío, se hace
    `Activate` (lo del click izquierdo) en vez de no hacer nada.
  - **El popup no se cerraba nunca solo.** Se cerraba únicamente con un click, y
    ese click lo consumía: al primer click derecho después de un menú olvidado el
    menú no aparecía, sólo se cerraba el viejo. Ahora se cierra cuando el puntero
    no está ni en el dock ni en él: `popup_should_dismiss()` en `draw.rs`,
    evaluado desde **`autohide_timeout()`** — con un popup abierto `draw_ex` no
    corre, así que ese timeout es el único tick. `arm_popup_tick()` lo agenda
    desde el `Leave` del dock y del popup, sin depender de que el autohide esté
    encendido. `popup_hovered` (en `DockPopupMode`) evita que el salto
    icono -> menú —`Leave` + `Enter` en el mismo lote de eventos— lo cierre al
    pasar. Tests: `app::draw::popup_dismiss_tests`.
  - La superficie del popup ocupa **todo el ancho del dock** (hace falta para
    centrar el recuadro en el icono sin conocer el offset real de la superficie en
    pantalla), así que ahora se le limita la **input region al recuadro**
    (`popup_input_region()`): antes una banda invisible del ancho del dock se
    tragaba los clicks y el hover de la ventana de abajo. Se re-aplica al cambiar
    de submenú (`set_tray_items`), donde también cambia el alto.
- Wifi y bluetooth se ignoran en el tray visible (`tray_ignored()` en
  `src/tray.rs`: filtra `nm-*`, `blueman`, `bluetooth`, `blueberry` por
  `icon_name`/`path`, porque el `service` es un nombre único tipo `:1.20`) y su
  menú se abre con click derecho desde los widgets Network/Bluetooth de la
  izquierda (`open_widget_tray_menu`, busca el item filtrado con
  `tray::find_menu`, así el menu del SNI no se pierde al ocultarlo del tray).
  Tests: `tray::tray_ignore_tests`, `render::tray::tray_hit_tests`.
- **Menú de energía nativo, sin rofi**: el botón power abría
  `rofi-powermenu.sh` si existía y solo caía al popup si no. Ahora abre siempre
  el menú nativo (`MenuScreen::PowerMenu` en la superficie del popup: Suspender,
  Log Out, Reboot, Shut Down; cada click ejecuta y cierra). `power.rs` con
  `run_first()`: intenta cada comando en orden y el primero que spawnea gana —
  `systemctl`/`loginctl` primero (sin sudo por polkit) y `sudo -n …`/`zzz` como
  fallback de Void, porque `zzz` no existe en Arch y Suspender no hacía nada
  (menú de energía; ojo: no tiene ítem propio en AUDIT.md — los ids van `b1`–`b12`
  y **no existe un b9**, la referencia vieja a "AUDIT.md B9" de este archivo era
  falsa). Test: `power::power_tests::encadena_hasta_el_que_existe`
  (verifica el fallback con poll, porque `spawn` es fire-and-forget y el assert
  inmediato pierde la carrera).
- **Bordes del dock y de los menús independientes**: `corner_radius`/`border_width`
  son solo del dock (`render/mod.rs`); los menús (power, volumen, launcher,
  clipboard, OSD, notificaciones, panel de ajustes) usan `menu_corner_radius` /
  `menu_border_width` vía `menu_radius()`/`stroke_menu_border()`
  (`menu_render/mod.rs`). En Ajustes: Layout trae "Dock Roundness" y Appearance
  "Menu Roundness" + "Menu Border" (+ "Dock Border" ya existente). Así el dock
  puede ir más curvo y los menús más rectos. Ojo: "Menu Roundness" existía en `SettingId` pero faltaba en `APPEARANCE_SETTINGS`, así que no salía en el panel; ya está agregado (con test `menu_border_tests` que lo fija).
- **El dock ya no desaparece: se relanza solo** (AUDIT.md A5/A6). `main.rs` envuelve
  `blocking_dispatch` en `catch_unwind` y, ante error de protocolo Wayland o panic,
  llama a `reexec()`: respawnea el binario con el mismo argv y sale. El fd de
  Wayland es CLOEXEC, así que el compositor da de baja las superficies viejas y la
  instancia nueva mapea las suyas. El contador viaja en la variable de entorno
  `DOCKYRS_REEXEC` y `puede_relanzar()` corta a los 3 intentos, para no girar en un
  crash loop si el fallo es determinista (test `reexec_tests::el_crash_loop_se_corta`).
  El hijo hereda stdout/stderr: sigue logueando en el mismo archivo. **Límite
  conocido**: el guard cubre el dispatch (dibujo y punteros), NO los drenajes de IPC
  que corren después del `match`; ahí la defensa es arreglar el panic en la fuente.
- **Lecturas del sistema con timeout, también el volumen** (AUDIT.md A1):
  `read_volume()` llamaba `wpctl get-volume` con `Command::output()` crudo — y corre
  en el **tick de 2 s**, así que un PipeWire colgado congelaba el dock para siempre
  y no revivía ni con Escape. Ahora pasa por `run_with_timeout` como el resto
  (test `widgets::percent_decode_tests::un_timeout_mata_al_hijo_colgado`).
- **Tres panics alcanzables cerrados** (AUDIT.md A5): `percent_decode` decodifica
  sobre **bytes** en vez de cortar `&s[i+1..i+3]` — el chequeo de índices sólo
  acotaba bytes, así que un `artUrl` como `%€` partía el `€` de 3 bytes en medio de
  un char y mataba el dock; el buffer oculto de `draw_hidden` ya no paniquea si
  falla `create_buffer` (corre en CADA ocultado; ahora deja el buffer viejo puesto,
  que es el estado seguro de la trampa 2); y el click de un app fija usa
  `icons.get()` en vez de `icons[idx]`. Tests: `widgets::percent_decode_tests`.

## Pendientes conocidos

- Panel de volumen: el selector de salida lista **4 dispositivos** como máximo y no
  tiene scroll (`VOLUME_MAX_DEVICES`); con más sinks habría que agregarlo. Los
  streams se releen cada 2s (tick de sistema), así que una app que empieza a
  sonar aparece en el panel siguiente, no al instante.

- El tray ignora wifi y bluetooth por heurística de nombre (`nm-*`,
  `blueman`, `bluetooth`): si el widget Network/Bluetooth está deshabilitado, esos
  items quedan fuera del dock por completo (no hay configuración para volver a
  mostrarlos).

- El panel debajo del dock está implementado sólo para el borde superior; con
  Left/Right/Bottom el panel sigue tapando el dock.
- Si un ajuste cambia el grosor del dock, el panel lo acompaña pero el alto de la
  superficie quedó fijado al abrirlo: el fondo del panel puede quedar cortado.
- Soltar una tarjeta en la franja del título (arriba de las columnas) la saca del
  dock: es intencional, pero la franja mide 16 px.
- `app_search.rs`, `clipboard_ui.rs` y `wallpaper_picker.rs` tenían el mismo autohide
  obsoleto que el panel (con `pointer_pos` viejo el dock no se ocultaba nunca):
  **arreglado** en `draw_ex`, que detecta el cierre del último modo que usa la
  superficie (`mode_was_open`) y refresca el estado. Un modo nuevo queda cubierto
  con sólo sumarlo a `autohide_force_visible()`.
- Campo `closing` en `DockMenuMode`: sólo se inicializa, nadie lo pone en `true`
  (código muerto). En `WsFlashMode` sí se usa (`close_ws_flash_mode` lo pone en `true`).
- `LEAVE_HIDE_MS` y `WS_FLASH_TIMEOUT_MS` son constantes; candidatos a ajuste.
