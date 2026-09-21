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

 1. **El tick del reloj es la única vía de los datos de 1 s, y dormía 20 s.**
   `spawn_clock_ticker` (la única llamada a `App::refresh_clock`) hacía
   `sleep(20s)`. El reloj muestra HH:MM, así que 20 s le sobraban y nadie lo notó
   hasta que la **grabación** colgó de ese mismo tick: `refresh_recording` es un
   `stat` que tiene que verse en ~1 s y su contador es MM:SS. Con 20 s, la isla
   tardaba ese tiempo en ver el archivo (el reloj de la isla incluido) y el
   contador avanzaba de a 20, no de a 1. Ahora son 1 s.
   **Lo que costó la vuelta**: el pendiente decía que "la verificación en vivo
   falló — con el archivo presente la isla NO mostró la actividad" y sospechaba de
   `refresh_clock` o de `dirs::cache_dir()`. Ninguna de las dos: el dato llegaba,
   sólo llegaba tarde. La espera de la prueba (2 s) era más corta que el tick (20
   s), así que el bug estaba *en el test*, escondido detrás de un tick de reloj
   que no se mira. Lección: antes de instrumentar con `log::debug!`, mirar **cada
   cuánto corre** el tick que alimenta el dato — `grep -n "from_secs" src/main.rs`
   responde eso en un segundo.
   Medido en reposo: 4 jiffies en 20 s con tick de 1 s contra 11 con el de 20 s
   (dentro del ruido: el tick que domina es el de 2 s del sistema, que spawnea
   `wpctl`), y 0 líneas de log (no hay redibujados de más: `refresh_recording`
   devuelve `false` cuando no hay archivo).

 2. **Lo que puede colgarse por algo externo va en un hilo, y el techo lo pone la
   conexión.** Dos auditorías (A3 y A4) encontraron lo mismo dos veces: una llamada
   D-Bus (`GetLayout` del tray, default de zbus **25 s**) y un `curl` de carátula
   (hasta 3 s) corriendo en el hilo que dibuja. El síntoma es "el dock se trabó" y no
   hay log que lo delate, así que se paga una sesión entera. Las dos quedaron en
   hilos propios (`tray::spawn_menu_worker` y el watcher de media) y el resultado
   vuelve por el canal de IPC (`IpcMessage`) o por un segundo aviso.

- **El techo va en la conexión, no en cada llamada**: `tray_conn()` crea la sesión
     con `method_timeout` de 500 ms (`tray::TRAY_CALL_TIMEOUT`), medido contra
     `GetLayout` real (10-20 ms) para no cortar una respuesta lenta pero legítima.
     Un `--max-time` en el `curl` es lo mismo del lado de la red.
- **Un resultado asíncrono necesita un guard de "todavía me interesa"**: el menú
     que llega tarde no puede pisar el panel que el usuario abrió entretanto
     (`tray_menu_still_wanted`, con test). El mismo problema, del otro lado, es
     trampa 15 (un resultado que llega y nadie dibuja).
