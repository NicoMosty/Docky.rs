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
   **nunca** el tamaño. De acá salieron tres bugs de "dock descentrado".
   El tercero: cambiar un ajuste de layout ("Dock Width", "Dock Size") **con el
   panel abierto**. `relayout_dock` se salteaba por estar la superficie prestada,
   así que el buffer crecía pero niri seguía centrando la superficie con el ancho
   viejo: el dock derivaba a la derecha (medido: 970.5 de centro contra 960 de la
   pantalla, con `width_padding = 61`). Por eso `relayout_dock` ahora re-aplica el
   reparto del panel cuando `dock_menu_mode` está abierto, y `apply_panel_size` es
   idempotente (no re-setea anchor/size si no cambiaron: cada `set_*` dispara un
   configure que puede perder el foco del puntero). Guard:
   `app::dock_menu::panel_layout_tests`.
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

 1. **La geometría de un widget que la calculan el reparto y el dibujo va en UNA
   función** (`volume_content_len`, `VOLUME_ICON_*`), no duplicada: el ancho que
   reserva `widget_natural_len` y el content que centra `draw_icon_label` tienen
   que coincidir o el contenido se sale de la pastilla (le pasó al altavoz: medía
   ~10 unidades contra las 6 reservadas porque su `s` interno no estaba atado a
   `VOLUME_ICON_R`). Mismo criterio que `hit_scale` (trampa 10).

 2. **La tabla de widgets: la exhaustividad del compilador pasó a ser un test.**
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

 1. **El workspace: `cargo test` en la raíz NO testea el crate.**
   Con `crates/dockyrs-canvas` como miembro, `cargo test --release` en la raíz corre
   sólo el paquete raíz, así que los guards del crate quedan invisibles para el
   comando que documenta este archivo. Lo arregla
   `default-members = [".", "crates/dockyrs-canvas"]` en el `Cargo.toml` raíz. Dos
   más del mismo commit: **NO** poner `panic = "abort"` en `[profile.release]`
   (rompe `catch_unwind`, que es la red de seguridad del reexec A5/A6; `strip = true`
   sí es seguro y baja 2,8 MB medidos), y el `*` de `[profile.dev.package]` **no
   alcanza a los miembros del workspace**, así que el crate del raster se compila en
   `opt-level = 1`.

- **Un canal NO despierta el loop de Wayland** (trampa 15). El selector de fondos abría con
   las tres miniaturas en negro y sólo aparecían al scrollear. No era el caché ni
   las rutas: el hilo que decodifica devuelve por `thumb_result_rx` y el loop de
   frames (`tick_wallpaper_frame`) se apagaba al terminar la animación de
   apertura, así que los resultados se quedaban en la cola hasta que cualquier
   OTRO evento (mover el scroll, un click) forzara un redraw. Como el caché se
   limpia al cerrar, pasaba en cada apertura. Arreglo: `thumbs_pending` (pedidos
   sin respuesta, se incrementa si el `send` da `Ok` y baja por cada resultado
   drenado) y `wallpaper_needs_frames()` mantiene el loop vivo mientras no llegue a
   0 — se apaga solo, medido: 1 tick de CPU en 3 s con el panel abierto y quieto.
   Guard: `app::wallpaper_picker::frame_tests`. **Cualquier resultado asíncrono
   nuevo** (hilo + canal, o un `Instant` que vence) tiene el mismo problema: si el
   loop no está animando, nadie lo despierta; hay que sumarlo a la condición del
   tick o pasar por el canal de `calloop`.

- **Un click afuera de un panel no le llega a la app** (trampa 16). El launcher y los
   paneles viven en la superficie del dock, que mide **sólo el panel** (640x236,
   640x460, …): un click fuera de ese rectángulo lo entrega el compositor a la
   ventana de abajo y la app no se entera. Por eso sólo se cerraban con Escape (el
   teclado sí lo tiene la superficie), con click derecho o clickeando un widget del
   dock. **`wlr-layer-shell` no tiene pointer grab** —sus requests son `set_size`,
   `set_anchor`, `set_exclusive_zone`, `set_margin`, `set_keyboard_interactivity` y
   `get_popup`— así que no hay forma de pedirle al compositor "avisame del click
   afuera"; niri no lo ofrece (mismo problema que su issue #1810 con menús de
   layer-shell). La solución es tener una superficie propia debajo del click:
   `app/click_catcher.rs` (mismo truco que el selector de región de screenshots,
   que ya era una superficie full-screen con input region). Arranca **vacía** y
   recién con el configure se sabe cuánto mide la salida para calcular el agujero.

## Mapa del código

- `src/render/widget_spec.rs` — la tabla `WIDGETS`: la única lista de widgets del
  render, con la medida y el dibujo de cada uno (trampa 13).
- `crates/dockyrs-canvas/` — raster por CPU sobre tiny-skia: texto (`text.rs`),
  iconos (`icon_cache.rs`) y miniaturas (`thumbnail_cache.rs`). Sin GPU ni contexto
  GL: el costo de rasterizar por CPU sólo se paga cuando hay algo que redibujar,
  que es por qué el reposo está en el piso (1 tick de sistema cada 5 s).

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
- `src/app/volume_panel.rs` — panel de volumen: se abre en la superficie del popup
  y aplica los cambios (arrastre de la barra, mute por fila, cambio de salida).
- `src/menu/volume_panel.rs` — geometría y hit tests del panel de volumen.
  `src/menu_render/volume_panel.rs` — su dibujo.
