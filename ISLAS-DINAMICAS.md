# Islas dinámicas: qué copiar de ChillPill-Shell y island (QML) a Docky.rs

Investigación del 2026-09-20. Fuentes:

- [LUCKYS1NGHH/ChillPill-Shell](https://github.com/LUCKYS1NGHH/ChillPill-Shell) — barra
  "pill" con Quickshell para Hyprland.
- [Guilhermerisu/island](https://github.com/Guilhermerisu/island) — isla dinámica para
  Omarchy, también Quickshell.

Los dos son **QML sobre Quickshell / wlr-layer-shell**. Docky.rs ya tiene el motor de
isla (`reveal_anim`, `island_plan`, `island_spans`, `draw_island`) y rasteriza por CPU
(tiny-skia, 0 GPU). Este documento separa **comportamiento** (portable a Rust, barato)
de **modelo de render** (QML/GPU, no portable sin pagar el costo).

---

## 1. Resumen ejecutivo

| | ChillPill-Shell | island (Guilhermerisu) | Docky.rs hoy |
| --- | --- | --- | --- |
| Stack | QML + Quickshell | QML + Quickshell (dentro de Omarchy) | Rust + tiny-skia (CPU) |
| Render | GPU | GPU | CPU, sólo cuando algo cambió |
| RAM | 200–500 MB (media 380) | corre en el proceso de Quickshell | ~13–30 MB RSS |
| CPU | idle 0%, media 3%, max 10% | no medido | piso (~1 tick cada 5–6 s) |
| GPU | media 15%, max 45% | GPU | 0 |
| Forma | barra fija + paneles/popups | **una cápsula que morphea entre vistas** | dock ↔ isla (el **núcleo** revela, el resto es hover) |
| Interacción en reposo | hover en la barra | tap / rueda en la isla | rueda = volumen, tap = play/pause |

**Conclusión de una línea** (actualizada después de A–M): Docky ya tenía el 70 % del motor
de "isla dinámica" y **ahora tiene el motor completo** — actividades vivas con cola y
vencimiento, la isla como estado de sus modos (OSD incluido), animaciones por **tiempo** e
interacción por **núcleo**. Lo que queda es **producto** (descargas, Ask AI, timer,
tooltips, DND) y un puñado de eventos de niri que ya llegan al event-stream. Nada de eso
sube el piso de CPU.

**Lo que NO hay que portar:** el visualizador de audio tipo cava a 30 fps, ni el morph
continuo de QML. Son exactamente las cosas que en QML se pagan con 15 % de GPU y acá
pagarían CPU en reposo (trampa 15: un resultado animado necesita el loop de frames
vivo).

---

## 2. Qué hace cada proyecto (lo relevante)

### 2.1 island — el más cercano a lo que se quiere

Una sola `Rectangle` (`island` en `Island.qml`) con `Behavior` sobre `targetWidth`,
`targetHeight`, `radiusCap`, `y` y `color`. El `view` es una máquina de estados
(`"rest"` normal; `"feedback"` para pills efímeras; `"controls"`, `"player"`, `"apps"`,
`"themes"`, `"wallpapers"`, `"power"`, `"menu"`, `"answer"` para vistas expandidas).
Cada vista declara su `islandWidth`/`islandHeight` y la isla se anima entre ellos.

Piezas portables:

1. **Feedback efímero con duración y tipo** (`showFeedback(message, duration, kind)`):
   un único `feedbackTimer` y un `feedbackKind` (`notification`, `volume`,
   `clipboard`, `activity`, `workspaces`, `keyboard`). Si hay una **vista** abierta, el
   feedback se **suprime** (`if (surfaceOpen) return`). Una notificación no puede ser
   pisada por otro feedback mientras corre.
2. **Live activities** (`services/DeviceActivities.qml`): batería (empieza a cargar,
   cae a 20 % y a 10 %, una vez por descarga), Bluetooth (conecta/desconecta con
   batería del dispositivo). Guarda `batteryWarned` para no repetir el umbral, y tiene
   un **warm-up de 3 s** donde sólo *registra* el estado, para no anunciar el estado
   inicial como novedad.
3. **Downloads** (`services/DownloadTracker.qml`): sondea `~/Downloads` por
   `*.part`/`*.crdownload`, reporta **bytes y velocidad** (no porcentaje: el navegador
   no guarda el total), detecta el **rename** al nombre final para el "listo", y sólo
   corre el timer mientras hay algo.
4. **Actualizaciones de sistema** (`services/PackageUpdateTracker.qml`): el lock de
   pacman (`/var/lib/pacman/db.lck`) arranca la actividad; lee el **log** (`/var/log/pacman.log`)
   por fase (Syncing/Downloading/Installing/Building), cuántos paquetes lleva, la
   velocidad por `rx_bytes`, y `pgrep makepkg` para el paquete en build.
5. **Portapapeles** (`services/ClipboardWatcher.qml`): vigila el `clipboard-history.json`
   de Omarchy por mtime; el contenido **ya presente al arrancar no es novedad**
   (`seeded`), y hay un `quietUntil` para cuando el propio paste vuelve a copiar.
6. **HUDs**: workspaces y layout de teclado como feedback de ~1.2–1.4 s.
7. **Ask AI**: escribir una pregunta en el launcher la manda a `claude`/`codex` por CLI
   y la respuesta aparece **en la isla** (`view = "answer"`, Siri-style).
8. **Ajustes propios** (`services/IslandSettings.qml`, `~/.config/omarchy/island.json`):
   `motionScale`, `hoverLift`, `clock24h`, `mediaPill`, `volumeHud`, `bannerSeconds`,
   `notch`, `downloads`, `clipboard`, `systemUpdates`, `batteryActivity`,
   `bluetoothActivity`, `workspaceHud`, `keyboardHud`, `askAi`, orden de las tarjetas
   del control center.
9. **MotionAnimation** (`components/MotionAnimation.qml`): un `NumberAnimation` con
   **tokens** — `pace` (`standard`/`morph`/`collapse`/`quick`) → duración, `curve` →
   `Easing.BezierSpline`. Los Behaviors **retargetean desde el valor actual**, así que
   invertir una transición no salta.
10. **Notch**: esquinas superiores rectas + dos "ears" cóncavas dibujadas con `Canvas`.
11. **Click afuera cierra**, con un **armado de 120 ms** para que el click que abrió no
    cuente.

### 2.2 ChillPill-Shell — barra + paneles, no isla

No morphea: es una barra fija con **estados** que abre con click (centro de control,
cliphist, mini dashboard) y OSDs. Lo portable son detalles de barra:

1. **CustomBarModule**: módulos waybar-style (`run` + `format`/`tooltip` con
   `{text}`/`{icon}`, `every`, `stream` = mantener el proceso y actualizar por línea,
   `click`, y salida JSON `{text, tooltip, icon, color}`).
2. **Tooltips** en cada módulo.
3. **FullscreenOsd**: tarjeta flotante que **se desliza desde el borde y se
   desmapea** al terminar la salida (`onFinished: shown = false`), desacoplada de la
   barra.
4. **NotificationStack**: dedup (`avoidDuplicateNotifications`), tope
   (`maxNotificationsInStack = 20`), "Clear all", y preview de icono.
5. **AudioVisualizer (cava)**: espectro dibujado en dos variantes (skyline y onda
   espejada).
6. **Timer/countdown** con presets y hold-to-burst; **Weather** con popup de pronóstico;
   **data usage** por deltas; **wifi/bluetooth con prompt de contraseña**; perfil de
   usuario + uptime.

---

## 3. Estado actual de Docky.rs (para no duplicar)

**Actualizado después de A–M**: lo marcado ✔ en §4 ya está en el árbol.

Ya existe y se reusa:

- **Motor de forma**: `reveal_capsule` (una sola cuenta de la cápsula para máscara, fondo
  y blob), `reveal_mask`, `draw_capsule`, `draw_island`, `island_spans`, `island_plan`.
- **Motor de actividades vivas**: `IslandActivities` (`app/island_activity.rs`) — cola,
  vencimiento y prioridad; una actividad viva **ocupa la isla** y al vencer vuelve al
  reposo. `island_activities()` devuelve el reposo (Recording > Cast > Clock > Workspaces
  si el split > 0 > Battery) o **sólo** la activa.
- **Actividades ya cableadas**: portapapeles (widget `Clipboard`), layout de teclado,
  Bluetooth, batería (enchufar / 20 % / 10 %), **captura guardada** y **compartir
  pantalla** (las dos últimas, de `ScreenshotCaptured`/`CastsChanged` de niri).
- **Reuso de dibujo**: `layout::draw_one_widget` — la isla no tiene una segunda versión de
  cada widget.
- **Interacción de la isla**: el reveal lo dispara su **núcleo** (`island_core_region`, el
  40 % central del blob); el resto del blob es hover franco (lift) **más rueda = volumen y
  tap = play/pause**. Un click en el núcleo revela ya; afuera rige el dwell de 120 ms.
- **Split por workspace** con easing (`ws_split_eased`).
- **Widgets**: `WidgetKind` + la tabla `WIDGETS` en **`src/widget.rs`** (un widget nuevo son
  **2 lugares**: el enum y la tabla; el orden y las etiquetas de Ajustes salen de ahí). Los
  de sólo-isla (`Screenshot`, `Cast`) los filtra `es_solo_isla`.
- **Custom widgets**: `CustomWidgetSource` con intervalo y timeout (`src/widgets.rs`), pero
  sólo texto plano, sin `stream`/JSON/tooltip.
- **Notificaciones**: historial + toast en superficie propia + panel `Notifs` + dedup.
- **Click-catcher** para cerrar al click afuera (`app/click_catcher.rs`).
- **Animaciones por tiempo**: `menu::Pace` + `anim_step`/`anim_towards`/`lerp_factor` con
  `App::frame_dt_ms`, así no dependen del frame rate del compositor.
- **OSD como estado de la isla**: píldora del grosor del dock, sin superficie prestada.
- **IPC**: canal `IpcMessage` (threads → loop), `run_with_timeout`. El gate por `has_widget`
  es **sólo para lo que corre en el tick**; lo event-driven (niri, D-Bus) se lee siempre,
  porque el evento ya llegó y el spawn es uno por evento raro.

Falta (y es lo que este informe prioriza):

- Fuentes nuevas: **downloads** y **updates de sistema**.
- Producto: **Ask AI**, **timer**, **tooltips**, custom `stream`+JSON.
- Deuda conocida: reloj de animación **por superficie** (hoy compartido: dos animando a la
  vez corren hasta 2× más lento) y la input region del OSD oculto (~150 px contra los 220
  de la píldora).

---

## 4. Qué implementar (ordenado por valor/costo)

Cada fila indica **dónde tocaría** y si **sube el piso de CPU** (lo que hay que evitar).

> **Cómo leer los ítems ✔**: el texto de arriba es la **propuesta original** (lo que se
> pensó antes de implementarlo, en presente) y lo que va después de **Implementado** es lo
> que quedó de verdad en el árbol, con lo que se desvió y por qué. Los ítems sin ✔ siguen
> pendientes y su texto es la propuesta vigente.

### 4.1 Alta prioridad · bajo costo · no sube el piso

#### A. Motor de actividades vivas (cola + duración + prioridad) — ✔ HECHO

El cambio estructural que habilita todo lo demás. Hoy `island_activities` devuelve una
lista que se dibuja entera; island muestra **una actividad a la vez** con un reloj.
Implementado:

- `app::island_activity::IslandActivities`: cola pura (`announce`/`expire`) + `VecDeque`
  de espera, testeada sin Wayland.
- `App`: `island_activity`, `island_activity_dirty` (trampa 15 del caso chico).
- `island_activities(widgets, ws_split, active)` devuelve **sólo** la activa cuando hay
  una (y con eso la miden `island_plan`/`island_blob_region`/`draw_island`/`draw`).
- El vencimiento viaja por el canal del autohide y se cobra en `autohide_timeout`: **sin
  timer, thread ni IPC nuevos**.
- Regla de supresión: con el dock visible o un panel/menú/popup abierto, se descarta.
- Guards: `app::island_activity::island_activity_tests` +
  `render::reveal_tests::una_actividad_viva_reemplaza_el_reposo`.

Costo real: ~200 líneas + 5 firmas de render tocadas. **Sin spawns nuevos.**

#### B. Batería cargando / batería baja (live activity) — ✔ HECHO

El dato **ya está**: `refresh_battery` corre cada 3 s. Implementado en
`App::note_battery_activity`:

- `battery_warned` (20 → 10, una vez por descarga), `battery_on_power` (lectura previa)
  y un **warm-up** del primer dato para no anunciar el estado de arranque.
- Al enchufar, al cruzar 20 % y al cruzar 10 %: `announce_island_activity(Battery)`.
- Dibuja el MISMO widget de batería (`draw_one_widget`), así que el color (verde
  cargando / rojo ≤20 %) sale gratis.

Costo real: ~60 líneas. Guard: `island_activity_tests::los_umbrales_de_bateria_son_20_y_10`.
**Sólo dispara con el widget `Battery` colocado** (los `refresh_*` están gateados por
`has_widget`, invariante de AGENTS): desgatearlo es una decisión aparte.

#### C. Clipboard copiado (live activity) — ✔ HECHO

Docky ya tiene el watcher `zwlr_data_control` y **toda** copia de cualquier app pasa
por `App::ingest_clipboard_capture` (el único sitio que agrega al historial).
Implementado:

- En ese sitio se guarda `WidgetSnapshot::clipboard` (vista liviana, sólo el `title`) y
  se llama a `announce_island_activity(WidgetKind::Clipboard)`.
- `WidgetKind::Clipboard` es un widget nuevo (enum + `WIDGETS`): pastilla icono+
  etiqueta igual a volumen/mic. En la isla es la actividad; en la barra es un widget
  con la última copia. El click abre el historial.
- `clipboard_label` recorta a 24 chars y es la misma cadena que mide y dibuja (trampa 12).

Costo real: ~120 líneas (la mayor parte, el widget nuevo). **Es la única actividad que
no necesita un widget colocado**, porque el watcher es global. No lee el historial al
arrancar: vacío hasta la primera copia de la sesión (no paga el parseo en frío).

Guards: `widget::clipboard_label_tests`. Verificado en vivo con `wl-copy`.

#### D. Bluetooth conectó / desconectó (live activity) — ✔ HECHO (verificado por tests)

`App::refresh_bluetooth` difiere el `connected` (`Option<String>`) contra el último
visto y anuncia `WidgetKind::Bluetooth` al conectar/desconectar/cambiar de dispositivo.
Warm-up del primer dato. La actividad dibuja el widget de Bluetooth (icono).

**Decisión de gating**: `refresh_bluetooth` **dejó de estar gateado por el widget**
(es event-driven: no hay tick). Inconveniente: el nombre del dispositivo no se ve (el
widget es symbol-only). **No verificado en vivo** (no hay dispositivo conectado).
Guard: `app::draw::island_activity_source_tests`.

#### E. HUD de layout de teclado — ✔ HECHO y verificado en vivo

`App::refresh_kblayout` anuncia `WidgetKind::KbdLayout` cuando el layout cambia de
verdad (ignora el `--`/vacío de arranque). La actividad es la pastilla del widget
("EN"/"ES"). **También se desgateó** (event-driven).

Verificado: `niri msg action switch-layout 1` → `island: actividad KbdLayout` → la isla
muestra "EN" → 3,2 s después vuelve a hora+batería, con el cambio de vuelta
re-anunciando. Guard: `app::draw::island_activity_source_tests`.

#### F. Tabla de curvas (MotionAnimation portado) — ✔ HECHO y verificado en vivo

Hoy el morph usa `ease_out` + pasos por tick. Reemplazar por una tabla
`pace → duración` y `curve → cubic-bezier` evaluada en Rust, con retarget desde el
valor actual (ya se hace: `island_ws_split` va hacia `_target`). Beneficio: feel
consistente en reveal, split, slide del overlay y panel de ajustes, con **una** función
(`menu::overlay_slide_offset` y los pasos del split pasarían a derivar de ahí).

Costo: ~60 líneas + tests puros. No sube el piso (es matemática, no más frames).

**Implementado** (con un ajuste de diseño): lo que estaba mal no era la CURVA —el dibujo ya
la aplica con `ease_out`/`ws_split_eased`— sino el **reloj**: los `+= 0.07` por frame ataban
la velocidad al frame rate. Ahora `menu::Pace { Open, Close, MorphClose, Quick }` +
`anim_step`/`anim_towards` avanzan por `dt` (`App::frame_dt_ms`, de `handlers::frame`) y
`lerp_factor` corrige los 4 lerps de scroll/resaltado. El ritmo se conserva (duraciones =
las que daban los pasos viejos a 60 fps). Se borraron las constantes de paso. El cubic-bezier
explícito no hizo falta: la curva ya vive en el dibujo y agregarla cambiaría el feel sin
ganar nada. Verificado: reveal `0.00 -> 0.07` por frame, llega a 1.00 en ~240 ms y se apaga.
Guards: `menu::anim_tests`.

#### G. Hover lift de la isla — ✔ HECHO (dwell de 120 ms) y verificado en vivo

`scale = 1.018` con el puntero encima, sólo en reposo. En Rust es un factor en
`reveal_capsule`/`draw_island`. Cosmético, ~10 líneas. Ojo: el hover de la isla hoy es
inalcanzable por diseño (la isla no recibe puntero, ver AGENTS "La isla NO recibe
puntero") → este ítem depende de la decisión de diseño de ampliar el trigger.

**Confirmado leyendo el código** (`app/pointer.rs:51-60`): con `!dock_visible`, CUALQUIER
`Enter`/`Motion`/`Press` sobre el blob hace `set_pointer(...)` + `reveal_dock(...)` en el
mismo handler, así que la isla nunca queda con el puntero encima. El clip del `scale` es
sub-pixel (26 px × 0,018 / 2 = 0,24 px por lado: invisible), así que el bloqueo es el
trigger, no el tamaño de la superficie. Dos salidas: **dwell** (revelar tras ~120 ms, y
antes se ve el lift) o **núcleo** (revelar sólo si el puntero cae en el centro del blob).

**Implementado con el dwell** (elección del usuario): el `Enter`/`Motion` del dock oculto
sólo guarda `island_hover_at` + arma un tick de `ISLAND_HOVER_MS` (120 ms); antes de vencer
la isla se dibuja con `HOVER_LIFT` (1,018, sólo el eje largo) y al vencer `island_hover_due()`
revela. Un `Press` revela ya; un `Leave` cancela. Verificado en vivo: reveal a los 121 ms
del `Enter`. Guards: `hover_due` y `el_hover_lift_alarga_la_isla_dibujada` (tinta del
bounding box).

**Y después el núcleo** (pedido del usuario): el reveal ya no es "todo el blob" sino su
**núcleo** (`island_core_region`: 40 % central del eje largo, piso 40 px, la MISMA cuenta
que la input region). El resto del blob es hover franco (lift) **más rueda = volumen y
tap = play/pause**. El dwell sólo corre dentro del núcleo. Verificado en vivo: dentro →
dwell + reveal; fuera → 0 dwells, 0 reveals y la rueda bajó el volumen 1.00 → 0.85.

### 4.2 Prioridad media · costo medio

#### H. Actividad de descargas

`read_dir("~/Downloads")` + `stat` de `*.part`/`*.crdownload` cada 1 s **mientras haya
algo** (mismo patrón que `thumbs_pending`: la condición del tick se apaga sola). Bytes +
velocidad por delta. Al desaparecer el `.part` y aparecer el final → "listo".
`xdg-user-dir DOWNLOAD` una vez al arrancar, no por tick.

**Riesgo de rendimiento:** un `read_dir` de Downloads es barato, pero el tick de 1 s
**no** puede quedar siempre encendido → gateado por "hay parciales". Costo: ~120 líneas
+ `has_widget`-equivalente (`settings.downloads_activity`). Guard: el tick se apaga.

#### I. Ask AI en el launcher

Launcher pregunta → `claude`/`codex` por CLI en un **hilo propio** (A3/A4: nunca en el
hilo que dibuja) → la respuesta vuelve por `IpcMessage::AskAnswer(String)` y se muestra
como vista expandida de la isla (o panel del overlay). Streaming línea a línea si el CLI
lo permite.

Costo: ~150 líneas (worker + estado + dibujo). Reusa el patrón del menú del tray.

#### J. Timer / Pomodoro

Backend barato (`Instant` + el tick del sistema), IPC `dockyrs --timer 25m`, UI de
entrada en el panel. Ya está en la lista de pendientes de AGENTS ("Isla dinámica, lo
que sigue" #8). Costo: medio (la UI de entrada es lo caro).

#### K. Custom widgets: `stream` + JSON + tooltip

Cerrar la brecha con `CustomBarModule`:

- `stream: bool`: mantener el proceso vivo y leer por línea (hilo propio + canal IPC).
- Salida JSON `{text, tooltip, icon, color}` (hoy sólo texto).
- Tooltips.

El `Custom(u16)` y `custom_widgets` ya existen, así que es extensión, no feature nueva.
Costo: ~150 líneas. **Ojo**: `stream` enciende un proceso permanente — gateado por
widget colocado, como `media_wanted`.

#### L. OSD como estado de la isla (escalón estructural) — ✔ HECHO y verificado en vivo

Junta dos pendientes de AGENTS (#3 y #6): hoy OSD/notificación/HUD **le piden prestada**
la superficie al dock (`layer_is_borrowed`), y por eso el dock desaparece cuando sale el
OSD. Migrarlos a `draw_island` + el motor de reveal los vuelve **estados de la misma
cápsula** y borra esa familia de bugs. La lección de `FullscreenOsd` (desmapear recién
al terminar la salida) es exactamente la inversa de la trampa 2 (nunca desmapear la
superficie compartida): con la isla **no** hay que desmapear nada, sólo animar a
`reveal = 0`.

Costo: medio-alto. Es el ítem que más simplifica el árbol a futuro.

**Implementado** (opción "píldora fina", elegida por el usuario): el OSD es una
cápsula del grosor del dock (largo `OSD_PILL_LEN` = 220) dibujada con `reveal_capsule` +
`draw_capsule` + `reveal_mask`. `show_osd` ya no hace `set_size` ni `Layer::Overlay`, y
`osd_mode` salió de `layer_is_borrowed`/`forces_dock_visible`. `menu_render::draw_osd`
sólo pinta el contenido (sin fondo propio). Se fue la animación (entra/sale con el timer).
Verificado con el dock oculto (sobre la isla) y visible (reemplazando el dock), sin
`configure` de tamaño nuevo ni cambio de layer. Guard:
`render::reveal_tests::el_contenido_del_osd_no_pinta_su_propio_fondo`.
De paso, `island_activity_dirty` pasó a `needs_repaint` (genérico) porque el OSD tenía la
misma carrera de "frame en vuelo".

#### M. Notificaciones: dedup y cap — ✔ HECHO y verificado en vivo

`avoidDuplicateNotifications` + `maxNotificationsInStack`. Docky ya tiene el cap de
historial como ajuste (`Notification History`); falta el **dedup** (misma
app+título+cuerpo en ventana corta no re-apila). Costo: ~30 líneas. Guard: dos avisos
iguales seguidos no duplican.

**Implementado**: `DockSettings::notif_dedup` (default prendido) + `SettingId::NotifDedup`
(fila "Avoid Duplicates" en Launcher). `show_notification` sale antes de
`push_notification` si el aviso es idéntico al último (función pura `notif_repetido`,
con test). Sin ventana de tiempo: compara sólo contra el último. Verificado con 5
`--notify` iguales → 1 toast, 4 líneas `notify: repetido` y una sola fila en el panel.

### 4.3 Baja prioridad / a evaluar

#### N. Notch (esquinas rectas + ears)

`island_notch` en Ajustes: `topLeftRadius/topRightRadius = 0` + dos arcos cóncavos
dibujados al lado. En Rust son dos paths; el dibujo del ear es trivial. Bajo valor, muy
barato. **Riesgo real**: el notch vive pegado al borde superior y en este setup el dock
está en `Left` (vertical) → no aplica. Implementar sólo si el dock va a `Top`.

#### O. Actividad de actualizaciones del sistema

El tracker de pacman es **específico de Arch**. Este repo documenta fallbacks de Void
(`zzz`) y Arch (`systemctl`), así que habría que detectar la distro y elegir: lock +
log de pacman, o `nixos-rebuild`, o nada. Convertido en "detectar herramienta y mostrar
fase" es ~200 líneas y mucha superficie de falso positivo. **Recomendación: posponer.**

#### P. Tooltips genéricos

Requiere una superficie flotante más (o reusar la del popup) y tracking de hover por
widget. Docky tiene el pill de SSID como precedente, pero generalizarlo toca todos los
widgets. Costo medio, valor medio.

#### Q. Visualizador de audio (cava)

En QML son 15 % de GPU; en Rust sería **CPU en cada frame** y obliga a mantener el loop
de frames vivo (contra el piso actual de ~1 tick cada 5–6 s). El propio
"Widgets descartados a propósito" de AGENTS ya marca como descartado lo que necesite
animación continua. **No portar** salvo pedido explícito, y si se hace, con `stream` de
cava y tope de fps + auto-apagado sin audio.

#### R. Weather / data usage

Ambos ya están en la lista de pendientes de AGENTS ("Widgets nuevos"). ChillPill confirma
la forma: weather con location/units/refresh y popup de pronóstico; data usage por
deltas de `/proc/net/dev`. No es específico de estos repos.

#### S. Wifi/Bluetooth con prompt de contraseña

Out of scope: en este setup el menú se delega al SNI (`open_widget_tray_menu`). Portar
la UI de conexión es un subproyecto.

---

## 5. Lo que NO conviene portar

| Idea de QML | Por qué no |
| --- | --- |
| Morph continuo con Behaviors sobre width/height/color | Acá el tamaño de la superficie se fija en `set_size`; el morph es dentro del pixmap (trampa 1). Mantener ese contrato. |
| Visualizador de audio 30 fps | Sube el piso de CPU; requiere loop de frames siempre vivo. |
| `PanelWindow` a pantalla completa con `mask` por región para el click afuera | Docky ya tiene el `click_catcher` (mismo resultado, sin GPU). |
| Render declarativo con `Variants`/`ListModel`/`FolderListModel` | Es el modelo de QML; en Rust se traduce a `read_dir` + ticks gateados. |
| Cava, espectro, blur, sombras | Implican GPU o costo por frame. |
| Layout flexible con `RowLayout`/anchors | Docky tiene `layout_widgets` + `WIDGETS`; no introducir un segundo motor de layout. |

**Regla de oro del port:** cada feature tiene que entrar por una de las **dos puertas** que
ya existen — **un `WidgetKind` nuevo** (el enum + la entrada en `WIDGETS`, `src/widget.rs`)
**o una actividad de isla** (`island_activities` + el `note_*`/`refresh_*` correspondiente).
Nada de un tercer camino. Sobre el gate: `refresh_*` **gateado por `has_widget` sólo si
corre en el tick**; lo event-driven (niri, D-Bus) se lee siempre, porque el evento ya llegó.

---

## 6. Orden sugerido (incremental, todo verificable)

1. ✔ **A** (motor de actividades) — hecho; habilita C, D, E, H, I sin tocar el render de
   nuevo.
2. ✔ **B** (prueba del motor), ✔ **C** (portapapeles), ✔ **D** (bluetooth), ✔ **E**
   (HUD de teclado), ✔ **M** (dedup de avisos) — hechas. El gating quedó resuelto
   desgateando las dos fuentes event-driven (batería/volumen/red siguen gateadas: corren
   en el tick). ✔ **L** (OSD como estado de isla) — hecho, el escalón estructural.
3. ✔ **F** (ritmo por tiempo, no por frame) y ✔ **G** (hover lift con dwell de 120 ms) —
   hechos y verificados en vivo. Y el follow-up de G, ✔ el **núcleo**: el reveal lo dispara
   sólo el centro del blob, y el resto es hover + **rueda = volumen / tap = play/pause**.
4. ✔ **Fuera del informe, ya hecho**: **Captura guardada** y **Compartiendo pantalla**, dos
   actividades más del event-stream de niri (`ScreenshotCaptured`/`CastsChanged`).
5. **H** (descargas) — el primer ítem con tick nuevo; patrón `thumbs_pending`.
6. **I** (Ask AI) — el primer ítem con worker de CLI nuevo; patrón del tray worker.
7. **Baratos y del event-stream que YA tenemos** (no estaban en el informe): `ConfigLoaded`
   (re-leer tema/fondo), `OutputsChanged` (perfiles por salida), `FocusedWindow` (título de
   la ventana en la isla), `PickColor` (tomar el acento de un píxel desde Ajustes→Colors) y
   el screenshot nativo de niri (reemplaza el selector de región propio: **borra código**).
8. Después: **J/K/P** y **DND** según ganas; **N** sólo si el dock va a `Top`; **O/Q/R/S** no.

Tests: cada ítem deja **un** guard (los módulos `*_tests` ya siguen ese patrón) y se
verifica a mano con `scripts/pointer.py` + `niri msg --json layers` + las líneas de log,
como documenta AGENTS.md. Antes de dar algo por hecho: `cargo build --release` (trampa 5),
`cargo test --release` (trampa 14: corre el workspace) y `cargo clippy --release
--all-targets`.

---

## 7. Verificación de rendimiento (cómo no romper lo que ya es bueno)

Al cerrar cada ítem, medir contra el baseline actual:

- **CPU en reposo**: `cutime+cstime` de `/proc/<pid>/stat` (campos 16-17) en ventanas de
  6 s. Objetivo: seguir en ~1 tick. Cualquier actividad nueva que encienda el loop de
  frames en reposo es una regresión aunque "se vea linda".
- **RSS**: `smaps_rollup` del dock + los hijos (`niri msg` ya no se lanza: habla por
  `$NIRI_SOCKET`; `pactl subscribe` 5,6 MB; `playerctl --follow` 7 MB **sólo con Media
  colocado**).
- **Superficies**: `niri msg --json layers` — la isla no debe agregar superficies; los
  popups/toasts son descartables y se sueltan al cerrar.
- **Frames por cambio**: `WAYLAND_DEBUG=1` contando `attach` (lo que documenta la
  sección `smooth_transitions` de AGENTS).

---

## 8. Apéndice: mapa de archivos a tocar

| Área | Archivos |
| --- | --- |
| Actividades de isla | `src/app/island_activity.rs` (`IslandActivities`, los `note_*`, la transición), `src/render/mod.rs` (`island_activities`, `island_spans`, `island_core_region`), `src/render/layout.rs` (`island_plan`), `src/app/draw.rs` (`draw_island`, ticks) |
| Estado de la app | `src/app/mod.rs` (`island_activity`, `battery_activity`/`bluetooth_activity`, `needs_repaint`, `island_hover_at`, `frame_dt_ms`) |
| Widget nuevo | `src/config.rs` (`WidgetKind`) + `src/widget.rs` (`WIDGETS`: medida, dibujo y click). Son **2 lugares**: el orden y las etiquetas de Ajustes salen de la tabla |
| Event-stream de niri | `src/ipc.rs` (`niri_mensaje` + `IpcMessage`) y el `note_*` en `src/app/island_activity.rs` |
| Lecturas nuevas | `src/widgets.rs` (`read_*` con `run_with_timeout`/`stat`), `src/ipc.rs` si hay worker |
| IPC nuevo | `src/ipc.rs` (`IpcMessage`), `src/main.rs` (parseo de `--flag`) |
| Ajustes | `src/config.rs` (`DockSettings`), `src/menu/settings.rs` (`SettingId`) y la fila en `src/menu/dock_menu.rs` |
| Animaciones | `src/menu/mod.rs` (`Pace`, `anim_step`, `anim_towards`, `lerp_factor`) |
| Tests | el `mod *_tests` del módulo tocado |

---

## 9. Ideas sin minar de los dos repos

Sección agregada **después** de la propuesta original, al releer §2 contra lo que ya está en
el árbol. Nada de esto se implementó: son las features de las dos fuentes que **no** figuran
en §4 ni como “ya está”. Verificado contra el código que Docky hoy **no** las tiene (no hay
emoji picker, ni búsqueda de keybinds, ni preview grande del portapapeles, ni panel-lista
del tray, ni confirmación en el menú de energía, ni eventos de calendario, ni ajustes de
velocidad/tope/duración).

### 9.1 De island

| Idea | Qué es | Costo | Riesgo |
| --- | --- | --- | --- |
| **Búsqueda de keybinds** | Una pestaña del overlay que lista y **filtra los atajos** de niri (`KeybindList.qml`) | ~120: parsear `~/.config/niri/config/binds/*.kdl` (el repo ya parsea `.kdl` para `wallpaper_program()`) | niri **no** expone los binds por IPC: hay que leer el archivo del usuario |
| **Panel-lista del tray** | Una vista con los items del SNI y sus menús (`TrayList.qml`) | ~150 | los datos ya están (`tray.rs`), pero hay que listar y navegar |
| **Emoji picker** | Pestaña que busca y copia emojis (`EmojiPicker.qml`) | ~150 | **bloqueo duro**: el canvas rasteriza texto con una fuente normal; sin una **fuente de emoji en color** saldrían cuadraditos. Verificar **antes** de empezar |
| **`motionScale`** | Multiplicador de la **velocidad** de todas las animaciones (`IslandSettings.qml`). Docky sólo tiene `smooth_transitions` (on/off) | ~20: multiplica las duraciones de `menu::Pace` | — |
| **Duración del aviso** | `bannerSeconds` (island) / `notificationDisplayTime` (ChillPill). Hoy el timeout del toast es fijo (4 s + 0,9 s por línea) | ~15: una fila más, y que el label la lea | — |
| **SoundWave / SiriDots** | Onda animada con la música y puntitos de “pensando” | ~80 | **descartado por CPU**: necesita el loop de frames vivo, la misma razón que cava (Q) |

### 9.2 De ChillPill-Shell

| Idea | Qué es | Costo | Riesgo |
| --- | --- | --- | --- |
| **VPN como estado** | Widget/indicador de VPN (como Network) — `Vpn.qml` | ~40: `nmcli`/`wg` + `read_*` en el tick | la fuente depende del setup (NetworkManager vs WireGuard) |
| **Portapapeles: preview grande + multi-select** | `Tab` para ver la imagen/texto completo, `Shift+↑↓` rango, `Shift+Space` elegir y `Del` borrar varios (`Cliphist.qml`) | ~200 | es la que más se siente en el uso diario y la más cara; hoy hay miniaturas y borrado de a uno |
| **Calendario con eventos** | Además del mes, marcar los días con eventos (`CalendarBox.qml` + `scripts/calendar_events.py`) | ~150 | hace falta una **fuente** de eventos (el script del repo); hoy `menu/calendar.rs` es sólo aritmética de fechas |
| **Confirmación en el menú de energía** | `confirmPowerActions`: un paso extra antes de apagar/reiniciar | ~40: un estado más en el popup de energía | — |
| **`maxVolume`** | Tope del volumen (barra, rueda y pasos) | ~20 | — |
| **`showSensitiveInfo`** | Ocultar el SSID (y la IP) por defecto | ~15 | — |
| **Popup de media automático** | `mediaPopupDuration`: al empezar a sonar, abrir el reproductor solo | ~30 | hoy se abre por click; ojo con no pelearse con el autohide |
| **`IpStatus`** | IP local en la barra | ~30 | lo cubre el pill de Network con un campo más |

### 9.3 Lo que ya está (no re-portar)

De §4: Timer (J), Tooltips (P), Weather/DataUsage (R), Descargas (H), Ask AI (I), cava (Q),
Notch (N), panel wifi/BT con contraseña (S), wallpaper switcher, menú de energía, portapapeles
(con miniaturas), notificaciones con dedup, panel de ajustes, launcher y control center.

### 9.4 Por dónde empezar

1. **`motionScale` + duración del aviso** — dos ajustes, ~35 líneas juntos, y cierran una
   asimetría: ya está el *on/off* de las transiciones (`smooth_transitions`) pero no la
   **velocidad** ni la duración del aviso.
2. **Búsqueda de keybinds** — producto nuevo y barato, del mismo tipo que el launcher.
3. **Portapapeles: preview grande + multi-select** — la más útil en el uso diario, la más
   cara (~200 líneas).

**Antes de comprometerse con el emoji picker**: verificar que el canvas pueda pintar emoji
en color (fuente + soporte del rasterizador). Es el único bloqueo duro de la lista.