- **Y hay que poder probarlo**: `scripts/fake_sni_hang.py` es la app del tray que
     nunca contesta, y el tarpit local de `/tmp` la red que nunca contesta. Con
     `10.255.255.1` (lo que sugería AUDIT) el `connect` rebota en 85 ms y **la prueba
     no prueba nada**: un host que falla rápido no ejercita un timeout.

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
- `scripts/pointer.py` — puntero virtual por uinput para tests (también
  `scripts/sweep_vertical.py` para el dock vertical y `scripts/fake_sni_hang.py`
  para una app del tray que nunca contesta `GetLayout`).

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
- **El dispositivo tarda ~1,6 s en ser reconocido**: si emitís eventos apenas después de
  `UI_DEV_CREATE` se pierden los primeros (o todos). `pointer.py` duerme 1.6 s y
  `click_at.py` lo tapa con el `sleep(2.0)` de `park_away`; un script nuevo tiene que
  esperar explícitamente o parece que "la app no recibe nada".
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
- `--shift-arrow left|right|up|down [--times N]` en vez de clickear teclea Shift+esa
  flecha por un **teclado** virtual (otro id de producto, `0x5679`): es la única forma
  de ciclar los modos del overlay sin tocar el teclado real. Necesita un modo abierto
  (la superficie tiene que estar con el teclado en `Exclusive`) e imprime la línea
  `overlay: X -> Y` del log, que es la confirmación. Con el dock vertical la banda de
  pestañas es una columna y ↑/↓ también ciclan, así que `--shift-arrow up|down` sirve
  para probar ese gesto.
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
  python3 scripts/pointer.py --key down --times 3
  python3 scripts/pointer.py --key escape   # cierra el panel
  niri msg --json layers | grep dockyrs          # interactividad de vuelta en None
  ```

- `--hover` deja el puntero donde llegó y espera en vez de clickear: es lo que
  ejercita lo que se abre con hover, hoy el calendario del reloj. Imprime la última
  línea `calendario:` del log (o el aviso de que no se abrió). Para saber dónde caer
  primero se busca el widget con un click normal (`--button L`, que imprime el hit) y
  después se repite el mismo `--extra` con `--hover`. El reloj está al final del
  clúster derecho:

  ```sh
  python3 scripts/pointer.py --extra 600 --button L   # -> Some(Clock)
  python3 scripts/pointer.py --extra 600 --hover      # calendario: abre center=…
  python3 scripts/pointer.py --key right --times 2    # → mes siguiente (x2)
  python3 scripts/pointer.py --key escape             # cierra el panel
  ```

- **`scripts/click_at.py X Y`** clickea (o deja el puntero, con `--only-move`) en una
  coordenada ABSOLUTA. Es lo que hace falta cuando `pointer.py` no sirve, que es el caso
  de los dos sensores de arriba: su reveal necesita una transición oculto→visible (con un
  panel del overlay abierto el dock ya está visible y el script espera para siempre) y
  barre la franja de arriba, pensada para el dock `Top`. Acá el puntero se parkea en la
  esquina superior izquierda (clamping) y se mueve en deltas conocidos (~3.05 px reales
  por paso de 3 y ~2.04 por paso de 2), así que aterriza con unos pocos px de error:
  sirve para una banda de pestañas o un widget, no para un borde de 1 px.

  ```sh
  dockyrs --toggle-search                  # el panel abierto es lo que hace falta
  python3 scripts/click_at.py 47 346       # 2ª pestaña con el dock en Left
  python3 scripts/click_at.py 1720 47      # 3ª pestaña con el dock en Top (3440 de ancho)
  python3 scripts/click_at.py 110 358 --only-move   # hover sobre una tarjeta
  ```

  Ojo con la cuenta en un setup multi-salida: las coordenadas son de la salida donde
  quedó el puntero, y el ancho importa (`niri msg --json outputs` da el layout). En el
  ultrawide de este setup la superficie del dock está centrada en 3440, no en 1920.

- **Con el dock VERTICAL (`Left`/`Right`) `pointer.py` no sirve**: barre la franja de
  arriba con `rel(3, 0)`, y con la barra al costado el puntero nunca entra al blob
  (que está centrado en la vertical). Para eso está `scripts/sweep_vertical.py`, que
  parkea abajo-izquierda (clamping), **sube** hasta cruzar el blob (así detecta el
  `reveal`), y después recorre la barra con paso configurable clickeando en cada
  paso: el sensor es la línea `dock: click ... -> Some(Kind)` del log, que trae las
  coordenadas **lógicas** de la superficie, y con eso se ubica cada widget. Como
  `park_away` deja el puntero abajo (no en 0,0), el script sube en y; y después de
  cada click manda ESC por el teclado virtual para cerrar el popup que se haya
  abierto. Medido con esta config (`Left` + `Middle`): Workspaces en `y≈349`, Tray en
  `420-441`, Clock en `445-474`.
- **Para las llamadas que pueden colgarse hace falta un servicio que cuelgue**:
  `scripts/fake_sni_hang.py` registra un StatusNotifierItem con `Menu` y un dbusmenu
  cuyo `GetLayout` **nunca contesta** (usa `async_callbacks` y no los llama, así el
  loop sigue vivo y sí recibe el fallback). Imprime con marca de tiempo
  `GETLAYOUT` / `ACTIVATE`, que es lo que se mide: el hueco entre los dos es el
  `method_timeout` de la conexión (500 ms) y `ACTIVATE` es el fallback de menú vacío.
  Dos cosas que costaron: el `BusName` **hay que retenerlo** en una variable (sin
  referencia se libera y el dock no puede resolver el item: el watcher lo lista
  igual, porque guarda el string), y los callbacks de python-dbus tienen que
  llamarse exactamente `reply`/`error` (los busca **por nombre**).
- **Una red que "no responde" de verdad** no es `10.255.255.1`: en esta máquina
  rebota en 85 ms (`curl` rc=7) y no ejercita ningún timeout. Lo que sirve es un
  tarpit local: un socket que acepta la conexión y **no contesta nunca** (`python3`
  con `accept()` y un `threading.Event().wait()` por conexión), apuntado por un
  `playerctl` falso en el `PATH` (ver trap 15 y A4).

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
  `handlers.rs`). El auto-repeat de esa flecha lo traga `accion_de_la_banda`
  (`overlay_cycle_key`): cicla la primera pulsación y las repeticiones se tragan sólo
  con el overlay abierto, para no comerse el ←/→ repetido del panel de ajustes. Ojo:
  **en niri 26.04 el bucle no se reproduce** — con el binario sin el guard y la flecha
  sostenida 2,5 s por uinput el log muestra UN ciclo: el compositor no manda
  repeticiones al cliente. El guard es para los que sí las sintetizan (sctk lo hace si
  recibe `RepeatInfo::Repeat`). Los tres
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
  `app::overlay_tabs_tests`. La banda **es clickeable**: `menu_render::overlay_tab_layout`
  (la misma función que dibuja) da el reparto de los slots y `overlay_tab_at` traduce un
  punto del panel a la pestaña, así que el click va **derecho** a la que se clickeó en
  vez de ciclar (`switch_overlay`, compartido con el ciclo de Shift+←/→).
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
  vecino. El HUD de workspaces ya no tiene geometría propia: su superficie mide y
  se ancla IGUAL que la del dock (`base_size` + `edge_anchor_margin`), y el
  indicador se dibuja en el rectángulo que `hit_layout` le da al widget, así que
  el punto cae exactamente donde estaba en el dock y su hit test es directamente
  `workspace_dot_hit`. Guard:
  `render::workspaces::workspace_hit_tests::la_pastilla_del_hud_sale_del_rectangulo_del_dock_no_de_un_panel_centrado`.
  Verificado contra píxeles (vertical con `Left` y horizontal con `Top`): los tres
  puntos del HUD caen en las mismas coordenadas que los del dock (y 541.5/568/594.5
  en vertical, x 958.5/985/1011.5 en horizontal).
  Verificado contra pixeles: el centro que el app cree que
  tiene cada widget coincide con el icono dibujado (wifi local 152 vs 151.5,
  tray 465 vs 464.5) y clickear el centro visual de cada uno da el widget
  correcto (power 23, battery 55, wifi 150, bluetooth 180, EN 210, tray 458).
- **El HUD de workspaces se alinea con el indicador del dock (vertical y
  horizontal).** El HUD era un panel propio del largo del indicador + relleno,
  centrado con `dock_align`: con zonas desparejas (en este setup, 6 widgets a la
  izquierda y tray+reloj a la derecha, Workspaces en `Middle`) el reparto NO deja
  el indicador en el centro del dock, así que al ocultarse el dock el punto saltaba
  de lugar (~27 px hacia arriba, medido). Ahora el HUD **es** la superficie del
  dock: mismo `base_size` y mismo `edge_anchor_margin`, y `draw_ws_flash` dibuja el
  indicador en el rectángulo que le da `hit_layout` y la pastilla alrededor de ese
  rectángulo (`ws_flash_pill`, relleno simétrico).
  - Muere la geometría propia del HUD (`ws_flash_panel_len`, `ws_flash_dot_hit`): el
    hit test del click es `workspace_dot_hit`, el del dock, así que no hay una
    segunda cuenta que se pueda desincronizar (trampa 1 / trampa 10).
  - `show_ws_flash` quedó idempotente (sólo re-aplica anchor/margin/tamaño si
    cambiaron): el HUD ya no cambia el tamaño de la superficie, así que un `set_*`
    de más sólo servía para disparar un configure que puede perder el foco del
    puntero (mismo criterio que `apply_panel_size`).
  - Verificado contra píxeles en las dos orientaciones: los tres puntos del HUD
    caen en las mismas coordenadas que los del dock.
  - Guard: `render::workspaces::workspace_hit_tests::la_pastilla_del_hud_sale_del_rectangulo_del_dock_no_de_un_panel_centrado`
    (el reparto del caso es desparejo — si no, el test no prueba nada — y la
    pastilla enmarca el rectángulo simétricamente en las dos orientaciones).
- **Menús del tray por click derecho** (tres bugs distintos que se veían todos como
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
    (`reveal_dock_if_stays` + ocultado inmediato al dejar de estar vacío).
    `request_redraw` se saltea si hay un frame pendiente, y `show_ws_flash` decide
    con `dock_visible` en el mismo bloque: dejarlo al dibujo hacía que el HUD
    apareciera y el dock se revelara tarde (o al revés, que al pasar vacío →
    ocupado no apareciera el selector y quedara el dock completo).
  - **El HUD comparte la superficie y `show_ws_flash` sale si ve `dock_visible`**,
    así que no cierra el HUD ya abierto: al revelar hay que pasar por
    `close_ws_flash_for_dock()` (devuelve layer y tamaño), que también usa
    `reveal_dock`.

- **Overview de niri abierto ⇒ dock fijo**: con el Overview abierto el dock no se
  oculta (y se revela si estaba oculto), así que se ve la barra sobre la vista
  alejada de los workspaces. El estado llega por el **mismo** event-stream de niri
  (`OverviewOpenedOrClosed` → `IpcMessage::OverviewChanged(is_open)` →
  `App::set_overview_open`), que además lo manda con el estado inicial al
  conectar, así que no hay sondeo ni `niri msg` extra (misma jugada que el layout
  de teclado). `set_overview_open` resuelve la transición donde se decide
  (`request_redraw` se saltea con un frame pendiente): al abrir revela, al cerrar
  oculta ya si corresponde. La condición vive en UNA función,
  `dock_stays_visible()` = `empty_workspace() || overview_open`, que usan el reveal
  y `should_hide()`: si se despegan, el dock se revela y se oculta en el mismo
  frame. Hyprland no tiene Overview, así que ahí `overview_open` queda en false.
  Test: `ipc::niri_event_tests::el_filtro_del_event_stream` (el booleano se busca
  dentro del evento, no en la línea suelta).

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
    A mano: `scripts/pointer.py --key up|down|enter|escape` y las lineas `teclado:`
    del log (sin sudo: `/dev/uinput` tiene ACL para `nicomosty`, ver "Testear sin
    mouse").

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
  - **Lo que quedaba distinto era el alto** (236 / 460 / 196 en horizontal): el ancho
    era lo que se percibía al ciclar. En el horizontal sigue así a propósito; en el
    vertical se unificó (ver el bullet del alto común).
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
  - **El click de VERDAD ya se puede probar sin sudo**: `/dev/uinput` tiene una ACL
    que le da `rw` a `nicomosty` (`getfacl /dev/uinput` → `user:nicomosty:rw-`), así
    que `scripts/pointer.py` corre como usuario normal. Las notas viejas de este
    archivo que decían "necesita sudo" quedaron desactualizadas (el `sudo` no
    molesta, pero si no hay contraseña a mano hace creer que la verificación es
    imposible y no lo es). Verificado el 2026-09-20 inyectando clicks reales: el log
    del dock imprime `dock: click derecho (x,y) -> Some(Kind)` con las coordenadas
    lógicas de la superficie, que es el sensor que usa `scripts/sweep_vertical.py`.

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

- **Ajuste "Matugen Apps"** (Colors): usa el matugen **del usuario** —o sea su
  `~/.config/matugen/config.toml` y sus templates— con el "Matugen Style" del dock,
  así las apps que ya siguen matugen (kitty, waybar, rofi, niri, gtk, …) quedan
  sincronizadas con el dock. `DockSettings::matugen_apps`, default **apagado**
  (prenderlo reescribe configs de otras apps, y su `config.toml` puede incluir un
  template de niri: trampa 8).
  - **Por qué hacía falta**: en este setup el kitty no lo tiñe el dock sino el
    matugen propio (incluye `current_theme.conf`, generado por
    `wallpaper-change.service`), que corre **sin `--type`** → apps y dock quedaban
    con esquemas distintos (`scheme-tonal-spot` contra `scheme-neutral`), y cambiar
    "Matugen Style" no tocaba las apps.
  - **El orden importa**: elegir fondo pasa por `wallpaper::apply_wallpaper(path,
    Some(scheme))`, que corre el matugen del usuario **después** de
    `apply_with_swaybg` (`systemctl start` bloquea el oneshot, así que el matugen
    del service ya terminó) → gana el esquema del dock, sin tocar el systemd del
    usuario. Costo: 2 corridas de matugen por fondo (service + dock).
  - Piezas: `wallpaper.rs` (`run_matugen` + `apply_wallpaper` con el esquema),
    `app/wallpaper_picker.rs` (`apps_matugen_scheme`, `apply_matugen_to_apps`),
    `menu/settings.rs`, `menu/dock_menu.rs` (fila en Colors) y los disparadores:
    `choose_wallpaper`, `choose_matugen_scheme`, `choose_theme` y
    `on_setting_changed` (prender la fila aplica ya). Guard:
    `menu::settings::menu_border_tests::matugen_apps_esta_en_colors_y_arranca_apagado`.
  - `apply_matugen_to_apps` es fire-and-forget (hilo propio): matugen tarda 1-2 s y
    no puede frenar el loop.
  - **Modo y esquema extra**: "Light Mode" (toggle, Colors, default apagado) manda
    `--mode light` al `extract_color_scheme` del dock y al `run_matugen` de las apps;
    `DockSettings::matugen_mode()` es la única traducción bool→`"dark"/"light"`.
    Ojo: `extract_color_scheme` lee `colors.<rol>.<mode>.color`, así que la clave del
    JSON **sigue al `--mode`** — con `"dark"` horneado, light devolvía siempre
    `None`. El desplegable "Matugen Style" ahora incluye `scheme-smart` (existía en
    matugen 4.2.0 y faltaba acá): es una línea en `MATUGEN_SCHEMES`. Guard:
    `menu::settings::menu_border_tests::matugen_light_esta_en_colors_y_es_toggle`.
  - **Los `sync-kitty/dolphin/p10k-theme.sh` del repo quedaron como estaban**, pero
    ojo: escriben `~/.config/kitty/matugen-colors.conf`, que el `kitty.conf` de este
    setup **no incluye** (incluye `current_theme.conf`), así que ese sync es letra
    muerta acá. Es candidato a borrar o a apagar si el ajuste nuevo está prendido.

- **El arranque en frío ya no espera las lecturas del sistema.**
  `WidgetSnapshot::refresh` corría **antes** de `Connection::connect_to_env` y de
  crear la layer, así que todo lo que tardara dejaba al dock sin superficie. Medido con
  una traza temporal: de los 255 ms del arranque en caliente, **211 eran esas
  lecturas** y sólo 44 el connect + binds + `list_font_families`. El desglose:
  `battery` 66 ms (el `capacity` del EC tarda eso en esta laptop — 66,8 ms medidos
  con python, el `status` 0,17), `bluetoothctl` ×2 41 ms, y 20 ms por cada
  subproceso (`playerctl`, `niri msg` ×2, `wpctl`, `iw`). Al encender el PC cada
  spawn arranca con el disco frío y el sistema cargado, y el techo de
  `run_with_timeout` es 500 ms **por lectura**, en serie: varios segundos hasta que
  la superficie existe. Dos cambios, medidos por separado:
  - **(1) En paralelo** (`std::thread::scope`): el arranque cuesta la lectura más
    lenta y no la suma. Las baratas (`time`, `date`, `cpu`, `ram`, que ya no lee
    dos veces) quedan en línea. A/B con `niri msg --json layers` (exec → el dock
    aparece en la lista), 5 corridas: `mediana 0,167 s → 0,076 s` (0,153-0,227
    contra 0,061-0,078) en caliente y **3,55 s → 1,02 s** con el peor caso
    simulado (los seis comandos reemplazados por stubs que duermen 500 ms).
  - **(2) Split**: el primer frame usa `WidgetSnapshot::refresh_quick` (sólo
    `time`/`date`/`cpu`/`ram` locales más `workspaces`, `network` y `kblayout`, en
    paralelo: **21-41 ms medidos**) y las cuatro lecturas que pueden tardar
    —batería, media, bluetooth, volumen— las trae `read_deferred` en un hilo que
    manda `IpcMessage::DeferredWidgets` con el dato **ya leído** (los `refresh_*`
    releen, y acá el punto es no volver a esperar). Llegan a los 63-107 ms, o sea
    mientras el resto del arranque sigue; `App::apply_deferred_widgets` las pega y
    hace `relayout_dock`, sin pasar por los `refresh_*`.
  - El canal de IPC se crea **antes** de tocar Wayland para que el hilo arranque
    temprano, y `read_deferred` gatea cada lectura con `has_widget`: con la config
    de fábrica `read_media` ya no corre sin un `Media` colocado.
  - **El pop-in es el precio y se ve en el log**: `481x26` (sin
    batería/volumen/bluetooth) → `560x26` a los 71 ms (llegó lo diferido) →
    `576x26` a los 2 s (tick de sistema). Los tres widgets diferidos no dejan
    hueco ni placeholder: `draw_battery_widget` y `draw_volume_widget` `return`
    temprano con el dato en `None`.
  - **Por qué `workspaces` NO se difiere** (es el único dato que no se puede):
    `dock_stays_visible()` (regla de workspace vacío) y `show_ws_flash` se deciden
    con él, y con la lista vacía el primer `refresh_workspaces` vería
    `before=None → after=Some(1)` y haría parpadear el HUD con el dock ya oculto.
  - No hace falta test de mapeo del tuple: los siete tipos son distintos
    (`Option<(u8, BatteryState)>` vs `Option<(u8, bool)>` vs `Option<MediaInfo>` vs
    `Option<BluetoothInfo>` vs `Vec<WorkspaceInfo>` vs `NetworkInfo` vs `KbLayout`),
    así que un campo cruzado es error de compilación. Un hilo que paniquea no tumba
    el arranque: su campo cae al default y el tick lo vuelve a leer.
  - Verificado en un reinicio real del equipo: el usuario reporta que el dock
    arranca en la mitad del tiempo (el único dato de boot real que hay, contra el
    A/B simulado de arriba).
  - **Lo que sigue en el camino crítico**: tres spawns en paralelo (`niri msg` ×2 y
    `iw`). Con el peor caso simulado el arranque quedó en **1,03 s**, y el culpable
    es `read_network`, que encadena `iw` y, si falla, `ip` (dos spawns **en
    serie**), no la batería ni el bluetooth, que ya no bloquean. Diferir la red lo
    bajaría a ~0,5 s; no se hizo porque el ícono de wifi mostraría "sin conexión"
    un instante. `read_bluetooth` sigue siendo dos spawns en serie, pero ahora en
    background.

- **Barra vertical (`Left`/`Right`): el reloj y el nub de la batería ya entran.** El
  reloj dibujaba la hora y la fecha como **dos columnas rotadas** lado a lado, y
  cada línea de texto mide `1.5 * text_px` en el eje del grosor: con `font_scale`
  1,51 son ~22 px contra los ~25 px que mide la barra, así que se veía cortado —la
  hora perdía la mitad de las letras contra el borde de la pantalla y la fecha se
  metía en la esquina redondeada— (medido en píxeles: el bloque del reloj era el
  único sin margen, tinta de x 0 a 25). Ahora es **una sola columna** rotada con la
  hora y la fecha, en el mismo orden que la barra horizontal ("10:37 PM 17-Sept"
  leído de abajo hacia arriba) y `len_clock` reserva la misma suma en las dos
  orientaciones (trampa 12). Guard:
  `widget::clock_vertical_tests::el_reloj_vertical_mide_la_suma_y_entra_en_el_grosor`
  (exige la suma **y** que `1.5 * text_px` entre en 25,02). Dos de la misma familia,
  en el mismo commit: el **nub** del polo de la batería sumaba `3 * render_scale` al
  ancho y `bx` centraba sólo el cuerpo, así que se cortaba contra el borde (ahora se
  centra icono + nub), y la **pastilla del SSID** del hover de Network se dibuja
  horizontal y en vertical se recortaba: se saltea con `dock.is_vertical()` (rotada
  tampoco entra: `1.5 * font` + padding de alto).
  - Verificado con la config del usuario (`Left` + `Middle`, `widget_scale` 1,1065,
    `font_scale` 1,5132): todas las bandas de tinta caen dentro de la barra con
    margen ≥1 px (el reloj pasó de x 0..25 a 7..16) y la barra creció de 26x548 a
    26x610, porque el reloj ahora ocupa una columna más larga.
  - **Cómo revelar el dock sin puntero con el borde `Left`**: abrir el **Overview de
    niri** (`niri msg action open-overview`, y `niri msg overview-state` antes para
    no togglear a ciegas), que además deja el dock fijo. `focus-workspace` a un
    workspace vacío no sirve: el workspace aparece y desaparece entre eventos y el
    HUD de workspaces tapa al dock.

- **Los tres paneles del overlay adaptados al dock vertical (`Left`/`Right`).** El
  portapapeles **no tenía rama vertical**: `open_clipboard` usaba `OVERLAY_PANEL_W` y
  `clip_panel_h()` a secas, así que con el dock al costado abría el panel horizontal
  (medido: 640x460) con la banda de pestañas en una fila que se salía del panel. El
  launcher y el selector de fondos sí tenían rama, pero el launcher medía **66** de
  cross (`dock_thickness.max(icono 46 + 2*MENU_PADDING)`), o sea los nombres cortados
  a ~10 caracteres (`Antigravit…`, `AppimageL…`) y la caja de búsqueda en `Sea…`.
  - Ahora los tres usan el mismo **cross** en vertical, `OVERLAY_PANEL_VERTICAL_W` =
    170 (el que ya tenía el selector de fondos, la miniatura de 150 más los dos
    `MENU_PADDING`), con el mismo motivo que `OVERLAY_PANEL_W` en horizontal: el
    borde no se mueve al ciclar con Shift+←/→. Medido en el log: Apps 170x660,
    portapapeles 170x538, fondos 170x744 (antes 66x660, 640x460, 170x744).
  - La tarjeta vertical del launcher pasa a ser una **tile** de 150x92 igual a la
    miniatura de los fondos: icono de 46 centrado, el nombre entero debajo y el
    bloque centrado en la tile (la tarjeta mide 92 y el contenido 66: con el icono
    pegado arriba quedaba todo el aire abajo y la pastilla se veía pesada).
  - El portapapeles estrena la banda **apilada** (como los otros dos: `stacked =
    args.is_vertical`; antes era `false` fijo con el comentario "es una caja ancha
    siempre"), el alto sale de `clip_content_y(is_vertical)` (la banda entra en la
    cuenta; antes estaba horneado `OVERLAY_TABS_H`) y la descripción de cada fila se
    elide con `fit` como el título, que en 150 px se sale. 7 filas, sin aire de más.
  - Los dos `170`/`640` que vivían en `app/mod.rs` (`WALLPAPER_PANEL_H`,
    `WALLPAPER_PANEL_MIN_W`) ahora apuntan a las constantes de `menu::` en vez de
    repetir el número: eran tres copias del 170.
  - Guards: `menu_render::clipboard::clip_vertical_tests` (el contenido mide lo
    mismo en las dos orientaciones y la primera fila arranca en el frame) y
    `menu::app_search::strip_tests` (el panel vertical es la grilla de 2 columnas).
    Verificado con screenshots de los tres modos por IPC (`--toggle-search` /
    `--toggle-clipboard` / `--toggle-wallpaper`), 0 errores en el log. "Windows"
    comparte panel y geometría con "Apps" (`open_search`).
  - **Ojo**: la banda apilada arriba de este bullet duró un turno; el diseño que
    quedó es el del bullet siguiente (columna al costado).

- **Banda de pestañas en COLUMNA y grilla de apps de 2 columnas en vertical.** Pedido
  textual: "el menú de Apps|Clipboard|Wallpapers|Windows de manera vertical en la
  parte más izquierda o derecha" y "el grid de las apps 2 x cualquier ancho".
  - **La banda ya no es un tramo del eje largo**: es una fila de 26 arriba en los
    paneles anchos y una **columna de 26** pegada al lado del dock (izquierda con
    `Left`, derecha con `Right`) en el vertical, con las etiquetas **rotadas 90°**
    (`super::osd::draw_text_rotated`, el mismo helper que el OSD: se leen de abajo
    hacia arriba como los widgets de la barra vertical). Apilada arriba le comía 104
    de alto a un panel que ya es una columna; ahora el panel es más bajo y un poco
    más ancho.
  - **El reparto de la banda y el contenido vive en `menu::PanelFrame`**
    (`frame_for`/`panel_size`/`band_rect`): el frame es la caja del CONTENIDO y todo
    el reparto de los tres paneles (buscador, filas, tiles, filmstrip) y sus hit
    tests salen de ahí, así que la banda se cuenta UNA vez y el dibujo no puede
    quedar corrido respecto del click. `overlay_tabs_h(is_vertical)` **se borró**:
    era el único lugar donde "vertical" significaba "104 de alto" y ya no existe.
    `Dock::panel_frame(panel_w, panel_h)` es el inverso de `menu::panel_size`.
  - El launcher vertical pasa a **2 columnas** de tarjeta de 150 (`APP_CARD_W_VERTICAL`,
    `app_search_cols` calcula las columnas del ancho del frame y el grid es row-major
    con scroll en y, como el horizontal pero en el otro eje). Medido: Apps 356x556,
    portapapeles 196x434, fondos 196x640 (el cross de los tres es el frame de 170 más
    los 26 de la banda, salvo el launcher que pide 330 de frame para las dos columnas).
  - Los tiles sin ícono en el tema ahora usan `render::draw_placeholder` (pasó a
    `pub(crate)`): `Advanced Network Configuration` (`preferences-system-network`,
    sólo en `AdwaitaLegacy`) y los tres `Avahi *` eran pastillas vacías en la grilla.
  - **Las cuatro pestañas miden lo mismo y no cambian con el panel.** La pastilla (el
    resaltado de la activa) sale del texto de la etiqueta MÁS LARGA ("Wallpapers"), no
    del de cada una: antes medía 45px con "Apps" y 74 con "Wallpapers" y la banda
    parecía cambiar de tamaño al ciclar (medido). Y en la columna el alto de cada
    pestaña sale de ese mismo texto (`pill_len + 10`), no de `bh / 4`: con `bh / 4` el
    launcher abría con pestañas de 139, el portapapeles de 108 y los fondos de 160, así
    que la banda "saltaba" al cambiar de pestaña. Verificado: las tres abren con slot
    de 84 y pastilla de 74.
  - **En el panel vertical las pestañas también se ciclan con Shift+↑/↓** (además de
    Shift+←/→, que sigue igual): la banda es una columna, así que la flecha que la corre
    es ↑/↓. Las flechas SIN Shift siguen siendo de la lista del panel (el grid del
    launcher salta una fila, el portapapeles mueve la selección), así que no se pisó
    nada: lo decide `flecha_de_la_banda(keysym, is_vertical)` (con test) y la dirección
    `overlay_cycle_dir` (↓/→ = siguiente). A mano: `scripts/pointer.py --shift-arrow
    up|down` (las cuatro flechas ahora).
  - Guards: `menu_render::tabs::la_banda_esta_fuera_del_frame` (el contenido arranca
    donde termina la banda, en fila y en columna, con el dock a los dos lados),
    `las_pestanas_se_reparten_a_lo_largo_del_panel` (el bug de la primera versión de
    la columna: los slots se dividían sobre los 26px del grosor y las cuatro
    etiquetas se apilaban en 6.5px), `menu::app_search::strip_tests::el_vertical_es_una_grilla_de_dos_columnas`
    (la 2ª al lado de la 1ª, la 3ª vuelve a la primera columna y las dos entran en el
    ancho útil), `el_click_del_vertical_sigue_las_dos_columnas` (el hit test con la
    banda a la izquierda y a la derecha) y `el_frame_y_el_panel_son_inversos`.

- **El dock queda a la vista con el panel abierto en los CUATRO bordes.** En el
  launcher con el dock al costado no se veía la barra: `panel_layout` sólo armaba
  `[dock][gap][panel]` para el borde superior y en los otros tres el panel ocupaba la
  superficie entera (y tapaba el dock, que vive en esa misma superficie). Ahora el
  reparto cubre los cuatro bordes —debajo con el dock arriba, encima con el dock abajo,
  a la derecha con el dock a la izquierda y a la izquierda con el dock a la derecha— y
  el par queda alineado con `dock_align` en el eje que no comparten.
  - El resto no se tocó porque ya salía de ahí: `show_panel_surface` dibuja el dock en
    `dock_at` y pega el panel en `panel_at` (un solo `attach`), `panel_local` traduce el
    puntero con el mismo offset (si no, los controles quedan muertos: trampa 4) y el
    agujero del click-catcher usa el tamaño de la SUPERFICIE, así que ahora cubre el
    dock y el panel juntos.
  - Medido en el log: launcher 390x610 (26+8+356), portapapeles 390x610, fondos
    230x640, panel de ajustes 555x610; el dock se dibuja en x 0..25 y el panel arranca
    en 34, los dos centrados en el alto de la superficie.
  - Guards: `app::dock_menu::panel_layout_tests::el_panel_va_al_lado_del_dock_en_los_cuatro_bordes`
    (los cuatro bordes con sus offsets, y que el panel entre en la superficie) y el de
    siempre `catcher_tests::el_agujero_cubre_el_dock_y_el_panel`.
  - **Lo que NO hace**: el panel no se aparta del dock cuando el dock crece (el
    `pos_y` sigue moviendo los dos juntos) ni hay animación de apertura entre ellos.

- **El dock aparece y se oculta como una isla dinámica, y en reposo queda un blob
  con una actividad viva.** Dos piezas, en el mismo motor:
  - **`reveal_anim` (0 = isla compacta, 1 = dock entero)**: la aparición/colapso del
    dock morphea la cápsula en vez de aparecer de golpe. `draw(.., reveal)` dibuja el
    dock completo en un pixmap aparte y lo pega **recortado por la máscara de la
    cápsula** (`reveal_mask`), con la opacidad subiendo con el largo. El piso es el
    blob de la isla, no la nada: el dock **no desaparece, se encoge**.
  - **`draw_island`**: el estado que se ve MIENTRAS el dock está oculto. Misma
    cápsula que el dock en `reveal = 0`, pero con las actividades adentro
    (`island_activities`: **Media si está sonando > Volumen**, y la **batería** de
    segunda siempre que haya dato) dibujadas con el **mismo `draw` del widget**
    (`layout::draw_one_widget`), pegadas a lo largo de la cápsula, no con una segunda
    versión. Sin dato no hay isla: queda el buffer transparente de siempre.
  - **Una sola cuenta de la forma**: `reveal_capsule` (rect) la usan la máscara, el
    fondo (`draw_capsule`, compartido con el dock entero) y el blob de la isla; y
    `island_plan` (qué actividades + el largo de cada una + el total) la usan el
    dibujo de la isla y el piso de la animación. Si se despegaran, el dock se
    encogería a un blob y la isla se dibujaría en otro (trampa 10).
  - **La isla crece para que entren sus actividades** (`island_plan` = la medida
    natural de cada widget, con el piso de `ISLAND_COMPACT` × el grosor —2,6: son
    ~73 px de blob— y el techo del dock): Media pide ~122 px con su carátula y su
    texto, el volumen ~38 y la batería ~43 (medido con la config del usuario y
    `widget_scale` 1,1065). Con un largo fijo, al título de Media le cortaba las
    letras a la mitad y la batería no entraba.
  - **El blob tiene RELLENO en los dos extremos** (`IslandPlan::pad` = el radio de la
    esquina, clampeado a la mitad del eje corto) y un **gap entre actividades**
    (`ISLAND_GAP` = 5). No es decoración: adentro de la franja del radio el blob se
    angosta (con r=13 en un blob de 26 de ancho, a 2 px del extremo solo quedan ~20 px
    de ancho) y **la máscara le cortaba el contenido**, que se veía como "la batería
    se ve rara" — es el icono más ancho (18 + 3 del nub) y estaba mordido contra el
    borde. El contrato es `compact - 2·pad ≥ suma + gaps` y lo custodia
    `la_isla_crece_para_que_entren_las_actividades` (calcula la franja útil).
  - **El piso NO se aplica sin actividades**: ahí `compact = 0` es "no hay isla" y el
    dock colapsa a nada (si no, el reveal terminaría en un blob que `draw_island` no
    dibuja). Lo cazó el mismo test.
  - **El buffer NO se redimensiona para animar** (trampa 1): el morph es todo dentro
    del pixmap; el `set_size` sigue siendo sólo el del relayout del dock. Y
    `draw_island` reusa la superficie y el pool del dock, así que no suma ni una
    superficie (tampoco un frame callback de más: el que pide lo consume el tick del
    reveal, que se apaga solo al llegar al destino).
  - **La isla no prende lecturas nuevas**: los datos son los de `refresh_*`, que ya
    están gateados por el widget colocado. Por eso Media aparece en la isla **sólo
    con el widget `Media` colocado** (que es también lo que enciende el
    `playerctl --follow`, +7 MB de hijo), y con `media_smooth_scroll` apagado el
    título se recorta en vez de scrollear, igual que en la barra. La config del
    usuario ya tiene el widget `Media` en `Left` (pedido explícito: la isla muestra
    Now Playing y la batería a la vez), así que el watcher está corriendo.
  - **Medido**: reveal `0.00 -> 1.00` en 15 frames (~230 ms) y colapso `1.00 -> 0.00`
    en 20 (~330 ms), todos por frame callback; el número de la isla cambia al instante
    con el volumen (`50` medido a los 1.5 s de un `wpctl set-volume`, contra el tick de
    2 s); con un `playerctl` falso en `PATH` se verificó el camino entero de Media
    (la isla pasó a mostrar `Tema de pr…` + la batería, ganándole al volumen) y la
    vuelta atrás al morir el player; en reposo **0-2 ticks de CPU en 6 s** con 0 líneas
    de log (el piso de siempre) y ~13,7 MB de RSS; `dockyrs` sigue en `Layer::Top` con
    `keyboard_interactivity: None` cuando está oculto.
  - **Limitación conocida**: el eje corto de la isla es el grosor del dock (26 px),
    así que el contenido va rotado como en la barra vertical y adentro se ve la
    pastilla propia del widget. Una isla más gruesa pide pre-dimensionar la superficie
    al estado mayor **y** restringir la input region a la cápsula: hoy
    `sync_autohide_surfaces` hace `set_input_region(None)` (toda la superficie es el
    disparador del hover), así que el aire reservado para crecer se comería clicks de
    la ventana de abajo. Patrón a copiar: `dock_popup::popup_input_region()`.
  - **El panel `Notifs` mide lo mismo que el portapapeles**: cross ancho en vertical
    (`OVERLAY_PANEL_VERTICAL_WIDE` = 330 + los 26 de la banda) y su alto de contenido
    (`clip_content_h()`). Medido: los dos abren **390x610**. Antes crecía con la cantidad
    de avisos y en vertical quedaba una columna de 196 de ancho, que se veía apretada.
  - **El dock oculto SÓLO se revela desde la isla**: la input region de la superficie se
    achica al blob (`render::island_blob_region`, la misma cuenta que dibuja) en vez de
    quedar en la superficie entera. Medido con `WAYLAND_DEBUG=1`:
    `wl_region.add(0, 247, 26, 116)` sobre una superficie de 26x610. Visible vuelve a la
    superficie entera, y **sin isla** (sin datos) también: si no, el dock no se podría
    revelar nunca. Efecto lateral: la franja de 26x~500 que antes se comía el dock le
    llega ahora a la ventana de abajo. Se re-aplica sólo cuando cambia (`applied_input`),
    que cada `set_input_region` es un `commit` de más.
  - **La isla NO recibe puntero, y no es un olvido**: al entrar al **blob** (que es lo
    único que el dock oculto tiene activo) el `Enter` revela el dock en el mismo handler y `should_hide()` exige `pointer_pos` en `None`,
    así que la isla existe **sólo con el puntero lejos de la superficie**. Se probó
    cablearle rueda = volumen y tap = play/pause (`island_hit` sobre `island_spans`) y
    hubo que revertirlo: era código inalcanzable. Para darle interacción hay que
    **achicar el disparador al blob** (se pierde tirar el mouse al borde) o aceptar que
    scrollear revele el dock: decisión de diseño, no de código.
  - **La isla entera SE CORRE a la altura del indicador** con el split abierto:
    `island_ws_shift` (del MISMO reparto del dock, `layout_widgets`) devuelve cuánto
    hay que desplazar el blob sobre el eje largo para quedar centrado donde el dock
    tiene el widget de Workspaces, y ese desplazamiento **viaja con el split**
    (`* ws_split`), así que el movimiento es parte de la misma animación de apertura.
    Sin esto, al revelarse el dock el indicador saltaba de lugar (el bug del HUD
    centrado). Medido en vivo: la banda de la hora pasó de `y=503` (isla cerrada) a
    `484` (split abierto), o sea ~19 px. Guard:
    `la_isla_se_corre_al_lugar_del_indicador` (caso desparejo a propósito: con el
    indicador en el centro el shift sería 0 y el test no probaría nada).
  - **Al cambiar de workspace la isla se PARTE en dos** (`island_ws_split`): el
    indicador entra EN EL MEDIO, entre la hora y la batería, en vez de que el HUD
    reemplace la isla 3 s (que se veía como "la isla desaparece"). Es una actividad
    más del plan (`island_activities(.., ws_split)` mete `Workspaces` al medio) y sus
    dos gaps crecen con el split (`plan.gap * ws_split`), así que en 0 la isla mide lo
    mismo que antes de la feature y **no hay salto** al abrirse. El largo lo anima
    `tick_island_split_frame` (mismo criterio que el reveal: el frame que se pide al
    dibujar es el reloj, y `smooth_transitions` apagado lo salta). El HUD de siempre
    queda como **fallback para cuando no hay isla** (sin actividades no hay dónde
    poner el indicador). Guard: `la_isla_se_parte_en_dos_con_el_indicador_al_medio`
    (en 0 son 2 items, en 1 son 3 con el indicador en el medio, a medias el indicador
    mide la mitad y el largo queda entre las dos). Sensor:
    `wsflash: split -> abrir la isla con el indicador en el medio` en el log.
  - **La isla muestra HORA + BATERÍA** (`island_activities`): la hora **compacta** (sólo
    `11:38`: sin AM/PM y sin fecha) y la batería de segunda. El **volumen queda en el
    dock** a propósito (ahí tiene su pastilla, su rueda y su panel) y **Media no se
    muestra**: necesitaría el widget `Media` colocado (los `refresh_*` están gateados
    por el widget) y la config del usuario ya no lo tiene. Si vuelve, es una línea en
    `island_activities`. El marquee de la isla queda cableado para ese caso
    (`advance: true` + `set_marquee_rate`), pero con media fuera de la isla no corre
    nunca: **el costo medido (18 ticks/6 s ≈ 3% de un core) sólo aparecía mientras
    sonaba música**.
  - **El "modo compacto" vive en `Ctx.compact`** (no en un segundo dibujo): `len_clock`
    y `draw_clock_widget` leen el MISMO flag y la MISMA cadena (`WidgetSnapshot::time_short`,
    que saca el sufijo AM/PM con `sin_ampm`), así que la medida y el dibujo no se pueden
    despegar (trampa 12: es el bug del reloj vertical otra vez). La barra lo arma en
    `false` (`draw_widgets`, `widget_natural_len`) y la isla en `true`
    (`draw_one_widget`, `island_plan`). Guards: `el_reloj_compacto_mide_solo_la_hora_sin_ampm`
    y `la_hora_de_la_isla_no_lleva_ampm`. El resto de los widgets ignoran el flag: el
    que necesite otra cosa en la isla lo mira él.
  - Guards: `render::reveal_tests` (el blob y el dock entero en los dos ejes, el largo
    que crece monótono, la máscara que recorta y deja el eje corto completo, la lista de
    actividades de la isla —media > volumen, batería siempre de segunda— y que el blob
    crezca para que entren, con la franja útil calculada). Sensor:
    `reveal:<ms> <antes> -> <después> visible=` en el log.

- **El `GetLayout` del tray y la carátula remota salieron del hilo que dibuja**
  (A3 y A4 de AUDIT.md, cerrados el 2026-09-20). Ninguno de los dos era visible
  hasta que se midió: son los dos únicos llamados que pueden **colgarse** por algo
  externo (una app del tray que no contesta su D-Bus, un host que no responde).
  - **A3 — el menú del tray en su propio hilo**: los dos sitios de
    `dock_popup.rs` que llamaban `tray::fetch_menu` (menú raíz y submenú) ahora
    **piden** (`crate::tray::MenuRequest`) y el resultado vuelve por el canal de IPC
    (`ipc::IpcMessage::TrayMenuReady` → `app::dock_popup::apply_tray_menu`). El
    worker es **uno solo** (`tray::spawn_menu_worker`), así las peticiones se
    atienden en orden y el último click pisa al anterior en vez de que gane el que
    conteste primero. Además `tray_conn()` pone `method_timeout` de **500 ms**
    (`tray::TRAY_CALL_TIMEOUT`) en la conexión compartida: es el techo de CUALQUIER
    llamado del tray (incluido el `GetProperty` de `resolve_item` que corre en el
    hilo del tray) en vez de los **25 s** del default de zbus. El 500 está medido:
    `GetLayout` real con gdbus dio 10,6-13,4 ms (nm-applet) y 17,5-20,2 ms (blueman).
  - **A4 — la carátula remota en el hilo del watcher**: `read_media` recibe
    `allow_network` y el hilo que dibuja SIEMPRE pasa `false` (sin red: si no está
    en caché, `art_path` es `None` y el widget se dibuja sin tapa). La descarga la
    hace `warm_media_art()`, que se llama **desde el watcher de media** después de
    avisar el título: si bajó algo, manda un segundo `MediaChanged` y la carátula
    entra en un frame posterior. El otro llamador con red es `read_deferred`, que ya
    corre en un hilo al arrancar. De paso el `curl` lleva `--fail` (D3: un 404 ya no
    se cachea como `.jpg`) y `--max-time 1` en vez de 3.
  - **Guard del resultado viejo**: `tray_menu_still_wanted(settings_open, popup_screen)`
    decide si el menú que llegó tiene a quién contestarle. Sin él, un tray lento
    (hasta 500 ms) le pisaba el panel de volumen o los ajustes que el usuario acababa
    de abrir. Test: `app::dock_popup::tray_menu_tarde_tests`. El submenú tiene su
    propio guard, más fino: `pending_submenu` (el id que espera) + el `menu_path`.
  - **Verificado end-to-end, sin sudo** (ver la ACL de `/dev/uinput` más arriba):
    - A3 con `scripts/fake_sni_hang.py` (un SNI que registra un `Menu` y **nunca**
      contesta `GetLayout`) + un click real: `GETLAYOUT` → **500 ms** → `ACTIVATE`,
      o sea el timeout disparando y el fallback de menú vacío. Y lo que importa: el
      dock **siguió procesando clicks** durante todo el cuelgue (los 11 clicks del
      barrido quedaron en el log; con el código viejo el primero habría congelado el
      hilo principal 25 s). Camino feliz con remmina: `getlayout=1ms` y el popup
      abre igual.
    - A4 con un **tarpit local** (un server que acepta y no contesta nunca) + un
      `playerctl` falso que emite metadata cada 300 ms: con la MISMA descarga en
      vuelo (verificado por pid, hija del proceso del dock) una notificación pedida
      por IPC se dibujó **0,4 s** después. Ojo: el `10.255.255.1` que sugería AUDIT
      no sirve acá, rebota en 85 ms y no ejercita nada.
  - **Lo que NO se hizo**: no hay estado "cargando" en el popup del tray (aparece
    cuando llega la respuesta: ~30 ms normal, 500 ms colgado; si viene vacío, cae al
    `Activate` de siempre). Y la descarga que falla se reintenta en cada evento de
    metadata, porque no se recuerda el fracaso (mismo comportamiento que antes, sólo
    que fuera del hilo que dibuja).

- **Categoría `Launcher` en el panel de ajustes** (pedido: configurar el launcher y
  el contenido de sus pestañas, no sólo abrirlas). El panel pasa a seis tabs:
  Layout, Appearance, Themes, Widgets, **Launcher**, System.
  - **Max Results** (10–200, default 60): tope de `.desktop` que lista el launcher.
    `refresh_app_search` dejó de usar la constante `SEARCH_MAX_RESULTS` y lee el
    ajuste (que es también el default de la config, así que no hay dos números).
  - **Sort by Usage** (toggle, default prendido): orden por frecencia contra
    alfabético. Apagado, el score es 0 para todos y manda el desempate por nombre.
  - **Clipboard Items** (10–200, default 50): tope del historial del portapapeles.
    Entra por parámetro a `ClipboardHistory::add` (`MAX_ITEMS` pasó a `pub`), y el
    techo de cordura del formato en disco subió a 256: con el de antes (50) un
    historial de 200 entradas se descartaba entero al cargar.
  - **Notification History** (10–200, default 50): tope del historial de avisos.
  - Los cuatro son `SettingId`/`DockSettings` comunes (ninguno pide relayout):
    heredan slider, teclado y guardado sin código nuevo. Guard:
    `menu::settings::launcher_settings_tests` (la categoría los expone, el toggle
    arranca prendido y los topes recortan al rango).
  - Verificado: la categoría dibuja los tres bloques (Launcher / Clipboard /
    Notifications) y con `Max Results = 10` la grilla queda en 10 tarjetas (Down×40
    satura en la 10ª, sin scroll); restaurado a 60 después de la prueba.

- **Regresión de B4: el HUD de workspaces y el split de la isla no aparecían.**
  `post_workspaces_changed` (extraído en `4a5f126`) tomaba `before` y `was_empty`
  **después** de mutar la lista, así que `before == after` siempre y `show_ws_flash`
  no se llamaba nunca (el log daba `wsflash: refresh before=2 after=2`, el valor
  nuevo en los dos lados). Los dos caminos —relectura por CLI y
  `WorkspacesChanged` con payload— ahora capturan la foto con `ws_snapshot()`
  antes de mutar y se la pasan a la función, que documenta el contrato. Verificado
  en vivo: `before=1 after=2` → `wsflash: show` → `split -> abrir la isla con el
  indicador en el medio`, y la isla muestra hora + puntos + batería.

## Cerrado de AUDIT.md (movido de ahí el 2026-09-20)

Los hallazgos de `AUDIT.md` que ya están resueltos. Cada uno conserva el análisis
original (que describía el árbol de aquel momento) y, arriba, lo que se hizo y cómo
se verificó; el número entre paréntesis es la sección que tenía en `AUDIT.md`. Lo que
sigue **abierto** vive en `AUDIT.md`, no acá.

Se movieron en dos tandas el 2026-09-20: **13** en la limpieza (A1, A2, A3, A4, A6, A7,
B3, C4, C8, D1, D2, D3, D11) y **9** al cerrar la Ronda 1 (B1, B2, B10, C3, C6, D9, D10,
D12, D13), 6 más en la Ronda 2 (B4, D4, D5, D6, D7, D8) y 6 en la Ronda 3
(B7, B8, B11, B12, C2, C7) y los 2 ultimos (B5, C5) al cerrar el audit.

### A1 — `wpctl` sin timeout: cuelgue indefinido (AUDIT §4.1)

> **Resuelto** (§3): `read_volume()` pasó a `run_with_timeout(cmd, 500ms)`, el
> mismo helper que usa el resto de las lecturas. Guard:
> `widgets::percent_decode_tests::un_timeout_mata_al_hijo_colgado`. El análisis de
> abajo describe el estado **original**.

**Archivo:** `src/widgets.rs:963-975`

```rust
pub fn read_volume() -> Option<(u8, bool)> {
    let out = std::process::Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()          // <-- sin run_with_timeout
        .ok()?;
```

**Impacto:** a diferencia de los otros subprocesos (que usan `run_with_timeout` con
400-500 ms), este no tiene cota. Si `wpctl` se cuelga —PipeWire caído,
`pipewire-pulse` a medio arrancar, D-Bus de sesión atascado— el proceso queda
bloqueado **para siempre**: el dock deja de responder y no se recupera ni con Escape.

**Alcance:** alto. `read_volume()` corre desde tres caminos calientes: `refresh_sys()`
(cada 2 s, `main.rs:467`), `read_volume_rows()` (panel de volumen) y
`widgets::volume_step()` (la rueda del mouse, desde `app/pointer.rs:384`, con relectura
en `:387`).

**Fix:** usar el helper que ya existe:

```rust
pub fn read_volume() -> Option<(u8, bool)> {
    let mut cmd = std::process::Command::new("wpctl");
    cmd.args(["get-volume", "@DEFAULT_AUDIO_SINK@"]);
    let out = run_with_timeout(cmd, Duration::from_millis(400))?;
    if !out.status.success() { return None; }
    …
}
```

**Verificación:** test que corre `read_volume` con un `wpctl` falso en `$PATH`
(`#!/bin/sh` + `exec sleep 60`) y asserta que vuelve en <500 ms. `run_with_timeout`
es privado del módulo: el test vive en `widgets.rs`, así que no hay que cambiarlo de
visibilidad.

### A2 — `matugen` sin timeout al elegir fondo de pantalla (AUDIT §4.4)

> **Resuelto por dos vías.** `extract_color_scheme` (los colores que el dock saca del
> fondo) pasa por `widgets::run_with_timeout` con tope de **10 s** (`wallpaper.rs:347`),
> y el matugen **del usuario** (`run_matugen`, el de “Matugen Apps”) no lleva timeout
> pero sale por un hilo propio fire-and-forget en sus dos llamadores
> (`apply_matugen_to_apps` y `apply_wallpaper`), así que ninguno de los dos puede
> congelar el hilo que dibuja. Lo que queda de este hallazgo: un matugen colgado deja un
> hilo vivo.

**Archivo:** `src/wallpaper.rs:292-305` (llamado desde `app/wallpaper_picker.rs:159`)

```rust
pub fn extract_color_scheme(path: &std::path::Path, scheme: &str) -> Option<ColorScheme> {
    let output = std::process::Command::new("matugen")
        .args(["--type", scheme, "image", &path.to_string_lossy(), …])
        .output()          // <-- sin timeout
        .ok()?;
```

Cadena: click/Enter en una miniatura → `choose_wallpaper`
(`wallpaper_picker.rs:121`) → `sync_accent_from_last_wallpaper` (línea 131) →
`extract_color_scheme` (línea 159) → `matugen` sincrónico → recién después
`request_redraw`. matugen decodifica la imagen y cuantiza colores: típicamente cientos
de ms a un par de segundos, y **sin cota superior** (puede colgarse).

**Son tres puntos de entrada, no uno** (`rg -n 'sync_accent_from_last_wallpaper\(\)' src/`):

| Entrada | Archivo:línea | Se dispara con |
| --- | --- | --- |
| Selector de fondos | `wallpaper_picker.rs:131` | click/Enter en una miniatura |
| Tema "desde el fondo" | `dock_menu_input.rs:150` | elegir el preset "wallpaper" en el panel |
| Cambio de esquema matugen | `dock_menu_input.rs:241` | elegir otro scheme en el dropdown |

Los dos últimos bloquean el **panel de ajustes**, no el selector de fondos: quien
depure el freeze mirando sólo el picker va a perseguir el fantasma equivocado.

**Impacto:** el selector de fondos queda congelado desde que elegís la imagen hasta
que matugen termina. La selección ya se aplicó (`apply_wallpaper` sí está en un
hilo, `wallpaper.rs:242`), pero la UI no lo muestra.

**Fix:** mover la extracción a un hilo y aplicar el resultado en el frame siguiente,
igual que ya se hace con las miniaturas (`wallpaper_picker.rs:57` usa
`thumb_request_tx`/`thumb_result_rx`): canal `mpsc` + `try_recv` en el tick del modo.
Si se quiere el fix de una línea, `run_with_timeout` con 400 ms (hacerlo
`pub(crate)`), pero **eso sigue bloqueando hasta 400 ms**; el hilo es lo correcto.

**Verificación:** con un `matugen` falso que duerme 3 s en `$PATH`, el selector debe
seguir animando y responder a Escape mientras corre.

### A3 — `GetLayout` D-Bus sincrónico en el hilo principal (AUDIT §4.5)

> **Resuelto (2026-09-20).** El `GetLayout` ya no corre en el hilo que dibuja: los dos
> sitios de `dock_popup.rs` **envían una petición** (`tray::MenuRequest`) a un hilo
> propio (`tray::spawn_menu_worker`) y el menú vuelve por el canal de IPC
> (`IpcMessage::TrayMenuReady` → `App::apply_tray_menu`, que descarta el resultado si
> el usuario ya abrió otra cosa: el guard es `tray_menu_still_wanted`, con test).
>
> Además la conexión D-Bus del tray ahora lleva `method_timeout` de **500 ms**
> (`tray::TRAY_CALL_TIMEOUT`), que es el techo de *cualquier* llamada del tray en vez
> de los 25 s del default de zbus. La constante está medida, no elegida a ojo:
> `GetLayout` real con gdbus (incluye el spawn del proceso) dio **10,6-13,4 ms**
> (nm-applet) y **17,5-20,2 ms** (blueman), o sea ~25-50x de margen.
>
> **Verificado end-to-end con `scripts/fake_sni_hang.py`** (un SNI que registra un
> `Menu` y **nunca contesta** `GetLayout`) más un click real inyectado: el log del
> falso da `GETLAYOUT` → **500 ms** → `ACTIVATE`, o sea el timeout disparando y el
> fallback de menú vacío funcionando. Lo importante: **el dock siguió dibujando y
> procesando clicks durante todo el cuelgue** — con el código viejo el primer click
> habría congelado el hilo principal 25 s (los clicks del barrido habrían dejado de
> registrarse; se registraron todos). El camino feliz también: con remmina (app real)
> el menú se abre igual, `getlayout=1ms`, por el hilo nuevo.
>
> **Lo que NO hace**: no hay estado "cargando" en el popup. El menú aparece cuando
> llega la respuesta (~30 ms normal, 500 ms con la app colgada) y si vuelve vacío se
> cae al `Activate` de siempre. Se prefirió eso a un spinner para un caso que, con el
> timeout, dura medio segundo.

**Archivo:** `src/app/dock_popup.rs:75` y `:568`

```rust
let items = crate::tray::fetch_menu(&service, &menu_path, 0);          // :75
…
let submenu = crate::tray::fetch_menu(&service, &menu_path, item.id);  // :568
```

`fetch_menu` (`tray.rs:199`) es `zbus::blocking::Proxy::call("GetLayout", …)` (llamada en
`tray.rs:219`), con el
timeout por defecto de zbus (**25 s**). Se llama desde el handler de click derecho:
icono del tray, click derecho en los widgets Network/Bluetooth
(`open_widget_tray_menu` → `find_menu`, que además hace `GetProperty`), y al abrir
un submenú.

**Impacto:** una app del tray lenta o colgada congela el dock completo hasta 25 s.
Síntoma reportable: *"el dock se trabó al abrir un menú del tray"*.

**Nota:** el repo ya mide esta latencia (hay un `log::debug!` con
`setup=/getlayout=` en `tray.rs:226`): el costo se conoce y se aceptó. El fix cambia
el costo de "bloqueante" a "asíncrono", no lo elimina.

**Fix:** abrir el popup en estado "cargando" y pedir el layout en un hilo, igual que
`send_menu_event` (`tray.rs:274`, ya usa `std::thread::spawn`). El resultado llega
por canal y se aplica con `set_tray_items` (`dock_popup.rs:146`) en el tick del
popup, que ya existe. Mantener el fallback `Activate` si el menú vuelve vacío.
**Ojo:** cubrir también el submenú (:568) y no perder el estado `tray_stack`.

**Verificación:** un SNI falso que no responda `GetLayout`; el dock tiene que seguir
animando y el popup mostrar "cargando" → menú o `Activate`.

### A4 — `curl` de carátula en el hilo principal (AUDIT §4.6)

> **Resuelto (2026-09-20).** `read_media` recibe un flag `allow_network` y el hilo que
> dibuja siempre llama con `false`: sin red, la carátula remota que no está en el caché
> simplemente se resuelve a `None` (el widget se dibuja igual, sin tapa). La descarga la
> hace `warm_media_art()` **en el hilo del watcher de media**, que después manda un
> segundo `MediaChanged` para que la carátula entre en un frame posterior. El único
> otro llamador con red es `read_deferred`, que ya corre en un hilo propio al arrancar.
> De paso se cerró **D3**: el `curl` ahora lleva `--fail` (un 404 ya no se cachea como
> `.jpg`) y `--max-time 1` en vez de 3.
>
> **Verificado con un tarpit local** (un server que acepta la conexión y no responde
> nunca) y un `playerctl` falso que emite metadata cada 300 ms con esa `artUrl`:
> mientras una descarga estaba **en vuelo** (verificado que era el *mismo* `curl`, hijo
> del proceso del dock, antes y después), una notificación pedida por IPC se dibujó
> **0,4 s después**. Es decir: el hilo principal nunca esperó por la red. El
> `10.255.255.1` que sugería esta auditoría no sirve en esta máquina: rebota en 85 ms,
> así que no ejercita nada (de ahí el tarpit local).

**Archivo:** `src/widgets.rs:932-954`

```rust
let status = std::process::Command::new("curl")
    .args(["-s", "-L", "--max-time", "3", "-o"])
    .arg(&path)
    .arg(url)
    .status()          // <-- bloqueante hasta 3 s
    .ok()?;
```

Cadena: watcher `playerctl --follow` (hilo aparte, bien) → `IpcMessage::MediaChanged`
→ `main.rs:436` → `app.refresh_media` → `WidgetSnapshot::refresh_media` →
`read_media` → `resolve_art_path`/`cached_remote_art`.

**Impacto:** hasta 3 s de congelamiento del dock **la primera vez** que suena un tema
con `artUrl` remoto (radio por internet, YouTube vía MPRIS, Spotify con URL http).
Después queda cacheado y es barato. Un freeze de 3 s por tema nuevo.

**Fix:** sacar `cached_remote_art` del camino sincrónico. El `MediaInfo` se dibuja
igual sin carátula; la imagen puede llegar en un frame posterior por canal, como las
miniaturas de fondos. Mínimo aceptable: bajar `--max-time` a 1 y mover la descarga a
un hilo que publique el resultado. (El `--fail` faltante es D3.)

**Verificación:** `artUrl` http apuntando a un host que no responde
(`http://10.255.255.1/art.jpg`): el dock debe seguir respondiendo.

### A6 — Error de protocolo Wayland = fin del proceso (AUDIT §4.8)

> **Resuelto** (§3): el dispatch pasó a `catch_unwind` + `reexec()`, que respawnea
> el binario con el mismo argv (tope de 3 relanzamientos seguidos vía
> `DOCKYRS_REEXEC`). El código de abajo es el **original**.

**Archivo:** `src/main.rs:431`

```rust
loop {
    event_queue.blocking_dispatch(&mut app)?;   // <-- `?` sale de main()
```

`main` devuelve `anyhow::Result<()>`: un error de protocolo (o del socket) termina el
proceso con un log y sin dock. Combinado con la trampa 2 (`attach(NULL)` mata el
cliente por error de protocolo), ésta es la ruta por la que "el dock desapareció".

**Fix mínimo:** loguear y salir explícitamente, para que el fallo sea diagnosticable:

```rust
if let Err(err) = event_queue.blocking_dispatch(&mut app) {
    log::error!("dispatch: {err}");
    return Ok(());
}
```

Opcional (más discutible, evaluar con el usuario): `catch_unwind` + rearmar la
superficie. **No hacerlo sin pedido explícito**: agrega complejidad real.

**Verificación:** por revisión (el `?` sale con el `Err` impreso por anyhow, con
contexto, en vez de un panic sin mensaje). Para probarlo en vivo hay que matar el
compositor: **no** uses `niri msg action quit` en tu sesión real (cierra todo); solo
en una sesión anidada o descartable.

### A7 — `Compositor::detect()` sin memoizar: un `niri msg` por tick (AUDIT §4.2)

> **Mitigado sin memoizar.** `detect()` corta por variable de entorno
> (`HYPRLAND_INSTANCE_SIGNATURE` / `NIRI_SOCKET`, que los dos compositores exportan) y
> el sondeo `niri msg` queda sólo para un arranque sin ninguna de las dos; además los
> workspaces y el layout de teclado salieron del tick de 2 s (llegan por el
> event-stream), así que `detect()` ya no se llama por tick.

**Archivo:** `src/compositor.rs:7-24` (probe en `:13`)

```rust
pub fn detect() -> Self {
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        Self::Hyprland
    } else if std::env::var("NIRI_SOCKET").is_ok() {
        Self::Niri
    } else if std::process::Command::new("niri")
        .args(["msg", "--json", "workspaces"])
        .output()          // <-- sin timeout
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        // ponytail: `niri msg` probea el socket; solo corre al arrancar
        Self::Niri
```

**El comentario es falso.** `detect()` se llama desde `read_kblayout()`
(`widgets.rs:293`, dentro de `refresh_sys`, cada 2 s), `kblayout_next`
(`widgets.rs:325`, en cada click del widget de layout), `read_workspaces()`
(`widgets.rs:620`), `workspace_switch`, `power.rs`, `ipc.rs:279`,
`popup_menu.rs:380` y `desktop.rs:52`. Si el proceso no heredó `NIRI_SOCKET` (lanzado
por ssh, por una unidad systemd, o con entorno limpio — el `setsid` de este repo no lo
garantiza), la rama del `niri msg` corre **en cada llamada**, sin timeout, en el hilo
de UI.

**Impacto:** con `NIRI_SOCKET` ausente, un `fork/exec` de `niri msg` cada 2 s, más
una llamada extra por cada evento de workspace (`read_workspaces`) y por cada cambio
de layout de teclado. Si `niri msg` se traba, el dock se traba con él.

**Fix mínimo:** el resultado no cambia en runtime; cachearlo:

```rust
pub fn detect() -> Self {
    static DETECTED: std::sync::OnceLock<Compositor> = std::sync::OnceLock::new();
    *DETECTED.get_or_init(|| { … } )
}
```

(Si en algún momento se quiere soportar reconexión al compositor, un
`OnceLock<Mutex<Compositor>>` con invalidación; hoy YAGNI.)

**Verificación:** `env -u NIRI_SOCKET -u HYPRLAND_INSTANCE_SIGNATURE strace -f -e trace=execve ./target/release/dockyrs 2>&1 | grep -c 'niri.*workspaces'`
en 10 s: con el fix, 1 (o 0 si el probe falla); sin el fix, ~5.

### B1 — El auto-repeat de Shift+flecha cicla modos en bucle (AUDIT §5.1)

> **Cerrado con medición.** `overlay_cycle_key` + `accion_de_la_banda` se tragan el
> auto-repeat de la flecha que ya cicló (con el overlay abierto; sin overlay la flecha
> sigue su camino, que es lo que espera el panel de ajustes). **Ojo: el bucle NO se
> reproduce en niri 26.04** — con el binario sin el fix y Shift+→ sostenida 2,5 s por
> uinput el log muestra **un** ciclo, no el bucle: el compositor no le manda repeticiones
> al cliente. El guard queda como defensa para los que sí las sintetizan (sctk lo hace si
> recibe `RepeatInfo::Repeat`) y porque cuesta un `Option<Keysym>`.

**Archivo:** `src/app/handlers.rs:320-328`

```rust
// `held_key`; si no, el auto-repeat de la flecha ciclaría un modo por frame. -----
if self.modifiers.shift && matches!(event.keysym, Keysym::Left | Keysym::Right) {
    let dir = if event.keysym == Keysym::Right { 1 } else { -1 };
    if self.cycle_overlay(dir, qh) {
        return;
    }
}
```

**El comentario describe una protección que el código no tiene.** `held_key` se arma
*después* de esta rama, y `cycle_overlay` (`app/mod.rs:315`) no mira `held_key` ni
deduplica por `event.time` (que tampoco se usa en ningún lado). Cada `press_key` con
Shift+←/→ entra a esta rama: el auto-repeat del teclado (tras ~500 ms, ~30/s) cicla
Apps → Clipboard → Wallpapers → Windows en bucle mientras se mantenga la tecla.

**Verificación pendiente (importante):** confirmar con teclado real.
`scripts/pointer.py --shift-arrow` manda pulsaciones discretas, **no** auto-repeat,
así que el bug no se reproduce por script. Es la razón por la que no lo detectó
ningún test.

**Fix:** deduplicar por `event.time`: guardar `last_cycle_time: u32` en `App` y
exigir `event.time != last_cycle_time` antes de ciclar.

**Verificación:** mantener Shift+→ 2 s con el launcher abierto y contar los
`overlay: X -> Y` en el log: debe haber **uno**.

### B2 — Socket IPC en `/tmp` si falta `XDG_RUNTIME_DIR` (AUDIT §5.2)

> **Resuelto.** `socket_path` ya no cae en `/tmp` pelado: cuando falta
> `XDG_RUNTIME_DIR` usa un subdirectorio por uid con 0700 (`/tmp/dockyrs-<uid>`), que
> sigue funcionando pero no lo puede abrir otro usuario.

**Archivo:** `src/ipc.rs:27-33`

```rust
let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
std::path::PathBuf::from(dir).join(if profile.is_empty() { "dockyrs.sock" … })
```

Con systemd/elogind `XDG_RUNTIME_DIR` siempre existe (`/run/user/1000`, modo 0700) y
el socket queda protegido. Si falta (sesión lanzada a mano, contenedor, login sin
`pam_systemd`), el socket se crea en `/tmp/dockyrs.sock` con el modo por defecto
(0777 & ~umask → normalmente 0755): **cualquier usuario local puede conectarse** y
enviar `toggle-search`, `notify<sep>…` (spam de notificaciones), `screenshot-region`,
`toggle-dock-menu`.

**Fix mínimo:** sin `XDG_RUNTIME_DIR`, usar un directorio propio con modo 0700
(`~/.cache/dockyrs` o `temp_dir().join(format!("dockyrs-{}", uid))`) creado con
`.mode(0o700)`; no seguir si el directorio no es del usuario.

**Verificación:** `unset XDG_RUNTIME_DIR; ./target/release/dockyrs &` y
`stat -c %a /tmp/dockyrs.sock` → inaccesible para otros usuarios.

### B3 — Hilo por conexión IPC, sin timeout de lectura (AUDIT §5.3)

> **Resuelto** (no estaba marcado en `AUDIT.md`). `spawn_listener` lanza **un hilo por
> cliente** y `send_message` avisa por stderr cuando no puede conectar o escribir, así
> que un cliente mudo ya no llena la cola de `accept` ni deja muda la IPC entera.
> Verificado abriendo **200 conexiones mudas**: con el dock nuevo el launcher y las
> notificaciones siguen andando. El timeout de lectura que pedía el hallazgo no se
> agregó: con un hilo por cliente, uno trabado cuesta un hilo y nada más.

**Archivo:** `src/ipc.rs:58-61` y `:71`

```rust
for stream in listener.incoming().flatten() {
    handle_client(stream, &tx, &conn, &qh);   // read() bloqueante, sin timeout
}
```

```rust
let mut buf = [0u8; 1024];
let Ok(n) = stream.read(&mut buf) else { return; };
```

Un cliente que conecta y no escribe deja el hilo bloqueado en `read` para siempre.
`incoming()` es secuencial: **una** conexión colgada y el dock no acepta más comandos
IPC (los `--toggle-search` del keybind dejan de funcionar).

**Fix mínimo:** `stream.set_read_timeout(Some(Duration::from_millis(200)))` antes de
leer. No hace falta thread pool ni async.

**Verificación:** dejar un socket conectado y sin escribir
(`python3 -c "import socket;s=socket.socket(socket.AF_UNIX);s.connect('<path>')"`), después
`./target/release/dockyrs --toggle-search` debe seguir funcionando.

### B4 — Watcher de niri: `niri msg` por cada evento de ventana (AUDIT §5.4)

> **Cerrado con medición.** El filtro es por NOMBRE de evento (ya no por substring) y
> `WorkspacesChanged` usa su payload en vez de lanzar `niri msg`. Medido envolviendo a
> `niri` en un script que registra cada spawn del dock: 3 ventanas abriendo y cerrando
> **27 → 14** lecturas, 6 cambios de workspace **5 → 2**, y 0 en reposo. De las 14 que
> quedan, la mitad son el reintento de ráfaga del throttle (`ws_read_pending`, se cobra en
> el tick de 1 s) que existe para no perderse un segundo cambio real dentro de la ventana
de 150 ms.

**Archivo:** `src/ipc.rs:298-308`

```rust
// ponytail: filtrado por substring, sin parsear JSON por evento
if (line.contains("Workspace") || line.contains("workspace"))
    && tx.send(IpcMessage::WorkspacesChanged).is_ok()
```

`line.contains("workspace")` matchea en minúscula **cualquier** evento con
`workspace_id`, y el `event-stream` de niri lo incluye en `WindowOpenedOrChanged`,
`WindowFocusChanged`, `WindowClosed`, etc. Cada uno dispara `WorkspacesChanged` →
`app.refresh_workspaces` → `read_workspaces()` →
**`Command::new("niri").args(["msg","--json","workspaces"])`** (`widgets.rs:627`) →
`sync_widget_bar_len` + `relayout_dock`.

**Impacto:** un proceso `niri msg` + un relayout completo **cada vez que cambiás el
foco de una ventana**. Es el churn más grande del programa y es innecesario: el
indicador de workspaces no cambió.

**Contexto:** el `ponytail:` marca la simplificación como deliberada con techo
conocido. El techo llegó: el costo no es "filtrar de más", es un subproceso por
evento de ventana.

**Fix:** parsear el JSON y filtrar por tipo de evento (una línea por evento;
`serde_json::from_str::<serde_json::Value>(&line)` y mirar la clave de nivel
superior: `WorkspaceActivated`, `WorkspacesChanged`, `WorkspaceActiveWindowChanged`
son los relevantes). Una deserialización por evento es mucho más barata que un
`fork/exec`. Mantener el fallback por substring si el JSON no parsea (versiones
viejas de niri).

**Verificación:** con `RUST_LOG=debug`, focusear/abrir/cerrar ventanas y contar las
líneas `wsflash: refresh before=… after=…`. Test: el filtro extraído a una función
pura (`fn niri_event_is_relevant(line: &str) -> bool`) con 4-5 líneas JSON reales de
`niri msg --json event-stream` como fixture literal, incluidas dos de ventana.

### B5 — Conexión D-Bus del tray cacheada, sin reconexión (AUDIT §5.5)

> **Cerrado.** Tenía tres patas: (1) la conexión cacheada pasa a `CacheConexion`, que se
> invalida donde una llamada falla (`fetch_menu`, `find_menu`, `activate`, `send_menu_event`),
> así la próxima reconecta sola; (2) el hilo del watcher pasa a `correr_watcher_tray`, con lazo
> de reconexión que rehace conexión + nombre + `RegisterStatusNotifierHost` si el bus no está, y
> pide el **relanzado** del dock (`relanzar_por_bus_caido`, la red de seguridad de A6) cuando el
> poll falla 3 veces seguidas, porque las conexiones cacheadas del resto del tray quedaron
> muertas; (3) y ese lazo reintenta la conexión inicial, que es el caso del dock relanzado que
> llega **antes** que el bus nuevo (se me escapó en la primera versión y el tray quedaba muerto
> igual). **Verificado con un bus privado** (`dbus-daemon --address=unix:path=/tmp/dbus-b5.sock`,
> nunca el de la sesión del usuario) y un dock de prueba: se registra como host (`busctl list`),
> se mata el bus → `tray: no pude leer el bus (2)/(3): Broken pipe` → `el bus de sesión se
> reinició: relanzo el dock` → PID nuevo → y al levantar el bus en la misma dirección el
> relanzado **se re-registra**. Guard: `tray::tray_conexion_tests`.

**Archivo:** `src/tray.rs:185-198`

```rust
fn tray_conn() -> Option<&'static Connection> {
    static CONN: std::sync::OnceLock<Option<Connection>> = OnceLock::new();
    CONN.get_or_init(|| match Connection::session() { … }).as_ref()
}
```

El comentario explica bien por qué se cachea (antes cada llamada hacía el handshake
completo). El problema es el otro extremo: si el bus de sesión se reinicia
(`systemctl --user restart dbus`, o el bus se cae), la `Connection` cacheada queda
muerta **para siempre**. Todo lo del tray (`fetch_menu`, `find_menu`, `activate`,
`send_menu_event`) falla en silencio (cadenas `let … && let Ok(…)` que no hacen nada,
más `.ok()?` que devuelve `Vec` vacío) y el tray del dock queda
vacío hasta reiniciar dockyrs.

**Fix mínimo:** guardar la conexión en un `Mutex<Option<Connection>>` y, ante el
primer error de una llamada, descartarla y reintentar una vez con
`Connection::session()`.

**Verificación:** con el dock corriendo, reiniciar el bus de sesión y después hacer
click derecho en un icono del tray: debe volver a funcionar sin reiniciar el dock.
Antes del fix, no.

### B7 — `configure` no reconcilia el tamaño con lo dibujado (AUDIT §5.7)

> **Cerrado.** `accion_del_configure()` (pura, con guard) decide entre adoptar el
> primer configure, **re-aplicar el reparto una vez** cuando el tamaño del compositor no es
> el que pedimos, y dibujar como vino si el compositor insiste (sin girar en bucle de
> `set_size` → `configure` → `set_size`). Verificado en vivo: en uso normal la
> reconciliación no se dispara ni una vez (0 en el log) y los `configure` siguen
> coincidiendo. Guard: `app::handlers::configure_accion_tests`.

**Archivo:** `src/app/handlers.rs:150-170`

```rust
log::debug!("dock: configure new_size={:?} base={:?} applied={:?}", …);
if self.first_configure { self.first_configure = false; }
self.draw(qh);
```

El repo conoce el peligro —está en el comentario: *"si no coinciden, el buffer se
estira y los clicks caen corridos"*— pero sólo lo **loguea**. Si el compositor impone
un `new_size` distinto del `base_size()` (layer-shell permite que el compositor
ajuste el tamaño), el buffer se estira y los hit tests —que trabajan en coordenadas
lógicas del layout— quedan corridos. Mismo síntoma que el bug histórico de
`hit_scale`, por otra causa.

**Fix mínimo:** si `configure.new_size != (0,0)` y difiere de `applied_size`,
loguearlo como `warn` y forzar `relayout_dock`. Como mínimo, dejar de ser silencioso.

**Verificación:** parcial — en el log, `configure new_size` y `base` deben coincidir
en operación normal; si alguna vez no coinciden, el fix tiene que actuar.

### B8 — Carátula: metadata MPRIS no confiable (AUDIT §5.8)

> **Cerrado.** El `curl` de la carátula lleva `--max-filesize` de 8 MB y, después, un
> chequeo del tamaño real del archivo (el flag de curl sólo actúa si el servidor manda
> `Content-Length`): si se pasó, se borra y no se usa. El resto del hallazgo ya estaba
> cubierto: `resolve_art_path` sólo acepta http(s) o `file://` (el resto se ignora sin
> salir a la red) y el nombre en el caché es un HASH de la URL, así que no puede escribir
> fuera de `~/.cache/dockyrs/art`. Guard: `widgets::art_url_tests`.

**Archivo:** `src/widgets.rs:896-954`

Cualquier aplicación con MPRIS (o cualquier proceso que pueda publicar un bus name
`org.mpris.MediaPlayer2.*`) puede hacer que dockyrs:

1. haga una request HTTP saliente a una URL arbitraria (fuga de que hay una sesión
   con este dock a un host atacante), y
2. escriba el cuerpo de la respuesta en `~/.cache/dockyrs/art/` y después lo
   **decodifique** (`image`, superficie de ataque de parsers).

**Severidad real: baja** (mismo usuario, mismo privilegio, no hay escalada), pero es
la única entrada de datos remotos no confiables del programa y merece constar. Lo
bueno: `--max-time 3`, extensión derivada de la URL (no del contenido → sin path
traversal) y hash de la URL como nombre (sin path injection).

**Fix mínimo:** aceptar sólo `http`/`https`, ignorar `file://` inexistente, y limitar
el tamaño descargado (`--max-filesize`). El `--fail` de este hallazgo ya está puesto;
lo que falta es el tope.

**Verificación:** unit tests de `resolve_art_path`/`youtube_thumbnail_url` con URLs
hostiles (`file:///etc/passwd`, `http://x/../../y.png`, URLs de 10 KB,
`http://x/%`).

### B10 — Elegir fuente pisa la config del usuario sin escritura atómica (AUDIT §5.16)

> **Resuelto.** Las escrituras de `gtk-3.0/settings.ini`, `gtk-4.0/settings.ini`,
> `kdeglobals` y `kitty.conf` pasan por `escribir_atomico()`: `.tmp` con el pid en el
> mismo directorio + `rename`, con los permisos del original copiados y `warn` si falla.

**Archivo:** `src/app/fonts.rs:63` y `:110`, invocados desde
`app/dock_menu_input.rs:282-284`

```rust
let _ = std::fs::write(path, lines.join("\n") + "\n");
```

Las tres funciones (`apply_system_gtk_font`, `apply_system_qt_font`,
`apply_kitty_font`) reescriben **archivos que no son del dock**:

- `~/.config/gtk-3.0/settings.ini` y `~/.config/gtk-4.0/settings.ini`
- `~/.config/kdeglobals`
- `~/.config/kitty/kitty.conf`

`fs::write` **trunca y después escribe**. Si el proceso muere en el medio (o se corta
la luz), el usuario se queda con su config de GTK/Qt/terminal truncada o vacía — y el
contenido se armó desde `read_to_string(...).unwrap_or_default()`, así que un archivo
ilegible se pisa con una versión reconstruida desde cero. Errores silenciados con
`let _ =`.

Es la única escritura del repo que toca config ajena, y es justo la que no sigue el
patrón correcto que el repo **ya tiene** en `clipboard/storage.rs:60-80` (tmp +
rename + modo 0600).

**Fix mínimo:** escribir a un temporal en el mismo directorio y renombrar:

```rust
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("dockyrs-tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}
```

Y conservar el original si el parseo falla (no reconstruir desde cero cuando
`read_to_string` devuelve `Err` por permisos).

**Verificación:** test con un archivo temporal que simule el fallo (escribir y
renombrar deja el original intacto); manual: elegir una fuente y verificar que
`kitty.conf` conserva el resto de las líneas. `git diff --no-index` contra una copia
previa sirve de evidencia.

### B11 — `dockyrs-notifyd` ignora el perfil: no entrega notificaciones (AUDIT §5.17)

> **Cerrado.** El notifyd acepta `--profile` (y `--profile=<x>`) y, si no hay perfil ni
> `dockyrs.sock`, le habla a **todas** las instancias que estén escuchando (con `warn` si
> no hay ninguna): antes mandaba el aviso a `dockyrs.sock`, donde con una instancia por
> monitor no escucha nadie, y se perdía en silencio. Verificado en vivo: con
> `--profile pruebas` el log dice `no hay dock escuchando en …/dockyrs-pruebas.sock` (antes
> habría ido al default). Guards: `destinos_tests`.

**Archivo:** `src/bin/dockyrs-notifyd.rs:13-16`

```rust
fn socket_path() -> std::path::PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    std::path::PathBuf::from(dir).join("dockyrs.sock")
}
```

El socket está **hardcodeado sin perfil**, mientras que el dock lo deriva del perfil
(`ipc.rs:26-33`: `dockyrs-<perfil>.sock`). El README y `niri-config.kdl.example`
documentan explícitamente el setup multi-monitor con `--profile dp` / `--profile hdmi`:
con ese setup el notifyd escribe en `dockyrs.sock`, que **no existe** (o pertenece a la
instancia sin perfil, que puede no estar corriendo). Resultado: las notificaciones del
sistema se pierden en silencio y sin log.

Además `forward()` (línea 18-26) arma `notify<US>título<cuerpo>` y lo manda de una:
el lado receptor corta a 1024 bytes (C6), así que un cuerpo real de una notificación de
chat (que suele pasar de 1 KB) **llega truncado**. Acá el truncado no es un caso de
borde del IPC: es el camino normal, porque `dockyrs-notifyd` es el productor.

**Fix:** aceptar `--profile` (mismo `parse_cli` que `main.rs:50-71`) y
derivar el nombre del socket igual que `ipc::socket_path` — mejor todavía: exponer
`ipc::socket_path` como `pub` y usarlo desde el binario, para que no haya dos
implementaciones de lo mismo. Y que `forward` registre un warn si el socket no está.

**Verificación:** `dockyrs-notifyd --profile dp` + `notify-send hola` con el dock
corriendo con `--profile dp`: tiene que aparecer. Antes del fix, no aparece nunca.

### B12 — Pegar del portapapeles lee sin tope (AUDIT §5.18)

> **Cerrado.** La lectura del pipe pasa por `leer_pegado()`, con tope de 64 KB y `warn` al
> cortar (el pegado se usa para 6 caracteres de hex o 24 de nombre). Era el único hallazgo
> de la lista que podía tumbar el proceso por una entrada de datos. Guard:
> `clipboard::paste::paste_tope_tests`.

**Archivo:** `src/clipboard/paste.rs:52-62`

```rust
std::thread::spawn(move || {
    use std::io::Read;
    let mut file = std::fs::File::from(r);
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_ok() {
```

`read_to_end` sobre el pipe del portapapeles **sin límite**: si el dueño del
portapapeles ofrece un `text/plain` de varios GB (un log, un dump — cualquier app
puede setearlo), el `Vec` crece hasta agotar la memoria. Es el camino de
"pegar en el campo de hex o de nombre del panel" (un click del usuario), así que no es
remoto, pero el límite ya existe en el resto del módulo (`TEXT_LIMIT = 256 * 1024`,
`clipboard/mod.rs:22`) y acá no se aplica. Ojo: `TEXT_LIMIT` es privado del módulo
`clipboard`, así que el fix usa `super::TEXT_LIMIT`:

**Fix mínimo:**

```rust
let mut file = std::fs::File::from(r);
let mut buf = Vec::new();
if file.take(super::TEXT_LIMIT as u64).read_to_end(&mut buf).is_ok() {
```

**Verificación:** test que sirve un pipe con 1 MB y verifica que `buf` queda en
`TEXT_LIMIT`. La sanitización posterior ya está bien (`apply_pending_paste` filtra a
hexdigits / no-control con `take(6)` / `take(24)`): no la toques.

### C2 — `DockMenuMode.closing` es código muerto (AUDIT §6.2)

> **Cerrado.** `DockMenuMode.closing` nunca se ponía en `true`, así que la rama que lo leía
> en `tick_dock_menu_frame` era código muerto (el cierre va por `close_dock_menu`). Se
> borraron el campo y la rama, y quedó anotado que la ruta de cierre es UNA. Verificado en
> vivo: el panel de ajustes abre (555x584) y cierra (26x584) igual que antes.

El campo se declara en `src/app/mod.rs:199` (dentro de `DockMenuMode`, `:190`) y se
inicializa en `src/app/dock_menu.rs:130`; `dock_menu.rs:401` lo lee. Nunca se pone en
`true`: `close_dock_menu` (`dock_menu.rs:184`) anula el modo de una, así que la rama
`if closing && anim <= 0.0` (`dock_menu.rs:405`) es inalcanzable.

**Precisión sobre `AGENTS.md`:** dice que el campo muerto está en *"`WsFlashMode`/
`DockMenuMode`"*. En `WsFlashMode` **sí** se usa (`ws_flash.rs:81` lo pone en `true`,
`:205` lo lee). El muerto es sólo `DockMenuMode.closing`. Corregir el texto junto con
el código: borrar el campo y la rama inalcanzable.

