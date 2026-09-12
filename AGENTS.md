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

1. **Una sola superficie layer para todo.** El dock y cada modo (panel de ajustes,
   OSD, HUD, popups) usan `self.layer`. Si un modo cambia el tamaño TIENE que
   anotarlo en `applied_size`/`applied_geom` (`apply_panel_size`) o restaurarlo con
   `relayout_dock` al cerrar: `sync_autohide_surfaces` reaplica anchor/margin pero
   **nunca** el tamaño. De acá salieron dos bugs de "dock descentrado".
2. **Nunca desmapear la superficie.** `attach(NULL)` hace que niri resetee el
   tamaño de la layer a 0 y el cliente muere por error de protocolo. Ocultar =
   buffer transparente (`draw_hidden`).
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
   `ps -o lstart= -p $(pgrep -x dockyrs)` contra el binario.
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

## Mapa del código

- `src/app/mod.rs` — `App`: campos de modo (`dock_menu_mode`, `ws_flash_mode`,
  `osd_mode`, `popup_mode`, …), geometría (`applied_size`, `applied_geom`), autohide
  (`last_ptr_event`, `ptr_left_at`, `autohide_armed`).
- `src/app/pointer.rs`, `handlers.rs` — despacho por superficie: cada modo
  intercepta antes de la rama general (`if self.x_mode.is_some() { …; return; }`).
- `src/app/draw.rs` — `draw_ex` elige el modo, `relayout_dock` fija el tamaño,
  `draw_icons` dibuja el dock normal.
- `src/app/dock_menu.rs`, `dock_menu_input.rs` — panel de ajustes: `panel_layout`
  (reparto superficie/dock/panel), `panel_local` (traducción de coordenadas),
  hit tests y el drag de widgets (`drop_widget_chip`).
- `src/app/ws_flash.rs` — HUD de workspaces.
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
- Requiere la app con `RUST_LOG=debug` y que el dock pueda ocultarse: si el panel
  quedó abierto no hay transición oculto→visible y el sensor no dispara.

Líneas de log útiles: `dock: click derecho (x,y) -> Some(Kind)`,
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
  `handlers.rs`; si no, el auto-repeat de la flecha cicla un modo por frame). Los tres
  primeros reusan sus modos existentes y se cierran por su propio camino; "ventanas"
  es la misma lista del launcher con `SearchList::Windows`, y Enter enfoca por id de
  niri en vez de lanzar.
- Los cierres del launcher y del portapapeles también son **inmediatos** (mismo
  criterio que el panel de ajustes y el selector de fondos): así un cambio de modo no
  cruza la animación de salida con la de entrada.
- `scripts/pointer.py`.
- Autostart en niri: `spawn-at-startup` en `~/.config/niri/config/5_autostart.kdl`.
  Con una sola salida, sin `--output` ni `--profile`. No arrancar
  `dockyrs-notifyd` si ya hay otro daemon de notificaciones (mako, dunst): pelean
  por `org.freedesktop.Notifications`.

## Pendientes conocidos

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
- Campos `closing` en `WsFlashMode`/`DockMenuMode`: sólo se inicializan, nadie los
  pone en `true` (código muerto).
- **Barra de pestañas compartida** (`OVERLAY_TABS_H` + `draw_overlay_tabs()`): hoy no
  hay ninguna señal de en qué mini-app estás salvo la forma del panel. El plan ya está
  resuelto y el punto clave es que cada layout tiene **una** fuente compartida entre el
  dibujo y el hit test, así que la banda se mueve en sincronía sola:
  - launcher/ventanas: 4 puntos en `menu/app_search.rs` — `build_app_search_controls`
    (el `y` y el `fixed_h`), `app_search_strip_y()` y `app_search_vertical_along()`.
  - portapapeles: `CLIP_HEADER_H` en `menu_render/clipboard.rs` y `clip_panel_h()`,
    cuidando que la caja de búsqueda no crezca (`box_h` tiene que restar la banda).
  - fondos: `wallpaper_thumb_size()` resta la banda y se ajusta el `cross_pos` del
    hit test; en `menu_render/wallpaper.rs` el `ty`/`tx` de las miniaturas suma lo
    mismo (hay rama horizontal y vertical).
  Más: el alto del panel en cada `open_*` y la llamada a `draw_overlay_tabs` en cada
  `draw_*` con el índice del modo (`OVERLAY_ORDER` y `current_overlay()` ya existen).
  **Verificar con una captura por modo**: es la trampa 4 (las coordenadas del puntero
  son de la superficie) y ya costó dos bugs de controles muertos.
- `LEAVE_HIDE_MS` y `WS_FLASH_TIMEOUT_MS` son constantes; candidatos a ajuste.