- `src/app/calendar.rs`, `src/menu/calendar.rs`, `src/menu_render/calendar.rs` —
  calendario del reloj: hover y cambio de mes / aritmética de fechas y geometría /
  dibujo.
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
- `--key up|down|left|right|enter|escape|backspace [--times N]` teclea esa tecla
  N veces (taps sueltos, sin auto-repeat) por el **teclado** virtual: es lo que
  ejercita la navegación de los menús. No necesita el reveal (no hay click): sirve
  con el panel de ajustes ya abierto por IPC. Imprime la última línea `teclado:`
  del log, que es la que dice qué quedó resaltado o si ESC cerró. Necesita un modo
  abierto (superficie con el teclado en `Exclusive`).

  ```sh
  dockyrs --toggle-dock-menu                     # abre el panel
  sudo python3 scripts/pointer.py --key down --times 3
  sudo python3 scripts/pointer.py --key escape   # cierra el panel
  niri msg --json layers | grep dockyrs          # interactividad de vuelta en None
  ```

  ```sh
  dockyrs --toggle-dock-menu                     # abre el panel
  sudo python3 scripts/pointer.py --key down --times 3
  sudo python3 scripts/pointer.py --key escape   # cierra el panel
  niri msg --json layers | grep dockyrs          # interactividad de vuelta en None
  ```

- `--hover` deja el puntero donde llegó y espera en vez de clickear: es lo que
  ejercita lo que se abre con hover, hoy el calendario del reloj. Imprime la última
  línea `calendario:` del log (o el aviso de que no se abrió). Para saber dónde caer
  primero se busca el widget con un click normal (`--button L`, que imprime el hit) y
  después se repite el mismo `--extra` con `--hover`. El reloj está al final del
  clúster derecho:

  ```sh
  sudo python3 scripts/pointer.py --extra 600 --button L   # -> Some(Clock)
  sudo python3 scripts/pointer.py --extra 600 --hover      # calendario: abre center=…
  sudo python3 scripts/pointer.py --key right --times 2    # → mes siguiente (x2)
  sudo python3 scripts/pointer.py --key escape             # cierra el panel
  ```

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
`wsflash: show … visible= placed= borrowed=`, `popup: t=… frame=Xms`,
`calendario: abre center=…` / `calendario: <mes> (+1)` (apertura por hover y cambio de
mes, que es el sensor de `pointer.py --hover`),
`teclado: panel|popup|menú de ícono <target viejo> -> <target nuevo>` (navegación de
los menús con las flechas; es el sensor de `pointer.py --key`) y
`teclado: Escape -> cierra …` (lo que cierra cada ESC).

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
- **Dock Width con el panel abierto ya no descentra el dock**: cambiar cualquier
  ajuste de layout mientras el panel de ajustes está abierto re-aplica el reparto de
  la superficie compartida (`relayout_dock` → `apply_panel_size`, idempotente), así
  que el dock queda centrado en cada paso del slider en vez de derivar a la derecha.
  Verificado contra píxeles: centro del dock 959.5 con `width_padding = 61` (antes
  970.5, con la superficie anclada en el ancho viejo).
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

- **Workspace vacío ⇒ dock fijo**: `WorkspaceInfo.empty` (niri:
  `active_window_id == null`; hypr: `windows == 0`) y `render::active_is_empty()`.
  Con el workspace activo vacío `should_hide()` no oculta y el dock se revela si
  estaba oculto; al dejar de estar vacío vuelve al autohide normal. El watcher de
  niri ahora engancha además eventos `Window*` para reevaluarlo al abrir/cerrar.
  Test: `render::workspaces::workspace_hit_tests::solo_el_activo_vacio_cuenta`.
  Dos cosas que costaron rondas y hay que respetar si se toca:
  - **La regla se resuelve en `refresh_workspaces`, NO en `draw_ex`**
    (`reveal_dock_if_empty` + ocultado inmediato al dejar de estar vacío).
    `request_redraw` se saltea si hay un frame pendiente, y `show_ws_flash` decide
    con `dock_visible` en el mismo bloque: dejarlo al dibujo hacía que el HUD
    apareciera y el dock se revelara tarde (o al revés, que al pasar vacío →
    ocupado no apareciera el selector y quedara el dock completo).
  - **El HUD comparte la superficie y `show_ws_flash` sale si ve `dock_visible`**,
    así que no cierra el HUD ya abierto: al revelar hay que pasar por
    `close_ws_flash_for_dock()` (devuelve layer y tamaño), que también usa
    `reveal_dock`.