### C3 — `Config::load` descarta apps y ajustes si el JSON no parsea (AUDIT §6.3)

> **Resuelto.** Un config que existe pero no parsea se respalda al lado
> (`config.json.bak-<ts>`) y se loguea `error`, en vez de quedar a merced del primer
> `save()`. Verificado en vivo con un `--profile` descartable.

`src/config.rs:283-289`

```rust
Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|err| {
    log::warn!("failed to parse config at {path:?}, using defaults: {err}");
    Config::default()
}),
```

`DockSettings` tiene `#[serde(default)]` (`config.rs:124`), así que agregar campos
nuevos **no** rompe configs viejas (bien). El problema es un JSON corrupto o un
`PinnedApp` con un campo faltante: se descartan silenciosamente **todos** los apps
pinned y todos los ajustes, y el próximo `save()` pisa el archivo. El usuario ve el
dock "reseteado" y perdió su configuración.

**Fix mínimo:** no pisar el archivo. Guardar el ilegible como `config.json.bak-<ts>`
antes de caer a los defaults, y loguear `error` en vez de `warn`.

### C4 — HECHO: AGENTS trampa 1 ya corregida en el árbol (AUDIT §6.4)

> **No había nada que implementar:** la trampa 1 ya decía "superficie compartida +
> superficies propias del popup y el selector", que era el estado del árbol. Se conserva
> el texto original abajo como trazabilidad.