- **Teclado en los menús** (flechas para elegir, Enter para activar, ESC para
  cerrar) en los cuatro menús que no lo tenían: panel de ajustes, dropdown de
  esquema, menús del popup (tray, energía y **panel de volumen**) y el menú de un
  ícono del dock. La pieza que lo hace chico es `menu/keynav.rs`:
  `panel_targets()` enumera lo seleccionable de un panel **en el orden visual** y
  `nav_step()` mueve el resaltado; como el resaltado es el MISMO `HitTarget` /
  `hovered` que dibuja el hover del mouse, no hay un segundo sistema de foco que
  se pueda desincronizar del dibujo (mismo criterio que `hit_scale`, trampa 10).
  Enter reusa el click de cada menú (`handle_dock_menu_click`, `handle_popup_click`,
  `volume_panel_hit` —extraído de `volume_panel_press` para que el mute y el cambio
  de salida vivan en un solo lugar— y `handle_menu_click`).
  - Panel de ajustes: ↑↓ recorren las pestañas y después los controles de la
    pestaña activa, Enter activa/togglea/cambia de pestaña y ←→ mueven el slider
    resaltado (un paso por evento, igual que el botón +/-). Dentro de un
    desplegable las flechas navegan **la lista** y ESC cierra sólo el desplegable;
    el segundo ESC cierra el panel.
  - Panel de volumen: ←→ son ±5% de la fila resaltada (con el mismo throttle de
    `VOLUME_APPLY_MS` que el arrastre, pero forzando el paso: acá no hay "soltar"
    que aplique el valor final) y Enter mutea. `volume_x_from_pct()` es la inversa
    de `volume_pct_from_x()` y vive al lado, para que no se despeguen.
  - **El foco del teclado de esos menús lo tiene su propia superficie**
    (`Exclusive` en `open_menu` y `create_popup_surface`) y el compositor lo suelta
    cuando se desmapea. Con `OnDemand` no llegaba ninguna tecla (por eso el panel de
    ajustes, que comparte la superficie del dock, pasa por `enforce_keyboard` y
    estos no). Test: `niri msg --json layers` → el `dockyrs-menu` queda
    `exclusive` mientras el menú está abierto y desaparece al cerrarse.
  - Guardo: `menu::keynav_tests` (orden/extremos/deshabilitados del tray),
    `menu::volume_panel::volume_panel_tests::el_x_de_un_porcentaje_da_la_vuelta_completa`.
    A mano: `scripts/pointer.py --key up|down|enter|escape` (nuevo; necesita sudo
    por `/dev/uinput`) y las líneas `teclado:` del log.

- **Calendario del reloj por hover**: con el puntero encima del widget `Clock` (el
  clúster derecho de la config de fábrica) se abre el mes actual debajo, igual que el
  panel de volumen, en la superficie del popup. ←/→ cambian de mes con vuelta de año,
  ESC cierra, y no hay nada clickeable. Piezas: `menu/calendar.rs` (mes y aritmética
  de fechas con `libc` — `mktime` normaliza el día 1 y de paso da `tm_wday`, meses con
  la regla de bisiesto: **sin traer chrono** —, geometría del panel y tests),
  `menu_render/calendar.rs` (título + 6 semanas, hoy con pastilla de acento) y
  `app/calendar.rs` (hover, apertura y cambio de mes). Dos cosas que hay que respetar
  si se toca:
  - **El mes viaja dentro del `Control`** (`ControlKind::Calendar(CalendarMonth)`), no
    en un campo de `DockPopupMode`: así el mes que se dibuja y el que cambian las
    flechas son el mismo dato (mismo criterio que `hit_scale`, trampa 10). El alto es
    fijo (siempre 6 semanas) justamente para que cambiar de mes no redimensione la
    superficie ni su input region.
  - **El hover necesita plazo** (`CALENDAR_HOVER_MS = 450`): sin él el panel se abre
    con sólo cruzar el reloj de camino a otro widget. El instante se registra UNA vez
    por entrada al widget (`hover_step`, con test): resetearlo en cada `Motion` lo deja
    sin abrir nunca. El tick es el del autohide (`autohide_timeout`), único que corre
    con el popup abierto, y `arm_calendar_tick` lo agenda con envío directo al canal,
    así que también funciona con el autohide apagado.
  - Se cierra al salir del **reloj**, no del dock: `popup_should_dismiss` cambia de
    objetivo según `p.screen` (quedarse sobre otro widget de la barra ya no lo mantiene
    abierto) y reusa `ptr_left_at`, el mismo reloj de salida del autohide.
  - **El popup toma el teclado en `Exclusive`**, que es lo que hace que las flechas
    lleguen; el precio es que mientras el calendario está abierto el foco del teclado
    es del dock (escribir en la ventana de abajo no anda hasta mover el puntero o
    apretar ESC). Es el costo de tener flechas en un panel que se abre por hover: si
    molesta, la salida es `OnDemand`, que pide un click antes de las flechas.
  - Guardo: `menu::calendar::calendar_tests` (vuelta de año, bisiestos, `tm_wday`
    contra `date +%u`, alto fijo) y `app::calendar::calendar_hover_tests` (el plazo no
    se resetea con cada Motion). A mano: `scripts/pointer.py --hover` (nuevo) más las
    líneas `calendario:` del log.

- **Paneles del overlay homogéneos en proporciones y estilos.** Los tres paneles
  del ciclo (launcher/ventanas, portapapeles, fondos) no compartían ni el inset
  lateral ni el estilo de la caja de búsqueda: el strip de tarjetas del launcher
  arrancaba **pegado al borde** del panel (inset 0, medido: x=559, el mismo x que
  el borde) mientras la caja de búsqueda iba inset 10, la fila del portapapeles
  inset 4, el filmstrip 6, y la separación banda→contenido era 10/5/6. Y la caja
  de búsqueda estaba dibujada **dos veces**: el launcher centraba el texto y el
  portapapeles lo alineaba a la izquierda (radios 6/6, altos 26/32).
  - Ahora el inset lateral y la separación son `MENU_PADDING` en los tres, la caja
    de búsqueda se dibuja **una sola vez** (`menu_render::draw_search_field`:
    lupa + texto a la izquierda, misma altura) y la usan el launcher, el
    portapapeles y el panel de ajustes.
  - El radio de todo lo interno es `menu::OVERLAY_RADIUS = 8` (antes 6 caja / 8
    tarjeta / 7 fila / 11 miniatura / 6 pestaña). El de la miniatura de fondo
    salía de tres lugares distintos (11 horneado en el pixmap, 11 en el anillo
    del hover, 5 en el placeholder): si no coinciden, el anillo no calza con la
    esquina de la imagen, así que ahora los tres leen la constante.
  - El strip del launcher y su hit test se movieron juntos (`app_search_viewport_along`
    descuenta los dos insets y `app_search_strip_hit_test` mide desde el inset):
    guard `menu::app_search::strip_tests`. Verificado contra píxeles: caja y
    pastilla arrancan en el mismo x (566 en el launcher, 750 en el portapapeles).
  - Costo: el panel del launcher crece 6 px de alto (la separación caja→tarjetas
    pasó de `ROW_GAP` a `MENU_PADDING`) y el del portapapeles 4 (encabezado =
    10+26+10, antes 42). Los anchos **no** se tocaron.