La trampa 1 ya dice "superficie compartida + superficies propias del popup y el
selector". Se conserva el texto original abajo como trazabilidad, sin nada que implementar.

`AGENTS.md` (trampa 1): *"Una sola superficie layer para todo. El dock y cada modo
(panel de ajustes, OSD, HUD, popups) usan `self.layer`."*

Realidad (`rg -n create_layer_surface src/`): hay **4**: `main.rs:94` (dock + todos los
modos), `app/popup_menu.rs:16` (menú de click derecho sobre un icono),
`app/dock_popup.rs:200` (menús del tray y **panel de volumen**), `screenshot/mod.rs:79`
(selector de región). La propia trampa 1 se contradice más abajo, cuando habla de la
superficie del popup y su input region.

**No es un bug de código**: es documentación que va a extraviar al próximo agente.
Corregirla a "una superficie compartida para el dock y los modos que lo acompañan
(menú, OSD, HUD, launcher, portapapeles, fondos) + superficies propias para el popup
del tray y el selector de screenshot".

### C5 — Huecos de tests (AUDIT §6.5)

> **Cerrado.** Lo que quedaba eran dos hit tests de la familia de la trampa 10: `Dock::icon_at`
> (el centro que da `layout()` devuelve su índice, y el hueco entre íconos no es de ninguno) y
> `Dock::drag_to` (reordena al slot cuyo centro está más cerca del puntero y `dragging_index`
> sigue al ícono movido). Los centros salen de `rest_centers()`, la misma cuenta que usa el
> reordenamiento: la primera versión del test inventaba coordenadas y pasaba por casualidad en
> una dirección y fallaba en la otra. Guards: `dock::icon_at_y_drag_tests`. El resto de los
> huecos se habían cubierto en la Ronda 3 (timers, `wallpaper_program`, IPC, workspaces,
> `percent_decode`): los tests pasaron de 37 a **175** (165 + 3 del notifyd + 7 del raster).