- **Ancho único de los tres paneles + launcher en grilla de 2 filas.** Los paddings
  iguales no alcanzaban: el salto de ancho era lo que se veía. Medido en el log
  (`base=(629,26)`): el launcher medía **809** (`dock + 180`), el portapapeles
  **440** y los fondos **640**, así que ciclar con Shift+←/→ cambiaba la forma del
  panel. Ahora los tres miden `menu::OVERLAY_PANEL_W = 640` (verificado:
  `configure new_size=(640, 236)` / `(640, 460)` / `(640, 196)`, y el dock vuelve a
  629×26 al cerrar). Como los tres se anclan igual (`edge_anchor_margin`), con el
  mismo ancho los bordes no se mueven al cambiar de pestaña.
  - Sale del dock a propósito (antes el launcher crecía con el dock): el valor no
    cambia porque el usuario agregue un widget. `APP_SEARCH_WIDTH_GROWTH` y
    `CLIP_PANEL_W` se borraron; el vertical (Left/Right) sigue con su cross de
    siempre, porque ahí el "ancho" es el otro eje.
  - El launcher pasó de una tira a una **grilla row-major de 2 filas x 6** (12
    apps contra 6) que scrollea de a páginas: el ranking se lee de izquierda a
    derecha y el #1 queda arriba a la izquierda. `APP_CARD_ROW_H` ahora sale de
    `APP_CARD_PAD + ICON + LABEL_GAP + LABEL_H + PAD` (72, antes 78 fijo con 6px
    arriba y 18 abajo: en dos filas el ritmo vertical no cerraba) y el dibujo usa
    esas mismas constantes.
  - **Una sola función reparte la grilla** (`app_search_card_offset`) y el hit test
    la recorre en vez de repetir la cuenta: no puede desincronizarse del dibujo
    (trampa 10). El resaltado pasó a ser 2D (`highlight_x/y`, los dos animados en
    `tick_app_search_frame`): con una sola coordenada la pastilla se dibujaba
    siempre en la fila de arriba.
  - Flechas: ←/→ = ±1 (orden de lectura) y ↑/↓ = ±6 (la otra fila de la grilla),
    con `app_search_row_step()` como única definición y saturación en los extremos.
  - Guards: `menu::app_search::strip_tests` (640 da 6x2, la 7ª cae debajo de la 1ª,
    la 13ª abre la página siguiente, el tope del scroll muestra la última página, y
    el centro de cada tarjeta visible da su índice incluidas las de la 2ª página).
  - Verificado a mano: las tres pantallas contra píxeles, y abrir/cerrar cada modo
    con `niri msg --json layers` (abierto ⇒ `Exclusive`, cerrado ⇒ `None`, el dock
    vuelve a 629×26, 0 panics).
  - **Lo que queda distinto es el alto** (236 / 460 / 196): el ancho era lo que se
    percibía al ciclar, pero si molesta el siguiente paso es un alto común.
  - Costo del ancho común: las filas de texto del portapapeles quedan con mucho
    espacio a la derecha (el título y el subtítulo son cortos). Se podría repartir
    el contenido de la fila (p. ej. los caracteres a la derecha) si se quiere
    aprovechar ese aire.

- **Las miniaturas del selector de fondos ya no tardan hasta un scroll en aparecer**
  (trampa 15). Se abría con las miniaturas en negro y aparecían recién al mover el
  filmstrip, porque el hilo que decodifica devuelve por un canal y nada despertaba
  el loop de frames cuando terminaba. `thumbs_pending` + `wallpaper_needs_frames()`
  (guard: `app::wallpaper_picker::frame_tests`). Medido: el panel abre con las tres
  miniaturas dibujadas y, ya cargadas, el loop se apaga (1 tick de CPU en 3 s).

- **La batería y el volumen tardaban en aparecer al tocar el set de widgets** (dos
  causas, ninguna era el dibujado).
  - **La batería se sondeaba cada ~30 s.** `spawn_battery_watcher` usa
    `poll(POLLPRI)` sobre los sysfs, que es lo correcto, pero **medido en esta
    laptop el poll no recibe nunca un evento**: `poll(POLLPRI)` sobre
    `capacity`/`status`/`energy_now` vuelve a los 30 032 ms con 0 eventos, así que
    el único reloj era el timeout. `refresh_battery` se llama **sólo** desde ese
    `BatteryChanged`, y `draw_battery_widget` no dibuja NADA mientras
    `widgets.battery` sea `None` (es un hueco, no un ícono vacío): al arrancar y
    al colocar el widget, hasta ~30 s sin batería. Timeout 30 s → **3 s** (leer
    cuesta 3 ms medidos; en las máquinas donde el sysfs sí notifica sigue siendo
    instantáneo).
  - **Cambiar el set de widgets no leía los datos.** `drop_widget_chip` (el único
    lugar donde cambia `settings.widgets`) hacía sólo `sync_widget_bar_len` +
    `request_redraw`, así que un widget recién colocado esperaba a su propio reloj:
    2 s el volumen/red/kblayout (tick de sistema) y ~30 s la batería. Ahora llama a
    `refresh_sys` + `refresh_battery` + `refresh_bluetooth` + `refresh_media` (los
    cuatro ya salen solos si el widget no está colocado).
  - Cadencia de cada dato, para tenerlo a mano: volumen/red/kblayout **2 s**
    (`spawn_sys_ticker`, `wpctl` ~19 ms por lectura), batería **3 s** (sysfs 3 ms +
    `poll`), media/bluetooth **evento** (DBus).
  - Verificado: a los 5,2 s del arranque la barra ya muestra la batería (5 % en
    rojo, descargando) y el volumen (100); antes la batería era un hueco hasta los
    ~30 s. Y el watcher a 3 s no se nota: 1 tick de CPU en 5 s con el dock oculto.

- **El volumen sigue los cambios al instante (40 ms medidos, antes hasta 2 s).** Las
  teclas de volumen del sistema corren `wpctl` **por fuera del dock**
  (`~/.config/niri/config/binds/media.kdl`), así que el único reloj era el sondeo
  del tick (2 s): el número llegaba tarde y salteándose los pasos intermedios de
  una ráfaga. PipeWire sí avisa, así que hay un watcher nuevo
  (`ipc::spawn_volume_watcher`, mismo patrón que el de media con `playerctl
  --follow`) que corre `pactl subscribe` → `IpcMessage::VolumeChanged` →
  `App::refresh_volume`.
  - Filtro: sólo `on sink #` y `on server`. **`on sink-input #` comparte el prefijo**
    y son los streams de las apps: eso lo relee el panel de volumen cada 2 s, porque
    hacerlo en cada evento lo volvería lento al arrastrar (el arrastre aplica cada
    50 ms).
  - `refresh_volume` relee **sólo** el volumen: `refresh_sys` de paso corre `iw` y
    `niri msg -j keyboard-layouts` (~20 ms medidos por vuelta) que no tienen nada
    que ver, y la rueda los pagaba **por muesca** (46 ms de hilo principal por
    muesca, medidos). La comparación del volumen quedó en
    `WidgetSnapshot::refresh_volume`, que `refresh_sys` reusa para no duplicarla.
  - Medido: **40 ms** de reacción (cambio de volumen y mute, contra hasta 2000 ms
    antes), 0 líneas espurias en 4 s quieto, **1 tick de CPU en 6 s** con el watcher
    escuchando, y ~1 lectura por evento en una ráfaga (150 ms de CPU hija por 10
    cambios). Sin lazo: nuestras lecturas generan eventos de *cliente*, que se
    filtran.

- **Cambio de pestaña del overlay: ahora se desliza, y antes no había transición.**
  Con el default `transparency = 1.0`, `anim_opacity` devuelve 1.0 siempre, así que el
  "fade" de apertura dibujaba **el mismo cuadro opaco ~15 veces** (medido: 110 ms de
  hilo principal por cambio, ~7 ms por frame) y lo que se veía era un corte seco: se
  cerraba al instante, la superficie cambiaba de tamaño y el panel nuevo aparecía ya
  opaco. Los otros paneles **nunca** se renderizan (sólo el activo), y los datos del
  nuevo cuestan poco (9-21 ms medidos: 142 `.desktop` parseados, un escaneo de
  carpeta).
  - El contenido entra corrido desde el lado hacia el que viajás: la dirección la
    sabe `cycle_overlay` (`start_overlay_slide`), y el progreso es el `anim` que cada
    modo **ya** tenía, así que no hay temporizador ni campo de animación nuevos. La
    fórmula es la misma del panel de ajustes (`menu::overlay_slide_offset`,
    `TAB_SLIDE_DISTANCE`), con guard `menu::slide_tests`.
  - El que se corre es el **cuerpo** (caja de búsqueda + grilla / filas / filmstrip);
    el fondo y la banda quedan fijos, si no el panel dejaría un hueco en el borde.
    Lo hace `menu_render::draw_body(dx, opacity, |dst| …)`, que dibuja el cuerpo en un
    pixmap aparte y lo pega traducido (guard `menu_render::body_tests`: el cuerpo cae
    corrido y el fondo no se toca). Los popups y el panel de ajustes lo llaman con
    `dx = 0`/`opacity = 1` y siguen con su animación propia, sin cambios.
  - Con el panel opaco la apertura ya no corre un fade inútil (`App::initial_panel_anim`
    la arranca terminada): abrir una pestaña por IPC es **1** dibujo. Medido: el
    portapapeles pasó de ~110 ms de CPU por apertura a ~15 ms (la primera de cada
    proceso suma ~120 ms porque carga el historial). El launcher y los fondos siguen
    animando por otros motivos: el `content_anim` de las tarjetas y la decodificación
    de miniaturas.
  - Efecto lateral para `transparency < 1.0`: el fondo del panel ya no se funde (queda
    fijo y lo que entra con la opacidad es el cuerpo). Con `1.0` — el default — no
    cambia nada.