> **Casi cerrado.** De la lista de abajo están cubiertos: 1 (el reparto de `workspaces.rs`
> tiene `workspace_hit_tests`), 2 (`percent_decode_tests` y, para `resolve_art_path`, el
> guard de B8), 3 (`ipc.rs` tiene 3 tests, incluido el filtro del `event-stream`), 4 (el
> filtro de niri, en el mismo test) y 5 (`timers_tests`, con el borde de plazo 0). También
> `wallpaper_program()` (`wallpaper_program_tests`). **Lo que queda:** los tests de
> `Dock::drag_to`/`icon_at` (hacen falta fixtures con iconos) y, si algún día se quiere
> más, `tray_geometry` con `widget_scale ≠ 1` propio. Con todo esto los tests pasaron de 37
> a **172** (162 en el paquete raíz, 3 del notifyd y 7 del crate del raster).

37 tests cuando se escribió esto, y los que hay son buenos: geometría y hit tests con
escala
(`hit_layout_tests`, `tray_hit_tests`, `tabs_tests`), parseo de JSON de terceros
(`volume_panel_tests`, `usage_tests`, `desktop::entry_tests`), lógica con signos
(`wheel_tests`, `battery_tone_tests`), máquina de estados del popup
(`popup_dismiss_tests`), orden del overlay (`overlay_tabs_tests`). Casi todos tienen un
bug real detrás: es la mejor parte del repo.