- **Miniaturas de fondos: caché en memoria + disco (1370 ms → 20 ms por visita).**
  `close_wallpaper_mode` hacía `thumbnail_cache.clear()` y la próxima visita volvía a
  decodificar los originales del usuario (fotos de varios MB: medido **1370 ms de CPU
  y 79 MB de pico de RSS en CADA visita**, porque un 4K decodifica a ~33 MB antes de
  escalar). Ahora hay dos niveles:
  - **Memoria:** el `ThumbnailCache` ya no se limpia al cerrar, con tope de 64
    miniaturas (~9 MB: una de 240x150 son 144 KB) y desalojo del más viejo. El
    `trim_heap()` del cierre se mantiene: devuelve el arena de malloc, no las
    miniaturas.
  - **Disco:** `dockyrs_canvas::load_thumbnail_cached` guarda en
    `~/.cache/dockyrs/thumbs/<hash>.png` la miniatura ya escalada con las esquinas
    horneadas. La clave es `hash(ruta + mtime + tamaño + w + h + radio)`, así que
    editar un fondo la invalida sola; `prune()` poda a 512 archivos por mtime porque
    cada versión de un archivo tiene su propia clave. Sin carpeta de caché
    (parámetro `None`) decodifica y no escribe.
  - **Medido** (5 fondos): visita con disco frío 1220 ms y 79 MB de pico; visitas
    siguientes **20 ms**; proceso reiniciado con disco caliente **30 ms y 15 MB de
    pico** (−64 MB). El caché en disco ocupa **250 KB** para 5 fondos.
  - La carpeta de caché sale de `usage::cache_dir()` (una sola definición de
    `~/.cache/dockyrs`, que antes estaba inline en `usage.rs`).
  - Guards: `dockyrs_canvas::thumbnail_cache::thumbnail_tests` (leer del disco da los
    mismos píxeles que decodificar —incluido el alfa premultiplicado de las esquinas—,
    dos tamaños = dos miniaturas, sin disco no escribe, editar el original invalida, y
    el tope de memoria desaloja el más viejo).
  - Lo que queda: el launcher tiene su 1ª apertura de la sesión en ~420 ms (rasteriza
    12 íconos y ~480 glifos) y después 40-50 ms; el portapapeles ~120 ms la primera
    (carga el historial) y después 10-20 ms. Eso ya es caché en memoria y no se
    rehace por visita.

- **Los paneles del overlay ahora se cierran al clickear afuera** (trampa 16). No era
  lógica de cierre la que faltaba: el click no llegaba a la app (la superficie del
  dock mide sólo el panel) y `wlr-layer-shell` no tiene pointer grab, así que niri
  no puede avisar. `app/click_catcher.rs`: mientras hay un panel abierto (launcher,
  ventanas, portapapeles, fondos o ajustes) se mapea una superficie transparente a
  pantalla completa en `Layer::Top` con `keyboard_interactivity = None` (el panel
  conserva su `Exclusive`, o Escape dejaría de funcionar) y la input region = la
  pantalla **menos** el rectángulo del dock, armada con los 3-4 rectángulos que lo
  rodean (`catcher_region_rects`, pura y con guard: tapa la pantalla menos el
  agujero y sin pisarse). El agujero sale de la cuenta inversa del anclaje
  (`dock_screen_rect`, que deduce dónde dejó el compositor la superficie a partir de
  `DockEdge`/`DockAlign`/`pos_y`) y se recalcula en cada `draw_ex`, así que un cambio
  de pestaña (que cambia el alto) lo acomoda solo. Un click en el catcher llama a
  `dismiss_overlay`, que es lo mismo que hace Escape en cada modo.
  - Efecto buscado: mientras el panel está abierto el overlay es **modal** — el click
    no sigue viaje a la ventana de abajo (como rofi).
  - **Costo medido (no se nota).** En reposo: **0 superficies** de catcher y 13 MB de
    RSS. Con un panel abierto: **1** superficie más y los mismos números que antes de
    la feature (launcher 420 ms y 13→16 MB, portapapeles 100 ms y 30 MB, fondos 30 ms y
    27 MB), con 10-20 ms de CPU por cada 4 s quieto (el reposo de siempre). El buffer
    es un memfd del tamaño de la salida que **nunca se escribe** (los ceros ya son
    transparentes y las páginas dispersas no se materializan), así que el RSS no sube;
    al cerrar se desmapea (0 superficies).
  - Los popups (tray, volumen, calendario) no lo usan: ya se cierran al salir el
    puntero del panel (`popup_should_dismiss`).
  - RAM: el buffer es un memfd del tamaño de la salida pero **no se escribe** (los
    ceros del memfd ya son transparentes y las páginas dispersas no se materializan),
    así que medido no se nota: 13,4 MB con el dock en reposo, 16,8 MB con el launcher
    abierto (eso es el caché de íconos), y el catcher se desmapea al cerrar (0
    superficies en `niri msg --json layers`).
  - Verificado a nivel protocolo con `WAYLAND_DEBUG=1`: `set_anchor(15)`,
    `set_size(0,0)`, `set_keyboard_interactivity(0)`, configure de niri con
    `1920x1080`, los tres `wl_region.add` exactos (`0,236,1920,844` / `0,0,640,236` /
    `1280,0,640,236`) y el buffer `1920x1080` stride 7680; 0 errores de protocolo.
  - **Falta verificar el click de verdad**: inyectar puntero necesita `sudo`/uinput,
    que no está disponible. Los tests cubren la cuenta de la región y el protocolo
    confirma la superficie; lo que hay que probar a mano es que clickear una ventana
    de abajo cierre el panel y que clickear una tarjeta del launcher siga abriendo la
    app (el agujero).