Sin cobertura, ordenado por riesgo:

1. **`render/workspaces.rs`**: ya cubierto por `workspace_hit_tests` (era el hueco más
   caro, el del bug D1).
2. `resolve_art_path` (`widgets.rs`): B8. (`percent_decode` ya tiene
   `percent_decode_tests`.)
3. `ipc.rs`: parseo de mensajes (separador `\u{1f}`, truncado de 1024 bytes) y path
   del socket.
4. El filtro del `event-stream` de niri (B4): extraerlo a función pura y testearlo con
   líneas JSON reales.
5. Timers (`spawn_osd_timer`, `spawn_notification_timer`, `spawn_autohide_timer`,
   `spawn_marquee_ticker`): todos comparten el patrón `recv` → `recv_timeout(dur)` →
   `flag` y ninguno tiene test. El borde `dur == 0` es real.
6. `wallpaper::wallpaper_program()` (búsqueda recursiva en `.kdl`).
7. `tray_geometry`/`tray_icon_hit` con `widget_scale ≠ 1` (hoy sólo se testea
   `nearest_tray_index`).
8. `Dock::drag_to`/`end_drag`/`icon_at` a nivel de `Dock`.

### C6 — `notify` por IPC truncado a 1024 bytes (AUDIT §6.6)

> **Resuelto.** La lectura del comando sale a `leer_comando()`, con tope de 16 KB y un
> `warn` cuando hay que cortar (antes: buffer fijo de 1024 y corte silencioso).

`ipc.rs:71-80`: `read(&mut buf)` con `buf = [0u8; 1024]`, y `notify` mete título +
cuerpo separados por `\u{1f}` en ese buffer. Un cuerpo largo se corta **en silencio**
(no hay bucle de lectura ni longitudes). **Fix mínimo:** leer hasta EOF acumulando en
un `Vec`, o truncar con `…` visible en vez de cortar en seco.

### C7 — `dockyrs-notifyd` detiene los otros daemons de notificaciones (AUDIT §6.12)

> **Cerrado.** El notifyd ya no apaga dunst/mako/swaync/fnott/wired cuando el nombre está
> tomado: avisa quién lo tiene (con el pid) y sale con código 1. Apagar la app del usuario
> por detrás no es decisión de ese binario. Verificado en vivo: el notifyd nuevo tomó el
> nombre y un `notify-send` real llegó al dock (toast), sin un solo `pkill`.

**Archivo:** `src/bin/dockyrs-notifyd.rs:68-76`

```rust
let flags = RequestNameFlags::ReplaceExisting | RequestNameFlags::AllowReplacement;
if conn.request_name_with_flags("org.freedesktop.Notifications", flags)? != RequestNameReply::PrimaryOwner {
    let hush = || std::process::Stdio::null();
    for daemon in ["dunst", "mako", "swaync", "fnott", "wired"] {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "stop", &format!("{daemon}.service")])
            …
        let _ = std::process::Command::new("pkill").args(["-x", daemon])…
```

Si no obtiene el nombre, el daemon **mata** dunst/mako/swaync/fnott/wired por systemd
y por `pkill`, en cada arranque. `AGENTS.md` avisa "no arranques `dockyrs-notifyd` si
ya hay otro daemon", pero el binario ya lo resuelve solo y por la fuerza: matar el
daemon que el usuario eligió es una decisión que no le corresponde al dock. Si además
el usuario lo tiene como dependencia de otra cosa (p. ej. swaync para el centro de
notificaciones), le rompe algo más.

**Fix mínimo:** no matar nada. Si el nombre está tomado, loguear `warn` con quién lo
tiene (`GetNameOwner`) y salir con código de error, dejando que decida el usuario. Si
se quiere conservar el comportamiento, hacerlo opt-in por env
(`DOCKYRS_NOTIFYD_TAKE_OVER=1`).

**Verificación:** con dunst corriendo, arrancar `dockyrs-notifyd` y comprobar con
`systemctl --user is-active dunst` que sigue activo.

### C8 — HECHO: AGENTS trampa 2 ya corregida en el árbol (AUDIT §6.13)

> **No había nada que implementar:** la trampa 2 ya estaba acotada a la superficie
> compartida del dock (el popup y el selector de screenshot sí se desmapean a propósito).
> Se conserva el texto original abajo como trazabilidad.

La trampa 2 ya dice "nunca desmapear **la superficie compartida del dock**" y
aclara que popup y selector sí se desmapean. Se conserva el texto original abajo
como trazabilidad, sin nada que implementar.

`AGENTS.md` (trampa 2) dice, sin matices: *"Nunca desmapear la superficie.
`attach(NULL)` hace que niri resetee el tamaño de la layer a 0 y el cliente muere por
error de protocolo."*

Pero `attach(None, 0, 0)` se usa en tres lugares y está bien:

```text
src/screenshot/region.rs:175   selector de región (al cerrar)
src/app/dock_popup.rs:256      popup del tray (al cerrar)
src/app/dock_popup.rs:418      popup del tray (al cambiar de submenú)
```

Ninguno es `self.layer` (la superficie compartida del dock), que efectivamente nunca
lo hace. La trampa aplica a **la superficie compartida**, que es la única con
`set_size(w, h)` propio y `set_exclusive_zone(-1)`; las auxiliares son descartables y
se re-crean al abrirse (`dock_popup.rs:200` crea una capa nueva por popup), así que
desmapearlas antes de soltarlas es intencional.

**No es un bug de código**: es la documentación enumerando un invariante más ancho de
lo que es. Corregir la trampa 2 a "nunca desmapear **la superficie compartida del
dock**", y aclarar que popup y selector sí se desmapean a propósito.

**Verificación:** `rg -n 'attach\(None' src/` → debe devolver sólo los tres sitios de
arriba, nunca `self.layer`.

### D1 — `workspace_dot_hit` reparte con `1.0`: clicks muertos (AUDIT §4.3)

> **Resuelto** (§3): el reparto salió a `workspaces::workspace_slot_at()`, que lee
> la escala de `hit_scale(settings)`. El HUD de workspaces ya no tiene geometría
> propia (`ws_flash_dot_hit` se borró): su superficie mide y se ancla como la del
> dock y el hit test es `workspace_dot_hit`. Guard:
> `render::workspaces::workspace_hit_tests::el_hit_test_toma_la_escala_de_las_settings`
> — verificado revirtiendo el `1.0` a mano: falla, mientras los otros 3 tests del
> módulo pasan (por eso el guard tiene que mirar el call site y no el helper).

**Archivo:** `src/render/workspaces.rs:126-155` (`first_center(…, 1.0)` en `:148`)

```rust
pub fn workspace_dot_hit(dock: &Dock, widgets: &WidgetSnapshot, tray_count: usize, x: f64, y: f64) -> Option<i32> {
    …
    let rects = hit_layout(dock, widgets, tray_count);      // <- escalado por widget_scale
    let (bar_len, bar_start, main) = if is_vertical { (r.h, r.y, y as f32) } else { (r.w, r.x, x as f32) };
    let count = slot_count(workspaces);
    let first = first_center(count, bar_len, bar_start, 1.0);   // <- AQUÍ: sin escala
    let half = WS_SLOT / 2.0;                                   // <- y aquí
    for (i, ws) in workspaces.iter().enumerate() {
        if (main - (first + i as f32 * WS_SLOT)).abs() <= half {  // <- y aquí
```

El dibujo sí lo hace bien (`render/workspaces.rs:52-80`):

```rust
let slot = WS_SLOT * render_scale;          // render_scale = output_scale * widget_scale
let active = WS_ACTIVE * render_scale;
let first = first_center(count, bar_len, bar_start, render_scale);
```

**Es la trampa 10 otra vez, en el sexto hit test.** `hit_layout()` devuelve rects
**escalados**, así que el `bar_start`/`bar_len` del hit test ya están escalados,
pero su geometría interna (`first_center` con `1.0`, el paso `WS_SLOT`, la
tolerancia `WS_SLOT/2`) no. El error por punto es
`(ω−1)·WS_SLOT·((n−1)/2 − i)` con `ω = widget_scale`:

| n | ω = 1.2166 | error en el punto 0 | tolerancia |
| --- | --- | --- | --- |
| 3 | 5.2 px | funciona (pasa desapercibido) | 12 |
| 6 | 13.0 px | **falla** | 12 |
| 8 | 18.2 px | **falla** (y también el último) | 12 |

Con `widget_scale = 1.0` (el default) es invisible; con `ω > 1` y ≥6 workspaces, el
click sobre los puntos de los extremos **no hace nada** (`workspace_dot_hit`
devuelve `None`) o cae en el workspace vecino. La constante `SCALE_DEL_BUG =
1.2166064` de `hit_layout_tests` es la escala del setup real, así que esto es el
entorno del usuario, no un caso teórico.

**`AGENTS.md` lo da por arreglado.** El "Qué se hizo" lista
`workspace_dot_hit` entre los cinco hit tests que pasan por `render::hit_scale()`.
Pasa por `hit_layout()` para el rect, pero **no aplica la escala a su propia
geometría**: el fix quedó a medias. Corregir también ese texto.

**Fix:** usar la escala de hit (ya existe el helper):

```rust
let sc = hit_scale(&dock.config.settings);
let first = first_center(count, bar_len, bar_start, sc);
let half = WS_SLOT / 2.0 * sc;
for (i, ws) in workspaces.iter().enumerate() {
    if (main - (first + i as f32 * WS_SLOT * sc)).abs() <= half {
```

El HUD de workspaces ya no dibuja con un panel propio: su superficie es la del dock
(mismo tamaño y anclaje) y el indicador sale del mismo `hit_layout`, así que no hay
un segundo reparto que ajustar (`ws_flash_dot_hit` se borró). Ver el bullet del HUD
de alineación en `AGENTS.md`.

**Verificación:** `#[cfg(test)] mod ws_hit_tests` en `render/workspaces.rs` con
`widget_scale = 1.2166064` y n = 6 y 8: el centro dibujado de cada punto
(`first_center(…, sc) + i·WS_SLOT·sc`) tiene que devolver `workspaces[i].id`. Con el
bug, i=0 falla. Revertir `hit_scale` a `1.0` debe romper el test (ese es el guard).

### D2 — `percent_decode` paniquea con `%` seguido de multibyte (AUDIT §5.9)

> **Resuelto.** `percent_decode` decodifica sobre **bytes** (junta un `Vec<u8>` y cierra
> con `from_utf8_lossy`), así que un `%` seguido de un char multibyte ya no corta el char
> al medio. Guard: `widgets::percent_decode_tests` (`percent_decode("a%€")` → `a%€`,
> además del caso `%` al final del string).

**Archivo:** `src/widgets.rs:906-923`

```rust
if bytes[i] == b'%'
    && i + 2 < bytes.len()
    && let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16)
```

`s[i + 1..i + 3]` es slicing por **bytes** de un `&str`: si el carácter tras `%` es
multibyte, el índice final no cae en frontera de char y Rust **paniquea**
(`byte index is not a char boundary`). Reproducción: `"a%€"` → bytes
`61 25 E2 82 AC`, i=1 cumple `i+2 < len`, y `s[2..4]` corta el `€` por la mitad.

El input es externo: `resolve_art_path` aplica `percent_decode` a cualquier
`file://…` que reporte un reproductor MPRIS. Un panic en el hilo del event loop mata
el dock (ver A5).

**Fix mínimo:** usar `get`, que devuelve `None` en vez de paniquear:

```rust
if bytes[i] == b'%'
    && let Some(hex) = s.get(i + 1..i + 3)
    && let Ok(byte) = u8::from_str_radix(hex, 16)
```

**Verificación:** `assert_eq!(percent_decode("a%€"), "a%€")` (hoy paniquea) y
`assert_eq!(percent_decode("%20"), " ")` (no romper el caso válido).

### D3 — `curl` sin `--fail`: un 404 se cachea como carátula válida (AUDIT §5.10)

> **Resuelto.** El `curl` de la carátula lleva `--fail` y `--max-time 1` (antes 3), así
> que un 404 no se guarda como `.jpg` ni se reintenta; además la descarga salió del hilo
> que dibuja (ver A4).

**Archivo:** `src/widgets.rs:942-953`

```rust
let status = std::process::Command::new("curl")
    .args(["-s", "-L", "--max-time", "3", "-o"])
    …
if !status.success() || std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) == 0 {
    let _ = std::fs::remove_file(&path);
    return None;
}
```

Sin `--fail`, `curl` devuelve exit code 0 en un 404/403 y **escribe el cuerpo del
error** (normalmente HTML) en el archivo. El chequeo de "success + tamaño > 0" lo da
por buena carátula, la guarda como `.jpg`, y a partir de ahí `path.exists()`
short-circuitea: **nunca reintenta** y la carátula no aparece jamás para ese tema.
Además contamina `~/.cache/dockyrs/art`.

**Fix:** agregar `--fail` a los args (y verificar el tipo con `image::guess_format`
si se quiere ser estricto).

**Verificación:** `curl -s -L --fail --max-time 3 -o /tmp/x.jpg https://i.ytimg.com/vi/NOEXISTE000/hqdefault.jpg; echo $?`
→ 22 con `--fail` (y sin escribir el archivo).

### D4 — La caché de carátulas en disco crece sin tope (AUDIT §5.11)

> **Resuelto.** `prune_art_cache()` con tope de 200 archivos y la misma política que la
> caché de miniaturas (se poda al bajar una carátula nueva, por `mtime`, y baja a la
> mitad). Guard: `widgets::art_cache_tests`.

**Archivo:** `src/widgets.rs:925-930` (`art_cache_dir`; `cached_remote_art` en `:932-954`)

```rust
fn art_cache_dir() -> PathBuf {
    let mut dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    dir.push("dockyrs"); dir.push("art"); dir
}
```

Cada URL distinta (cada video de YouTube escuchado, cada tema con `artUrl` remoto)
deja un `.jpg` **permanente**. Nada purga por cantidad ni por antigüedad: es la única
caché del proyecto sin tope (`TextCache`/`IconCache` tienen `CACHE_CAP`).

**Fix:** antes de insertar, si `read_dir(dir).count() > N` (p. ej. 200), borrar los
más viejos por `metadata.modified()`.

**Verificación:** test con `dir` temporal: insertar N+1 entradas y afirmar que el
directorio queda ≤ N.

### D5 — El `Watcher` del tray no purga items muertos (AUDIT §5.12)

> **Resuelto.** El `Watcher` (que sirve la interfaz) y el hilo del poll comparten la
> lista de items, y el poll purga por `NameHasOwner` antes de resolver (también saca
> repetidos; ante error de D-Bus asume VIVO, para no borrar un ícono que existe).
> Verificado en vivo con `scripts/fake_sni_hang.py`: la lista pasa de 3 a 4 items al
> registrarse y vuelve a 3 al morir, con `tray: purgo 1 item(s) de servicios que ya no
> estan` en el log. Guard: `tray::tray_purga_tests`.

**Archivo:** `src/tray.rs:77` (registro) y `:350-371` (loop de polling)

```rust
fn register_status_notifier_item(&self, service: &str, …) {
    …
    if !items.iter().any(|s| s == &entry) { items.push(entry); }
}
```

```rust
loop {
    let raw: Vec<String> = watcher_proxy…get_property("RegisteredStatusNotifierItems")…;
    let icons: Vec<TrayIcon> = raw.iter().filter_map(|raw_svc| resolve_item(&conn, raw_svc))…
    …
    std::thread::sleep(Duration::from_millis(2000));
}
```

`Watcher.items` **sólo crece**: nada lo limpia cuando un cliente muere (no hay
`NameOwnerChanged`). Como el poll itera **todos** los registrados cada 2 s y
`resolve_item` hace un `GetAll` por D-Bus, el costo por poll crece con el uptime: cada
app de tray relanzada deja basura que se sigue consultando (y fallando) para siempre.
Encima `fingerprint_icons` hashea el `pixmap.data()` completo de todos los iconos en
cada poll.

**Fix mínimo:** antes de resolver, filtrar con `conn.name_has_owner(service)`; y
purgar de `Watcher.items` los que no tienen owner (o suscribirse a
`NameOwnerChanged`). El poll ya está en su propio hilo y eso está bien; el problema es
la lista que no se limpia.

**Verificación:** test que registra dos items, simula la caída de uno (o llama a un
`prune()` nuevo) y verifica que `registered_status_notifier_items()` no crece.
En runtime: `busctl --user get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher RegisteredStatusNotifierItems`
tras relanzar la misma app de tray 3 veces.

### D6 — `draw_text_clipped` aloca un `Pixmap` por frame (AUDIT §5.13)

> **Resuelto.** `con_scratch`: un Pixmap por hilo que se limpia y se reusa, y sólo se
> re-aloca cuando cambia el tamaño (en régimen estable el título mide siempre lo mismo).
> Guard: `render::media::clip_scratch_tests`, verificado revirtiendo la limpieza (falla
> con 12 píxeles de tinta vieja). **No se pudo verificar en vivo** porque esta config no
> tiene el widget `Media` colocado: es el mismo dibujo de antes, sin el `Pixmap::new` por
> frame.

**Archivo:** `src/render/media.rs:8-20` y `:23-42`

```rust
let avail_i = avail_w.round().max(1.0) as u32;
let Some(mut clip) = Pixmap::new(avail_i, glyphs.height()) else { return; };
tile_text(&mut clip, glyphs, avail_w, offset);
```

El widget Media se redibuja en cada frame del marquee (`MARQUEE_TICK_MS = 33`,
~30 fps) y cada llamada crea **y zero-llena** un buffer nuevo que se descarta
inmediatamente. Es la allocation más caliente del frame, y hay dos variantes
(normal y rotada) con el mismo patrón.

**Fix:** guardar un `Pixmap` reutilizable en `MarqueeState` (o `TextCache`) y
recrearlo sólo cuando cambien `w`/`h` (`Pixmap` no tiene `resize`).

**Verificación:** `heaptrack` o `perf stat -e page-faults` con un título largo en
reproducción; la bajada de allocations es la señal. No hay test unitario razonable.

### D7 — `draw_widgets` reparte el layout 2-3 veces por frame (AUDIT §5.14)

> **Resuelto.** El reparto se calcula UNA vez por frame y de ahí salen el dibujo, los
> separadores entre zonas y la píldora del SSID (antes 2-3 repartos de todos los widgets
> por frame). `WidgetRect` pasa a `Copy` para pasarlo por valor.

**Archivo:** `src/render/layout.rs:319`, `:497` y `:584`

```rust
for r in layout_widgets(s, widgets, tray_count, is_vertical, w, h, render_scale) {   // :319
…
let rects = layout_widgets(&dock.config.settings, widgets, tray_count, …);           // :497 (separadores)
…
let Some(rect) = layout_widgets(&dock.config.settings, widgets, tray_count, …)       // :584 (hover de Network)
```

Cada `layout_widgets` aloca `members_of` + `lens_of` por zona y recalcula
`widget_natural_len` (con formato de strings) de todos los widgets. El camino de los
separadores lo repite entero, y el hover del pill de Network lo repite por tercera
vez.

**Fix:** calcular `let rects = layout_widgets(...)` una vez en `draw_widgets`,
dibujar desde ahí y pasar `&rects` a los separadores y al pill de hover.

**Verificación:** `cargo clippy --release --all-targets` limpio + (opcional) un
contador `#[cfg(test)]` de llamadas a `layout_widgets` por frame.

### D8 — Etiqueta de RAM duplicada entre reparto y dibujo (trampa 12) (AUDIT §5.15)

> **Resuelto.** `render::ram_label()` es la única definición del texto y la llaman
> `len_ram` (la medida) y `draw_ram_widget` (el dibujo): era el caso vivo de la trampa 12
> que seguía abierto.

**Archivo:** `src/render/layout.rs:240` vs `src/render/cpu_ram.rs:200`

```rust
Some((used, total)) => format!("{:.1} / {:.0} GB", used, total),   // layout.rs:240 (mide)
Some((used, total)) => format!("{:.1} / {:.0} GB", used, total),   // cpu_ram.rs:200 (dibuja)
```

El ancho reservado y el texto dibujado deben salir de **una** función (mismo criterio
que `volume_content_len`/`VOLUME_ICON_*`, documentado en la trampa 1 del "Qué se
hizo"). Hoy coinciden sólo porque el `format!` está copiado: cambiar el formato en un
lado deja el texto pisando el borde.

**Fix:** `pub(super) fn ram_label(widgets: &WidgetSnapshot) -> Option<String>` en
`layout.rs`, usada en ambos.

**Verificación:** test que compare `widget_natural_len(Ram, …)` contra el ancho real
del label rasterizado para `ram_gb = (12.34, 32.0)` y `(9.9, 8.0)`.

### D9 — `nearest_tray_index` resta `count - 1` (AUDIT §6.7)

> **Resuelto.** `nearest_tray_index` usa `saturating_sub(1)`: con el tray vacío ya no
> hay underflow.

`src/render/tray.rs:7-9`

```rust
((rel.max(0.0) / per).floor() as usize).min(count - 1)
```

Con `count == 0` es underflow (panic en debug, `usize::MAX` en release). **Latente**:
el único llamador es `tray_icon_hit` (ramas vertical/horizontal, `:38` y `:44`), que
corta antes con `tray_count == 0` (`:17`); `tray_icon_center` no la llama (su guarda es
`idx >= tray_count`). **Fix:** `.min(count.saturating_sub(1))` como defensa en
profundidad — los tests la llaman directo. **Verificación:**
`assert_eq!(nearest_tray_index(0.0, 26.0, 0), 0)`.

### D10 — `read_cpu` suma `guest`/`guest_nice` (doble conteo) (AUDIT §6.8)

> **Resuelto.** La cuenta sale a `cpu_totales()`, que corta en `steal`: `guest` y
> `guest_nice` no se suman dos veces.

`src/widgets.rs:173-174`

```rust
let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
let total: u64 = fields.iter().sum();
```

En `/proc/stat`, `guest` ya está incluido en `user` (y `guest_nice` en `nice`): sumar
los 10 campos lo cuenta doble y sesga el porcentaje. **Fix:**
`let total: u64 = fields.iter().take(8).sum();`. **Verificación:** test con la línea
sintética `cpu  100 0 100 800 0 0 0 0 200 0 0 0` → total 1000, no 1200.

### D11 — `read_ram` sin saturar, y se llama dos veces por refresh (AUDIT §6.9)

> **Resuelto** (no estaba marcado en `AUDIT.md`). `read_ram` calcula con `f64` y
> `clamp` (no queda ninguna resta en `u64` sin saturar) y se llama una sola vez por
> refresh (`refresh_sys`), no dos: el arranque en frío dejó de leerla dos veces.

`src/widgets.rs:101-102` y `:212`

```rust
ram: read_ram().map(|(pct, _)| pct),
ram_gb: read_ram().map(|(_, gb)| gb),
```

```rust
Some((pct, (gb(total - avail), gb(total))))
```

Dos problemas: (a) el `pct` y los GB pueden venir de **dos muestras distintas** (y se
parsea `/proc/meminfo` dos veces); (b) `total - avail` en `u64` sin chequeo: si
`MemAvailable > MemTotal` (kernels/containers con contabilidad rara) es underflow.
**Fix:** `if let Some((pct, gb)) = read_ram() { self.ram = Some(pct); self.ram_gb = Some(gb); }`
(patrón que ya usa `refresh_cpu_ram`) y `total.saturating_sub(avail)`.

### D12 — Código muerto / no-op en render (AUDIT §6.10)

> **Resuelto del todo.** Además del `bg_margin` y el `let _ = label_len` (que ya no
> estaban), se borraron `ICON_OVERSAMPLE = 1.0` (se multiplicaba por 1.0 en el tamaño del
> caché de iconos) y el parámetro muerto `_text_cache` de `draw_network_widget`.

> **Parcialmente resuelto.** De la tabla de abajo quedan vivos `ICON_OVERSAMPLE`
> (`render/mod.rs:41`, multiplicado por 1.0) y el parámetro `_text_cache` muerto de
> `draw_network_widget`. El `let _ = label_len` y `bg_margin` ya no están.

| Archivo:línea | Código | Nota |
| --- | --- | --- |
| `render/syswidgets.rs:19,30` | `let label_len = …; draw_text_rotated(…); let _ = label_len;` | cálculo muerto, silenciado con `let _` |
| `render/mod.rs:26` | `const ICON_OVERSAMPLE: f32 = 1.0;` | multiplicado en `:330`: no-op |
| `render/mod.rs:277` | `let bg_margin = 0.0;` | resta de 0.0 |
| `render/syswidgets.rs:304-306` | `draw_network_widget(…, _text_cache: &mut TextCache, …)` | parámetro que el widget nunca usa; firma divergente del resto |

**Fix:** borrar los tres primeros y quitar el parámetro del cuarto (ajustar el match
en `render/layout.rs`). **Verificación:** `cargo build --release && cargo clippy --release --all-targets`.

### D13 — `IconCache::get` aloca un `String` por lookup (AUDIT §6.11)

> **Resuelto.** `IconCache` pasa a `HashMap<u32, HashMap<String, _>>`: el lookup usa el
> `&str` que ya tiene (el `String` se asigna sólo en el miss) y el tope es por tamaño, no
> global.

`src/icon_cache.rs:23-24` y `:36-37`

```rust
pub fn get(&mut self, icon_name: &str, size: u32) -> Option<Rc<Pixmap>> {
    let key = (icon_name.to_string(), size);
    if let Some(hit) = self.cache.get(&key) {
```

Se llama una vez por icono de tray y por carátula en **cada frame**; el hit de caché
no debería alocar. **Fix mínimo y realista:** memo de un elemento — guardar el último
`(name, size)` consultado y devolver el `Rc` directo cuando coincide (tray y carátulas
repiten el mismo par en frames consecutivos). Un `HashMap` con `Borrow` para `&str`
requeriría cambiar la key a algo tipo `Arc<str>`. **Verificación:** test con
`#[global_allocator]` contando allocations, o `perf stat` de malloc durante hover.

## Pendientes conocidos

- **Isla dinámica, lo que sigue** (ordenado por valor/costo, medido contra el código;
  lo marcado con ✔ ya está):
  1. ✔ **El marquee del título** (con `media_smooth_scroll` prendido) y ✔ **mostrar dos
     actividades** (la principal + la batería).
  2. **Las interacciones de puntero están bloqueadas por el trigger** (ver arriba):
     decidir si el disparador se achica al blob (rueda = volumen, tap = play/pause, y se
     pierde tirar el mouse al borde) o si se deja la isla como display y los controles
     viven en el dock revelado (hoy: así).
  3. **Un solo cuerpo que morphea**: hoy el OSD, la notificación y el HUD de
     workspaces le *piden prestada* la superficie al dock (`layer_is_borrowed`,
     `set_size` por modo), así que el dock desaparece cuando sale el OSD. Migrarlos a
     `draw_island` + el motor de reveal los vuelve estados de la misma cápsula y borra
     esa familia de bugs. Es el escalón estructural (medio-alto).
  4. **Que Media viva sólo en la isla**: hoy el widget `Media` está colocado en la
     barra (pedido explícito) y muestra `Nothing is playing` al pedo cuando no suena
     nada. Sacarlo de la barra y dejar que la isla lo lea sola pide desgatear
     `media_wanted`/`refresh_media`/`read_deferred` (con el arte gateado por
     `has_widget(Media)`, si no el `curl` de carátula de AUDIT A4 corre al vacío) — ~15
     líneas, a cambio del `playerctl --follow` encendido siempre que haya autohide.
  5. **Actividades concurrentes con prioridad real**: `island_activities` es una lista
     fija (principal + batería); falta la cola que desplace la que llega y el reparto en
     mitades. `island_plan`/`island_spans` ya son la única cuenta del largo y la
     posición, así que el split entra ahí. Ojo con la asimetría del tap: la actividad de
     Media existe **sólo mientras `playing`**, así que el tap pausa pero no reanuda
     (habría que decidir si un player pausado deja el título en la isla).
  6. **Grabación como actividad viva**: `record-toggle.sh` ya escribe
     `~/.cache/dockyrs-recording-path` al empezar y lo borra al terminar, así que
     alcanza con leerlo en el tick (el tiempo sale del mtime). Punto rojo + tiempo.
  7. **Long-press**: no tiene sentido todavía (la isla ES el dock: expandir ya es
     revelar). Serviría recién para "abrir la app del player" o para controles sin
     revelar, y para eso hay que decidir antes qué abre. Necesita un `Instant` del press
     (hoy hay `press_pos` pero ningún reloj de long-press).
  8. **Timer/countdown**: no existe nada de timers (necesita IPC + UI de entrada).
  9. **Notificaciones con botones**: el más caro: el notifyd ignora `actions`
     (`_actions: Vec<&str>`) y el pill no tiene hit targets ni teclado.
  - Cosmético descartado: **separador de 1 px entre actividades** (el gap de 5 ya las
    separa y el iPhone no dibuja divisor) y **una isla más gruesa que la barra** (el
    bump: el eje corto de la isla ES el grosor del dock, así que un bump más grueso pide
    pre-dimensionar la superficie al estado mayor **y** restringir la input region a la
    cápsula —hoy `sync_autohide_surfaces` hace `set_input_region(None)`, o sea que el
    aire reservado se come los clicks de abajo—; patrón a copiar:
    `dock_popup::popup_input_region()`. Subir `dock_scale` engorda las dos cosas).
  - Lo que **no** conviene: swipe para descartar (no hay detección de gestos y el
    mouse-out ya cierra), squish/stretch (la forma es un rounded-rect en un eje) y
    badges/Face ID/AirDrop (no hay fuente de dato).
  - **Prioridad honesta**: la auditoría **quedó cerrada** el 2026-09-20 — los 36 hallazgos
    de `AUDIT.md` están resueltos y documentados en “Cerrado de AUDIT.md”, arriba, cada uno
    con su evidencia. Los tres que quedan **abiertos a propósito** están en la §3 del audit:
    **A5** en parte (los drenajes de IPC se arreglan en la fuente, y los `.lock().unwrap()`
    no son alcanzables porque el `reexec` ya pasó), **B6** (`repo_dir` sólo afecta a los
    `sync-*.sh`, que acá no se usan) y **C1** (`DrawArgs` a todo `render/`: 57 sitios sin
    prevención real). O sea: de acá en adelante lo que hay es **producto**, no deuda — el
    orden sugerido está en “Isla dinámica, lo que sigue” y en “Niri: qué falta usar”, acá
    arriba.
  - **Verificado 2026-09-20 en una pasada por los pendientes**: de los dos puntos
    marcados "sin verificar a ojo" quedó **cero**. La grabación andaba pero tarde
    (trampa 17: el tick dormía 20 s, no 1) y el widget `Mic` quedó confirmado
    (`MUTE` en rojo / `ON` sin rojo). Lo único que sigue sin verificar a mano es el
    **click real del click-catcher** (la nota de arriba, línea ~889): el protocolo y
    la cuenta de la región están verificados, el click físico no — y ahora **sí se
    puede** (ver la ACL de `/dev/uinput` en esa misma nota), ya no hace falta sudo.

- **Niri: qué falta usar** (repaso contra la IPC de niri 26.x: `enum Request` 16
  variantes, `enum Event` 19, `enum Action` 141; el dock hoy usa Workspaces/Windows/
  Layers/KeyboardLayouts + el event-stream + un puñado de acciones). Ordenado por
  valor/costo:
  - ✔ **Urgencia** (HECHO): `WorkspaceInfo.urgent` se lee del `is_urgent` de niri y el
    punto del workspace va **rojo pleno** (`ws_tone` en `render/workspaces.rs`, el mismo
    rojo del mute). No hizo falta tocar la IPC: `niri_mensaje` ya manda cualquier línea
    con "Workspace"/"Window" a `WorkspacesChanged`, así que el evento
    (`WorkspaceUrgencyChanged`) re-lee los workspaces y el dato entra solo. Hyprland no
    expone urgencia (`urgent: false`). Guard: `ws_tone_tests::el_urgente_va_en_rojo_y_el_resto_en_el_acento`.
    Verificado en vivo: `niri msg action set-window-urgent --id N` → el JSON de
    workspaces pasa a `is_urgent: true` → el punto se ve rojo. **Ojo al probarlo**: la
    acción es `set-window-urgent --id <id>` (no existe `set-workspace-urgent`), y
    **enfocar la ventana limpia la urgencia** (lo dice niri), así que si cambiás al
    workspace de esa ventana el rojo desaparece — hay que dejarla en otro workspace
    (p. ej. estando en el vacío) para verlo.
  - **`Event::ScreenshotCaptured { path }`**: la isla puede hacer el flash "Guardado en
    …" **sin adivinar nada** y también para las capturas hechas con la UI de niri (hoy
    el dock sólo sabe de las suyas). El `path` es `None` si fue sólo al portapapeles.
  - **`Event::CastStartedOrChanged` / `CastStopped` / `CastsChanged`** (y
    `Request::Casts`): actividad de isla "se está compartiendo pantalla" — el mismo
    caso que la grabación, pero para cualquier cast (ventana/monitor).
  - **Nombres de workspace** (`Workspace.name` + `Action::SetWorkspaceName`): el widget
    muestra puntos; con el nombre se puede elidir texto. Y el dock puede **nombrar** el
    workspace activo.
  - **`Request::FocusedWindow` / `FocusedOutput`**: más barato que `niri msg --json
    windows` para "qué está enfocado" (título de la ventana en la isla, o resaltar la
    enfocada en el panel Windows).
  - **`Request::PickColor`**: "tomar el acento de un píxel de la pantalla" desde
    Ajustes → Colors. Es el caso de uso natural de este dock (ya tiene acentos +
    matugen) y niri lo expone listo.
  - **`Request::PickWindow`** (+ `Action::ScreenshotWindow --id`): capturar la ventana
    que clicás (es el ejemplo que da la propia doc de niri).
  - **`Window.focus_timestamp`** (+ `WindowFocusTimestampChanged`): el orden de
    **recientes** para el panel Windows, que hoy usa el orden de `niri msg windows`.
  - **Screenshot nativo** (`Action::Screenshot`/`ScreenshotScreen`/`ScreenshotWindow`
    con `show-pointer`): reemplazaría el selector de región propio (`app/screenshot*`,
    superficie full-screen + input region) por la UI de niri, que ya es táctil, tiene
    teclado y avisa con `ScreenshotCaptured`. Candidato fuerte a borrar código, con el
    costo de perder el look del dock en ese modo.
  - **Layout niri-native** (`ExpandColumnToAvailableWidth`, `SetColumnWidth`,
    `CenterColumn`, `ToggleColumnTabbedDisplay`, `MoveWindowToWorkspace`,
    `MoveWorkspaceToMonitor`): un panel/widget de layout en el dock.
  - **Chicos y baratos**: `Action::ShowHotkeyOverlay` (botón de ayuda de atajos),
    `PowerOffMonitors`/`PowerOnMonitors` (botón o actividad), `Quit { skip_confirmation }`
    como ítem del menú de energía, `ConfigLoaded` (el usuario recargó niri: re-leer
    `wallpaper_program()`/tema, que hoy se lee una vez al arrancar), `OutputsChanged`
    (monitor conectado: los perfiles por salida) y `Request::Version` (chequear antes de
    usar un request nuevo, en vez de asumir).