- **El nombre de la 1ª tarjeta titilaba al mover el mouse.** No era un artefacto de
  dibujo: había **dos fuentes de verdad** para "qué tarjeta está resaltada". La pastilla
  sigue a `highlight` (que sólo cambia cuando el hit da una tarjeta), pero el color de
  la etiqueta seguía a `hovered`, que se borra al salir de las tarjetas —banda, caja de
  búsqueda, el hueco entre tarjetas o el panel—. Al borrarse, el fallback
  `hovered.or(Some(selected))` marcaba la SELECCIONADA (la 1ª): su etiqueta se
  encendía sola mientras la pastilla quedaba en otro lado. Justo lo que se veía.
  - El color sale de la **pastilla**: `menu::app_search_hot_card(count, panel_w,
    is_vertical, highlight)` devuelve la tarjeta más cercana al resaltado (durante la
    animación gana la más próxima, así el color cambia **una vez**, al pasar el punto
    medio). Guard: `menu::app_search::strip_tests::el_color_de_la_etiqueta_sigue_a_la_pastilla`
    (probado también en el panel vertical).
  - Pasar el mouse por una tarjeta **también la selecciona** (`selected = i`), como
    rofi: si no, Enter lanzaba la 1ª mientras la pastilla marcaba la que estabas
    mirando. Y al salir de las tarjetas no se toca nada, así que el resaltado se queda
    donde estaba en vez de saltar a la 1ª.
  - `AppSearchMode.hovered` quedó sin lectores y se borró, con su rama de `Leave` (que
    existía sólo para limpiarlo).

- **Ajuste nuevo: `smooth_transitions` (Appearance → "Smooth Transitions", toggle,
  default prendido).** Apaga las transiciones de los paneles del overlay **y** del panel
  de ajustes. Apagado no se anima nada adentro de los paneles: arrancan terminados (un
  solo dibujo en vez de ~9-15 frames), el cambio de pestaña no desliza ni funde, y el
  scroll / resaltado / fundido del strip saltan al valor final. El lerp del resaltado
  era el que más se pagaba: sigue al puntero, así que cada movimiento costaba ~8 frames.
  - **Medido** (3 aperturas en caliente del launcher): **9 → 2-3 frames** y **50-60 →
    20 ms** de CPU por apertura.
  - Piezas: `DockSettings::smooth_transitions` (con `#[serde(default)]` del struct, así
    los configs viejos no se rompen), `SettingId::SmoothTransitions` (label, `range`,
    `is_toggle`, `get`/`set`, `display_value` vacío —como los otros toggles— y **fuera**
    de `affects_layout`, que no cambia ninguna medida) y la entrada en
    `APPEARANCE_SETTINGS` (el array es de tamaño fijo: 12 → 13).
  - El gating: `App::initial_panel_anim` (devuelve 1.0 = terminado),
    `App::initial_content_anim`, `App::start_overlay_slide` (no hace nada), el cambio de
    categoría del panel de ajustes (`slide_dir` 0 / `slide_anim` 1) y los tres ticks
    (`app_search`, `clipboard`, `wallpaper`) donde los lerps pasan a saltar.
  - Guard: `menu::settings::menu_border_tests::las_transiciones_estan_en_appearance_y_son_toggle`
    (está en la lista, es toggle, no pide relayout, y `build_category_controls(Appearance)`
    la arma como `ControlKind::Toggle`).
  - **Ojo al medir**: en el `config.json` los ajustes viven bajo la clave `"settings"`;
    un `smooth_transitions` en la raíz se ignora en silencio y el A/B da idéntico. Y para
    contar frames de verdad hay que contar los `attach` de la superficie del dock con
    `WAYLAND_DEBUG=1`: la CPU sola no distingue, porque la apertura del launcher está
    dominada por el re-parseo de los ~140 `.desktop`.

- **Los procesos hijos eran el costo de RAM escondido** (medido 2026-09-15). Antes de
  tocar código se midió el stack completo y apareció lo que no se ve mirando sólo
  `dockyrs`: **3 hijos residentes, 27,6 MB** — `niri msg --json event-stream` (15 MB),
  `playerctl -a metadata --follow` (7 MB) y `pactl subscribe` (5,6 MB) — contra 31,5 MB
  del propio dock. El CPU en reposo ya estaba en el piso (0,20-0,33% de un núcleo,
  medido en 12 s), así que **no había regresión que buscar en el dibujo**: los
  últimos 8 commits no agregaron trabajo periódico (verificado con el diff: sólo el
  hover del calendario y el widget `Custom`, que corre un `sh` cada ≥2 s y sólo si
  hay uno colocado).
  - **El watcher de niri ya no lanza el binario `niri`**: `spawn_niri_workspace_watcher`
    habla `$NIRI_SOCKET` directo (`UnixStream` + `"EventStream"\n`, respuestas JSON
    por línea; niri 26.04 no pide handshake, verificado). Mismo filtro por substring
    (`niri_evento_relevante`, con test). Si no hay `NIRI_SOCKET` o falla el connect,
    cae al CLI de siempre, así que el peor caso es el de antes. Medido: −15 MB.
  - **El watcher de media sólo corre con un widget `Media` colocado**
    (`App::publish_watcher_wants` + el flag `media_wanted` que lee
    `spawn_media_watcher`; se publica donde cambian los widgets, hoy sólo
    `drop_widget_chip`). Medido: −7 MB en la config de fábrica, que no tiene `Media`.
  - Queda `pactl subscribe` (5,6 MB), que sí corresponde al widget `Volume`.
  - `DockSettings::has_widget` es la única definición de "el widget está colocado"
    (la usan el reparto, los `refresh_*` y el gate del watcher).
  - Resultado: el stack pasó de **~59 MB a ~30 MB** (hijos 27,6 → 5,6 MB) con el
    workspace andando por socket y 0 errores. Guards:
    `ipc::niri_event_tests::el_filtro_del_event_stream`,
    `config::has_widget_tests::has_widget_mira_todos_los_slots`.
  - **El layout de teclado dejó de sondearse**: niri lo manda por el MISMO
    event-stream (`KeyboardLayoutSwitched` al cambiar, `KeyboardLayoutsChanged` en el
    estado inicial), así que `ipc::niri_mensaje` mapea la línea a
    `IpcMessage::KbdLayoutChanged` → `App::refresh_kblayout`, y `read_kblayout` salió
    de `WidgetSnapshot::refresh_sys`. Medido: el tick bajó de **~33 ms a 20 ms** por
    vuelta (el `niri msg -j keyboard-layouts` costaba 13,8 ms standalone) y el widget
    se actualiza al instante en vez de hasta 2 s después. Verificado en vivo: cada
    `niri msg action switch-layout N` dispara el read en ~1 ms.
    - **Ojo al probarlo**: `switch-layout` **exige el argumento** (`<LAYOUT>`); sin él
      niri sale con rc=2 y no cambia nada, lo que hace parecer que el evento nunca
      llega. Cómo medir el costo del tick: `cutime+cstime` de `/proc/<pid>/stat`
      (campos 16-17) es el CPU de TODOS los hijos cosechados, o sea exactamente el
      costo de los spawns periódicos; el poll con `ps` es demasiado grueso para
      procesos de 13 ms.
    - Hyprland no se toca: `read_kblayout` **no tiene rama de Hyprland** (devuelve
      `--` siempre), así que sacarlo del tick no puede romper nada ahí.
  - **Ojo con la atribución**: el RSS del propio `dockyrs` varía 13→25 MB por los
    cachés (acotados, no fuga) y por el pool shm de sctk (2-7 MB según el uso, es el
    `memfd:smithay-client-toolkit (deleted)` de `smaps`); no es mérito NI culpa de
    este cambio. Lo que sí es causal es la baja de los hijos.