- **Panel de notificaciones** (implementado; el pedido era "un dock para notificaciones, lo
  más sencillo posible, con negrilla e itálica"):
  - **Historial**: `App::notifications: Vec<NotifyEntry { title, body, at }>`, el más
    nuevo primero, tope `menu::NOTIF_HISTORY_CAP` = 50. Lo llena `push_notification`, que
    se llama **antes** del corte de `show_notification` (el aviso entra al historial
    aunque el pill no se muestre porque hay otro modo abierto). `at` es la hora del reloj
    del dock **sin AM/PM** (`WidgetSnapshot::time_short`): no hay un segundo formato de
    hora en el código. Si el panel está abierto, el aviso nuevo lo re-abre para que el
    alto crezca (el frame sale de la cantidad de avisos).
  - **Un panel más del overlay**: pestaña **`Notifs`** (`OVERLAY_TABS` pasó a 5 y
    `OVERLAY_ORDER` a 5, entre Clipboard y Wallpapers), `notifications_mode` con sólo
    `{ scroll, hovered, frame, is_vertical }` — sin selección, sin animación y sin
    acciones. Geometría en `menu/notifications.rs` (filas de 34, 5 visibles, el rect de
    cada fila y su hit test) y dibujo en `menu_render/notifications.rs`. El cross en
    vertical es el angosto (`OVERLAY_PANEL_VERTICAL_W` = 170, medido: la superficie queda
    en 230x610).
  - **Tipografía**: título en negrilla (700), cuerpo normal elidido y **hora en
    itálica** — la itálica NO existía en el canvas: se rasteriza armando un SVG
    (`font-style`), así que ahora hay `TextCache::get_italic` (una línea de flag por
    `lookup`/`rasterize` + el flag en la clave del caché). El elidedor `fit` pasó a ser
    `pub(super)` para que lo compartan el portapapeles y este panel.
  - **El toast de notificaciones vive en su PROPIA superficie** (`dockyrs-notify`,
    `Layer::Overlay` anclada **arriba a la derecha** con margen 8, `exclusive_zone(-1)`,
    teclado `None` e **input region = su rectángulo** — un click sobre él lo descarta y
    el resto se lo traga la superficie del dock). Se crea al primer aviso y se **suelta**
    al terminar el fade
    (`toast_layer = None`), como el popup: es descartable, no la compartida del dock
    (trampa 2 no aplica). Antes el aviso le pedía la superficie al dock, que es una
    franja de 26 px pegada al borde izquierdo: de ahí que apareciera encima del panel
    del launcher y que la píldora fuera vertical (76x300). Ahora el toast es **siempre
    apaisado** (300x76 medido) y `NotificationArgs::is_vertical` lo fuerza, porque el
    dock de este setup es vertical y el render elegía el layout con `dock.is_vertical()`.
  - Por eso `notification_mode` **ya no está** en `layer_is_borrowed` ni en
    `forces_dock_visible` (el dock no se revela ni se da por prestado por un aviso), ni
    hay corte por modos abiertos en `show_notification`: un aviso se ve aunque el
    launcher esté abierto.
  - **El cierre es INMEDIATO a propósito** (timeout de 4 s + 0.9 s por línea, o un
    click). Antes se iba con un fade que dependía de los frame callbacks de la
    superficie: si esos no llegaban, el aviso se quedaba pegado en pantalla para
    siempre (bug reportado). No volver a meterle animación de salida sin resolver eso:
    la superficie se suelta de una (`toast_layer = None` la desmapea).
  - **Un click sobre el toast lo descarta**: la input region es SU rectángulo
    (`region.add(0, 0, w, h)`) y el `Press` sobre esa superficie llama a
    `close_notification_mode` desde `handlers::pointer_frame`.
  - **Un `notification_mode = None` de otro panel dejaba el toast mapeado PARA
    SIEMPRE** (cazado en vivo el 2026-09-20, en el equipo remoto). Los cinco `open_*`
    del overlay (launcher, portapapeles, fondos, ajustes, notificaciones) pisaban
    `notification_mode = None` a mano y `close_notification_mode` cortaba temprano con
    el mismo `is_none()`: la superficie `dockyrs-notify` quedaba mapeada con su input
    region (300x76 en la esquina) y `niri msg --json layers` la seguía listando minutos
    después — o sea un click-blocker invisible en la esquina superior derecha hasta
    reiniciar el dock. Medido: notificación → abrir el panel de notificaciones 1 s
    después → `dockyrs-notify` todavía en la lista a los 10 min; **sin** abrir ningún
    panel el camino del timeout sí la liberaba (a los 4 s). Arreglo: los `open_*` ya no
    tocan `notification_mode` (los dos viven: el aviso termina su timeout encima del
    panel abierto, que es lo que promete el diseño) y `close_notification_mode` suelta
    la superficie sin corte temprano.
  - **El toast ya no es "un modo más" de la superficie del dock**, y tres caminos
    viejos que le apuntaban a esa superficie costaban de verdad: `pointer.rs`
    interceptaba TODOS los eventos de puntero de la franja (Enter/Motion/Leave
    incluidos) y devolvía sin procesarlos mientras había un aviso arriba —hover y
    autohide del dock muertos 4 s—; `handlers::frame` consumía el frame del dock para
    tickear la notificación (el reveal y la isla se quedaban sin reloj); y `show_osd`
    salía temprano con `notification_mode.is_some()`, así que una tecla de volumen
    dentro de esos 4 s no mostraba OSD. Los tres se borraron: el toast se tickea y se
    cierra por su propia superficie (`is_toast` en `handlers::frame`).
  - **Ojo al crearla**: la superficie recién creada no tiene tamaño hasta el
    `configure`, así que el primer dibujo lo dispara `handlers::configure` (un `attach`
    antes de eso lo rechaza el compositor).
  - **Cierre y teclado del panel**: ESC y click afuera (catcher) cierran, como los otros
    paneles; el modo está en `enforce_keyboard`, `layer_is_borrowed`, `forces_dock_visible`,
    `overlay_panel_size`, `current_overlay`, `cycle_overlay`, `dismiss_overlay` y en el
    corte de `show_notification`. **Ojo con `overlay_panel_size`**: sin el arm de un modo
    nuevo el panel queda mapeado pero invisible (no hay reparto) — pasó en la primera
    prueba.
  - Flechas/rueda/PageUp/PageDown/Home/End scrollean (`scroll_notifications`, clamp con
    `notif_max_scroll`); IPC `--toggle-notifications` (y el ciclo de pestañas).
  - Guards: `menu::notifications::notif_tests` (el hit test cae en la fila que dibuja el
    rect, fuera del frame no hay fila, el alto se acota y el scroll no pasa de lo que
    sobra) y `menu_render::tabs` (el slot de la banda ahora con 5 pestañas). Sensor:
    `notifs: abre con N aviso(s)` en el log.

- **Widgets y actividades nuevos** (propuesta, nada de esto está implementado). Un
  widget nuevo son **4 lugares**: el enum `WidgetKind`, su entrada en `WIDGETS`
  (`natural_len` + `draw` + `click`), `WIDGET_KIND_ORDER` y `widget_label`; el test
  `la_tabla_cubre_todos_los_widgets_del_panel` avisa si falta alguno.
  - **Para el dock**, por valor/costo:
    1. ✔ **Mic mute** (HECHO y **verificado a ojo**): widget `Mic` (icono +
       `ON`/`MUTE`, **rojo cuando está muteado**), `read_mic` comparte el parseo con
       `read_volume` (`read_wpctl`) y el click togglea el mute
       (`WidgetAction::ToggleMic`). Se lee en el tick SÓLO si el widget está colocado
       (es otro `wpctl` de ~19 ms). Verificado: `MUTE` en rojo muteado y `ON` sin
       rojo al desmutear (`wpctl set-mute @DEFAULT_AUDIO_SOURCE@ 0`). **Cómo verlo**:
       agregar `{"kind":"Mic","slot":"Left"}` a `settings.widgets`, reiniciar y
       revelar el dock con `niri msg action open-overview` o con
       `scripts/pointer.py` (funciona sin sudo); después restaurar la config.
    2. **Perfil de energía**: `powerprofilesctl get/set` (performance/balanced/
       power-saver) con click que cicla. Barato.
    3. **Inhibidor de idle**: lanzar/matar `systemd-inhibit --what=idle:sleep` y mostrar
       el estado (hay que matar el hijo al salir del dock). Barato.
    4. **Clima**: Open-Meteo por HTTP (sin API key) con `run_with_timeout`, caché en
       disco y refresco cada 15-30 min. Primer widget que necesita red + estado propio:
       el más pedido, costo medio.
    5. **Tráfico de red**: deltas de `/proc/net/dev` cada 2 s (↓↑ en KB/s); hay que
       guardar la muestra anterior, como hace `custom_last_polls`.
    6. **Espacio libre** (`statvfs` del home) y **temperatura** (`/sys/class/thermal`):
       baratos, valor medio; lo que se extraña de CPU/RAM.
    7. **Notificaciones + No Molestar**: el notifyd es nuestro (contar no leídas + un
       toggle), pero toca el otro binario y un canal nuevo: costo medio.
  - **Para la isla**, como actividad viva (el motor ya está: `island_activities` + el
    `draw` del widget o un caso propio):
    1. **Timer/Pomodoro**: el clásico de una isla. Backend barato (`Instant` + el tick del
       sistema) + IPC `dockyrs --timer 25m`; lo caro es la UI de entrada.
    2. **Grabación**: punto rojo + tiempo leyendo `~/.cache/dockyrs-recording-path` (lo
       escribe `record-toggle.sh`) y el mtime. Barato y muy "live activity".
    3. **Cargando** (rayo + % unos segundos al enchufar) y **batería baja** (≤10%: el blob
       late en rojo). El dato y el color ya están.
    4. **Flash de "listo"**: al guardar un screenshot o copiar al portapapeles, la isla
       muestra el destino o el conteo 2 s. Los dos flujos ya existen y saben cuándo
       terminan.
    5. **Conectividad**: wifi caído / VPN (2 s). El dato de red está; la VPN habría que
       leerla.
    6. **El OSD (volumen/brillo) como estado de la isla** en vez de pedirle la superficie
       al dock: es el escalón estructural que ya está en la lista de arriba.

- **Grabación como actividad viva** (implementado y **verificado a ojo**): widget
  `Recording` (punto rojo + `MM:SS`, el mismo reparto de pastilla que volumen/mic) y
  `island_activities` lo pone **primero** cuando hay grabación, aunque el widget no esté
  colocado en la barra: el dato es un `stat` (`read_recording` mira
  `~/.cache/dockyrs-recording-path`, que escribe `record-toggle.sh`, y el tiempo sale
  del `mtime`), así que no cuesta un spawn. El click corre el script.
  - **El bug de la verificación fallida era el tick, no el dato** (trampa 17):
    `refresh_recording` se llama desde `refresh_clock`, y ese ticker dormía **20 s**,
    así que el archivo se veía hasta 20 s después y el contador saltaba de a 20. La
    espera de 2 s de la prueba era más corta que el tick, de ahí el "no se ve".
    Arreglado: el ticker va a **1 s**.
  - Verificado a ojo (crear el archivo → isla en ~1-2 s con `00:02`; unos segundos
    después `00:25`; borrarlo → la isla vuelve a reloj+batería en ~1-2 s). El archivo
    se creó a mano; no hace falta `wf-recorder` para probarlo.

- **A2 cerrado, por dos vías**: `extract_color_scheme` (los colores que el dock saca del
  fondo, que corría en el hilo principal y tarda 1-2 s) pasa por `widgets::run_with_timeout`
  (que pasó a `pub(crate)`) con un tope de **10 s**: matugen tarda de verdad, así que el
  tope es holgado, pero colgado ya no deja al dock sin dibujar para siempre. Y el matugen
  **del usuario** (`run_matugen`, el de "Matugen Apps") sale por un hilo propio
  fire-and-forget en sus dos llamadores (`apply_matugen_to_apps` y `apply_wallpaper`), así
  que tampoco frena el loop — ojo: ese no lleva timeout, un matugen colgado deja un hilo
  vivo (no bloquea la UI, pero no se recicla).

- **Ojo al agregar un widget (corrección)**: son **2 lugares**, no 4 — el enum
  `WidgetKind` y la entrada en `WIDGETS` (con `label`, `natural_len`, `draw` y
  `click`). El orden y las etiquetas de Ajustes salen de la tabla
  (`widget_kind_order`/`widget_label`), así que no hay listas paralelas. El guard
  `la_tabla_cubre_todas_las_variantes_del_enum` **falla a propósito** y hay que
  actualizar sus dos números (los fijos + 1 por `Custom`, y los fijos). Y ojo con el
  anclaje al insertar en `WIDGETS`/`syswidgets.rs`: meter una función *antes* de otra
  se lleva su `#[allow]` pegado (eso sumó un warning nuevo de clippy hasta que lo
  devolví).

- **Widgets y actividades descartados a propósito**: relleno/brillo automático por hora,
  clima en la isla (la isla es de 26 px de grosor: entra un número, no un ícono con
  texto), badges numéricos sobre los widgets y cualquier cosa que necesite un `curl`
  sincrónico en el hilo principal (AUDIT A4 sigue abierto).

- Paneles del overlay: el **ancho del contenido** ya es el mismo en los tres
  (`OVERLAY_PANEL_W` en horizontal; `OVERLAY_PANEL_VERTICAL_W` = 170 en vertical,
  salvo el launcher que pide 330 por las dos columnas) y la banda es la misma fila o
  columna de 26, pero el alto no (236 / 460 / 196 en horizontal, 556 / 434 / 640 en
  vertical).
  - El portapapeles usa el cross ANCHO en vertical (`OVERLAY_PANEL_VERTICAL_WIDE` =
    330, el del launcher): con 170 los títulos se elidían a ~16 caracteres. Los fondos
    siguen en 170 porque su miniatura mide 150.
  - La banda de pestañas es clickeable además de ciclarse con Shift+←/→: el click va
    derecho a la que se clickeó (ver el bullet "La banda de pestañas es clickeable").

- **En el vertical los cuatro paneles del overlay comparten el ALTO**
  (`menu::OVERLAY_PANEL_H` = 640), que es el espejo de `OVERLAY_PANEL_W` en el
  horizontal: el borde de arriba y el de abajo no se mueven al ciclar con Shift+←/→.
  Medido antes: apps 556, portapapeles 434, notifs 434 y fondos 640 ⇒ el panel saltaba
  206 px al cambiar de pestaña. El ancho (cross) **no** se toca: los fondos siguen en
  196 por la miniatura de 150 y el resto en 356.
  - **Las filas que entran salen del alto del frame**, no de una constante: los tres
    paneles que listan cosas tienen su propia cuenta y las tres descuentan su padding
    de abajo — `menu_render::clip_visible_rows(frame)`, `menu::notif_visible_rows(frame)`
    y `menu::app_search_strip_h(frame, true)`. En el horizontal dan exactamente las de
    siempre (7 / 12 / 2 filas), así que el panel ancho no cambió: para el launcher y el
    portapapeles el test compara las dos orientaciones.
  - La fila que no entra **queda asomando** (el viewport es el alto real, no filas
    enteras): es la pista de que hay más, y es lo que el portapapeles ya hacía con su
    fila +1. En el launcher el vertical pasó de 5 filas a 5 + un peek de la 6ª.
  - El precio: en las listas cortas (notifs con 3 avisos, portapapeles vacío) el panel
    es alto y sobra aire abajo. Es lo que pide un alto común; la alternativa es que el
    panel vuelva a crecer con el contenido y el borde salte al ciclar.
  - El `along`/`WALLPAPER_PANEL_MIN_W` de `open_wallpaper_picker` se borró: en el
    vertical el alto es `OVERLAY_PANEL_H` y en el horizontal `WALLPAPER_PANEL_H`.
  - `NOTIF_VISIBLE_ROWS` y `APP_SEARCH_VERTICAL_VISIBLE` se borraron (los reemplazan
    esas cuentas). PageUp/PageDown del panel de notifs ahora avanza **una página de
    verdad** (`notif_visible_rows`), no las 5 filas de la constante vieja.
  - Guard: `menu_render::clipboard::clip_vertical_tests::las_filas_salen_del_alto_del_frame`
    (el frame ancho da las 7 de siempre y el vertical pide más). A mano: los cuatro
    `--toggle-*` con el dock en `Left` ⇒ las cuatro superficies abren 640 de alto
    (390/390/230/390) y volver a 26x584 al cerrar; con ↓ 200 veces el tope del scroll
    deja la última fila entera y el peek no se despega del borde.

- **La banda de pestañas es clickeable** (pedido: con el cursor no se podía cambiar de
  pestaña). El click va **derecho** a la pestaña clickeada —no cicla— y el ciclo con
  Shift+←/→ sigue igual: los dos entran por `App::switch_overlay` (que es el viejo
  `cycle_overlay` partido en dos, con la dirección del deslizamiento como parámetro).
  - El reparto de los slots vive en **`menu_render::overlay_tab_layout`**, la función que
    ya usaba el dibujo, y `overlay_tab_at` traduce un punto a un índice: una sola cuenta
    para la pastilla, el slot y el click (trampa 10). Antes el reparto estaba adentro de
    `draw_overlay_tabs` y no se podía reusar, que es por lo que la banda no era
    clickeable.
  - En el panel vertical el slot mide `pill_len + 10 * scale` (84 medido), así que los
    cinco slots **no reparten los 640 del panel**: el aire de abajo no cambia de modo
    (`overlay_tab_at` devuelve `None`) y el hit test resta el origen de la banda, que con
    el dock a la derecha arranca corrido.
  - `App::overlay_tab_hit` sale de `current_overlay()` (el panel de ajustes comparte la
    superficie pero no tiene banda) y usa el mismo `scale` que el dibujo
    (`output_scale`), con las coordenadas del `panel_local`. El chequeo va **antes** de
    las ramas de cada panel en `pointer.rs`, así que el hit test del panel no se lo come.
  - Click en la pestaña activa = nada (no cierra el panel): para cerrar están ESC, el
    click afuera y clickear un widget del dock.
  - Verificado a mano con el dock en `Left`: las cinco pestañas se clickean (Apps →
    Clipboard → Notifs → Wallpapers → Windows → Apps, incluidas las dos que comparten
    `app_search_mode`), el panel queda en 390x640 en todas, la pestaña activa no hace
    nada y el aire debajo de los cinco slots tampoco. Sensor: `overlay: click en la
    banda (px,py) -> <Modo>` en el log. Guard:
    `menu_render::tabs::tabs_tests::el_click_de_la_banda_cae_en_la_pestana_dibujada`
    (centros y bordes de los cinco slots en fila y columna, el aire del final de la
    columna y la banda corrida del dock a la derecha).

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

- El panel del overlay va AL LADO del dock en los cuatro bordes, pero el `MENU_GAP`
  entre los dos es 0 y la separación de la composición es `PANEL_GAP` (8): con el dock
  abajo o a la derecha el panel hereda el `pos_y` del dock (se mueve con él).
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

- **La IPC del dock se quedaba muda para siempre (bug cazado 20-sep).** Síntoma:
  `Meta+Space` (y todo `--toggle-*`, `--notify`, screenshots) no hace nada, sin ningún
  error. Diagnóstico: `pgrep -x dockyrs` vivo, el hilo principal en `ppoll`, el socket
  de `/run/user/1000/dockyrs.sock` **bound y LISTENING** en `/proc/net/unix`, el proceso
  con su fd… y `connect()` dando **`ECONNREFUSED`**: eso es **cola de aceptación llena**
  (en AF_UNIX, a diferencia de TCP, la cola llena se reporta así). Causa: el listener era
  **un solo hilo** y corría `handle_client` **en línea**, así que un cliente que conecta y
  no escribe (o cuyo `read` se cuelga) dejaba de aceptar; a los ~128 clientes, todos los
  `connect` fallan y el cliente los ignora en silencio (`send_message` no chequeaba nada).
  Arreglo: **un hilo por cliente** (`ipc::spawn_listener`) y `send_message` que avisa por
  stderr cuando no puede conectar/escribir. Verificado abriendo **200 conexiones mudas**:
  con el dock nuevo el launcher y las notificaciones siguen andando (antes, con eso, la
  IPC moría). Lección para el próximo "no anda el keybind": si el comando es de la IPC,
  mirarlo del lado del socket antes de sospechar del bind de niri.