- **Los paneles del overlay van debajo del dock** (pedido: el launcher se abría en
  el mismo rect que el dock y se veía que se solapaba). Como el panel de ajustes ya
  lo hacía, los tres del overlay componen la misma superficie `[dock][gap][panel]`
  con `app/dock_menu.rs:panel_layout`, así el dock queda a la vista. Los cuatro
  comparten ahora: `App::overlay_panel_size()` (el tamaño del panel del modo
  abierto), `App::overlay_layout()` (el reparto), `App::show_panel_surface()` (dock
  arriba + `panel` bliteado en su offset, un solo `attach`), `App::restore_dock_size()`
  (vuelve a `base` y lo anota en `applied_size`), `App::panel_local()` (superficie →
  panel; vale para los cuatro) y `draw_dock_into()` (dibuja el dock en un destino).
  - El agujero del click-catcher sale de `overlay_layout` (el tamaño de la
    SUPERFICIE entera, no del panel): si no, el catcher se comía los clicks de la
    franja del dock y del borde de abajo. Los `open_*` del overlay llaman a
    `apply_panel_size` y los `close_*`/tick-close a `restore_dock_size`.
  - Medido: launcher 640x270 (26+8+236), portapapeles 640x494, fondos 640x230;
    screenshots que muestran el dock arriba y el panel debajo. Guards:
    `panel_layout_tests::con_el_dock_arriba_el_panel_va_debajo` y
    `catcher_tests::el_agujero_cubre_el_dock_y_el_panel`.
  - **Ojo al probarlo con `pointer.py`**: el script parkea abajo-izquierda y barre
    la franja de ABAJO, así que con el dock en `Top` nunca ve el reveal. En su lugar,
    probar el hover con /tmp/hover_test.py (parkea por clamping en (0,0) y mueve con
    los mismos tiempos que pointer.py: ~3.21 px por paso de 3 en x, ~2.04 px por paso
    de 2 en y) y confirmar con screenshots + las píldoras / anillos de hover. Verificado
    así: la pastilla del launcher cae en la fila 2 bajo el puntero (el offset del panel
    está bien aplicado) y el anillo de fondos en la 2ª miniatura.

## Pendientes conocidos

- Paneles del overlay: el **ancho** ya es el mismo en los tres (`OVERLAY_PANEL_W`),
  pero el alto no (el launcher 236, el portapapeles 460, los fondos 196). En el
  panel vertical (dock Left/Right) los tres siguen con su cross de siempre: el
  portapapeles es ancho por naturaleza y el launcher/fondos van pegados al dock.

- Calendario del reloj: no se pasa de mes con el mouse (no tiene ‹ › clickeables,
  sólo ←/→) y no selecciona días ni navega semanas: muestra el mes y marca hoy. El
  popup va en `Exclusive` mientras está abierto (ver la nota del teclado abajo).

- Panel de volumen: el selector de salida lista **4 dispositivos** como máximo y no
  tiene scroll (`VOLUME_MAX_DEVICES`); con más sinks habría que agregarlo. Los
  streams se releen cada 2s (tick de sistema), así que una app que empieza a
  sonar aparece en el panel siguiente, no al instante.

- Teclado en los menús, límites conocidos: los campos de texto (hex y nombre de
  paleta propia) no son stops de la navegación (su resaltado sale de
  `custom_focus`, no de `hovered`, así que un stop ahí sería invisible): hay que
  clickearlos para escribir. El panel de ajustes tampoco scrollea: si algún día
  un contenido no entra en la pantalla, la navegación por flechas no lo alcanza.
  El desplegable de fuentes arranca resaltando la primera fila (no la fuente
  actual) porque `dropdown_scroll` es 0 y un índice alto dejaría el resaltado
  fuera de la ventana visible.

- El tray ignora wifi y bluetooth por heurística de nombre (`nm-*`,
  `blueman`, `bluetooth`): si el widget Network/Bluetooth está deshabilitado, esos
  items quedan fuera del dock por completo (no hay configuración para volver a
  mostrarlos).

- El panel debajo del dock (ajustes y los tres del overlay) está implementado sólo
  para el borde superior; con Left/Right/Bottom el panel sigue tapando el dock.
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
