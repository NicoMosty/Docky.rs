# `master` vs `niri-backend` — diferencias entre las dos ramas

Informe generado el **2026-09-20** sobre el fork `NicoMosty/Docky.rs`.

| | rama | tip | qué es |
| --- | --- | --- | --- |
| mainline | **`master`** | `8b52360` *Update README.md* | espejo exacto de `upstream` (`Volcha-Z/Docky.rs`) |
| trabajo | **`niri-backend`** | `4ac3013` *rustfmt: parte la firma de `overlay_tab_at`…* | rama de trabajo, `origin` del fork |

```sh
# reproducir este informe
git fetch upstream origin
git log --oneline --reverse master..niri-backend        # qué agrega mi rama
git log --oneline --reverse niri-backend..master        # qué tiene master y mi rama no (0)
git diff --stat master niri-backend
```

`merge-base master niri-backend` = **`8b52360`**, o sea el tip de `master`: la rama de
trabajo es **lineal** sobre la mainline, sin commits de `master` sin mergear (0 detrás).
`master` no tiene ni un commit propio: es el espejo, tal como documenta `AGENTS.md`.

## 1. Números

| métrica | `master` | `niri-backend` | diff |
| --- | --- | --- | --- |
| commits | — | +46 | 46 adelante, 0 atrás |
| archivos `.rs` | 70 | 88 | +18 |
| líneas `.rs` | 12 843 | 32 206 | +19 363 |
| tests (`#[test]`) | **0** | **147** (en 37 archivos) | +147 |
| archivos tocados por el diff | — | 100 | +26 287 / −2 756 |
| dependencias de terceros nuevas | — | **0** | `Cargo.lock` sólo suma `dockyrs-canvas` (path local) |
| crates | 1 | 2 (workspace) | `crates/dockyrs-canvas` |

Lo más grande del diff: `AGENTS.md` (+1862), `AUDIT.md` (+1518), `src/widgets.rs`
(+1455/−77), `src/render/mod.rs` (+1241/−58), `src/widget.rs` (+1197, nuevo),
`src/render/layout.rs` (+888/−130), `src/app/draw.rs` (+778/−30),
`src/app/dock_menu.rs` (+624/−74), `src/menu/app_search.rs` (+545/−36),
`src/render/syswidgets.rs` (+539, nuevo), `src/app/dock_menu_input.rs` (+525/−108),
`src/config.rs` (+525/−27), `src/main.rs` (+518/−93), `src/app/dock_popup.rs`
(+515/−69), `src/menu_render/mod.rs` (+459/−50), `src/app/mod.rs` (+437/−49),
`src/ipc.rs` (+422/−48), `src/app/pointer.rs` (+370/−52).

Lo que más se **reescribió** de upstream (líneas borradas): `src/app/app_search.rs`
(−134), `src/render/layout.rs` (−130), `src/app/dock_menu_input.rs` (−108),
`src/menu_render/app_search.rs` (−103), `src/main.rs` (−93),
`src/menu_render/clipboard.rs` (−82), `src/app/wallpaper_picker.rs` (−82),
`src/app/clipboard_ui.rs` (−78), `src/widgets.rs` (−77), `src/app/dock_menu.rs` (−74).

## 2. Diferencias por área

### 2.1 Compositor: niri además de Hyprland

El cambio de fondo del fork. `src/compositor.rs` (**nuevo**, 25 líneas) tiene
`enum Compositor { Hyprland, Niri }` y `detect()`, que corta por variable de entorno
(`HYPRLAND_INSTANCE_SIGNATURE` / `NIRI_SOCKET`, que los dos compositores exportan) y
deja el sondeo `niri msg` como último recurso. **El código de Hyprland sigue intacto**:
niri es un backend más, no un reemplazo.

- Workspaces **por output**: cada instancia se ancla a una salida
  (`--output DP-2 --profile dp`) y filtra los workspaces de esa salida
  (`09a2106`, `1dce606`), con clic que enfoca monitor + workspace.
- Estado en vivo por el **event-stream** de niri sobre `$NIRI_SOCKET`
  (`UnixStream`, JSON por línea) en vez de lanzar `niri msg …` cada 2 s: −15 MB de
  proceso hijo y layout de teclado / urgencia / Overview / workspaces instantáneos
  (`4f467db`, `17491b9`).
- Widgets que antes no existían: `KbdLayout`, `Network`, `Volume` (paridad con waybar),
  además de urgencia del workspace en rojo.

### 2.2 Modos y paneles — el grueso del fork

- **Autohide sin reservar espacio** (`2ec0ed1`): la superficie del dock es el disparador
  (`set_exclusive_zone(-1)`, buffer transparente al ocultar; nunca se desmapea).
- **HUD de workspaces** al cambiar de workspace, alineado con el indicador del dock
  (`2a0276c`, `b9bf354`).
- **Panel de ajustes** debajo del dock, con drag de widgets y navegación por teclado
  (`bd8e40e`, `0bb88fa`, `7f0cae9`).
- **Overlay** con pestañas Apps / Clipboard / Notifs / Wallpapers / Windows:
  paneles debajo del dock en los cuatro bordes, ancho y (en vertical) **alto común**,
  fundido/deslizamiento entre pestañas, cierre con click afuera
  (`40e4328`, `581b0dd`, `aa857e7`, `0559a45`, `664fcdb`).
- **Launcher tipo rofi**: historial de uso, matching difuso, `Keywords`, `Name[es]`,
  `Terminal=true`, filtro `OnlyShowIn` (`92454d5`).
- **Panel de volumen** con filas por app, arrastre, mute y selector de salida
  (`2d91b51`).
- **Calendario del reloj** por hover (`0bb88fa`).
- **Panel de notificaciones** + toast en superficie propia (`769acf4`).
- **Isla dinámica**: el dock se encoje y crece con actividades adentro (reloj, batería,
  media, grabación, indicador al partirse) (`769acf4`).
- **Click-catcher**: click afuera cierra el overlay, porque `wlr-layer-shell` no tiene
  pointer grab (`40e4328`).

### 2.3 Widgets

`src/widget.rs` (**nuevo**, 1197 líneas) y `src/render/syswidgets.rs` (**nuevo**, 539)
reemplazan los `match` paralelos de upstream por una **tabla** (`WIDGETS` en
`render/widget_spec.rs`): agregar un widget son 2 lugares (el enum y la tabla) en vez de
~16 sitios en 7 archivos (`cfcbca3`, `b07f969`, `7f0cae9`, `c66dd38`). Widgets nuevos:
`Custom` (script + pastilla de texto), `Mic`, `Recording`, además de los ya nombrados.
El reloj, la batería y el volumen se rehicieron (píldoras, `%` adentro, rojo ≤20 %,
nub del polo, reloj vertical en una sola columna).

### 2.4 Arranque, perf y memoria (todo medido y documentado en `AGENTS.md`)

- Arranque en frío: lecturas del sistema en paralelo + `refresh_quick` con las cuatro
  lentas diferidas por IPC (`8ec8757`). Mediana 0,167 s → 0,076 s en caliente.
- Watchers gateados por widget colocado (`4f467db`, −7 MB) y **caché de miniaturas en
  memoria + disco**: 1370 ms → 20 ms por visita al selector (`264f184`).
- Timeouts en todo lo que puede colgarse: lecturas (`run_with_timeout`), `matugen`,
  `curl` de carátula (`--fail`, `--max-time 1`), y `method_timeout` de 500 ms en la
  conexión D-Bus del tray.
- Reposo: ~1 tick de CPU cada 5-6 s y ~13,7 MB de RSS (por el raster por CPU del crate).

### 2.5 Raster extraído a un crate

`crates/dockyrs-canvas` (**nuevo**): `text.rs`, `icon_cache.rs` (renombrado desde
`src/icon_cache.rs`), `thumbnail_cache.rs` (movido desde `src/`), más
`crates/dockyrs-canvas/Cargo.toml`. Es el único `Cargo.toml` nuevo del fork y no agrega
dependencias: se lleva `resvg`/`usvg`/`tiny-skia`/`image`/`freedesktop-icons`.
Los originales `src/text.rs` y `src/thumbnail_cache.rs` se **borran** en la rama.

### 2.6 Robustez / auditoría

- **Reexec ante fallo fatal** (`catch_unwind` + relanzado con tope de 3) (`51003dc`).
- **IPC con un hilo por cliente**: un cliente mudo ya no llena la cola de `accept` y
  deja muda toda la IPC (`efbbb1a`).
- **Tres panics alcanzables cerrados** y llamadas D-Bus/red sacadas del hilo que dibuja
  (`769acf4`).
- `AUDIT.md` (**nuevo**, 1518 líneas): la auditoría con severidades y estado
  (qué está cerrado y qué queda, hoy `B1` y parte de `A5`).

### 2.7 Documentación y herramientas de prueba

- `AGENTS.md` (**nuevo**, 1862 líneas): mapa del código, las **trampas numeradas** (11
  items en la lista, con referencias hasta `trampa 17`: salieron de las sesiones de
depuración, cada una con su medición) y el "qué se hizo" cronológico. Es el documento
  que hace operable el fork.
- `README.md`: +36/−7 — se declara fork, se agrega "Hyprland o niri", autostart en
  niri, cómo correr los tests, el toggle **Matugen Apps** y **Light Mode**. Fuera de
  eso el README sigue siendo el de upstream.
- `niri-config.kdl.example` (**nuevo**): cómo arrancar el dock con una o varias
  salidas (`--output` + `--profile`).
- `scripts/` (**4 nuevos**): `pointer.py` (puntero virtual por uinput; es el sensor de
  casi todas las verificaciones), `click_at.py` (click en coordenada absoluta),
  `sweep_vertical.py` (dock lateral) y `fake_sni_hang.py` (un SNI del tray que nunca
  contesta `GetLayout`).
- `.gitignore`: `target/`, `.pi/tasks/`, `.pi/delegate/`, `__pycache__/`.

### 2.8 Manifiesto (`Cargo.toml`)

```diff
+ [workspace]
+ members = ["crates/dockyrs-canvas"]
+ default-members = [".", "crates/dockyrs-canvas"]   # sin esto, `cargo test` en la raíz no corre los del crate
+ [profile.dev]        opt-level = 1                  # rebuilds rápidos
+ [profile.dev.package."*"] opt-level = 3             # tiny-skia/resvg sin optimizar son ~20-50x más lentos
+ [profile.release]    strip = true                   # −2,75 MB, y NO `panic = "abort"` (rompe el reexec)
- calloop, calloop-wayland-source                    # declaradas sin uso
- freedesktop-icons, resvg, usvg                     # se mudan a crates/dockyrs-canvas
```

`Cargo.lock`: 15 líneas agregadas / 34 borradas, **una sola entrada nueva**:
`dockyrs-canvas` (path local). Cero dependencias de terceros nuevas.

### 2.9 Archivos nuevos de la rama

```
AGENTS.md  AUDIT.md  niri-config.kdl.example
crates/dockyrs-canvas/{Cargo.toml, src/lib.rs, src/text.rs, src/icon_cache.rs, src/thumbnail_cache.rs}
scripts/{pointer.py, click_at.py, sweep_vertical.py, fake_sni_hang.py}
src/compositor.rs
src/usage.rs
src/widget.rs
src/render/syswidgets.rs
src/app/{calendar.rs, click_catcher.rs, notifications_ui.rs, volume_panel.rs, ws_flash.rs}
src/menu/{calendar.rs, keynav.rs, notifications.rs, volume_panel.rs}
src/menu_render/{calendar.rs, notifications.rs, tabs.rs, volume_panel.rs}
```

## 3. Los 46 commits

```
09f7a85 2026-09-11 Niri backend: dual Hyprland/Niri compositor support
c458821 2026-09-11 Waybar parity: Network/Volume/KbdLayout widgets, GB ram, clicks, waybar order
42da0b3 2026-09-11 Quitar log temporal del repo e ignorar .pi/tasks
09a2106 2026-09-11 Per-output docks: --output pin + --profile instances, first-configure draw guard
1dce606 2026-09-11 Workspaces por monitor: filtrar por output anclado, clic enfoca monitor+workspace
757d961 2026-09-11 Fix ws primera pasada: tracking fuera del draw + barra compacta minimalista
2ec0ed1 2026-09-12 Autohide, HUD de workspaces y pulido de barra y menús
8336f9c 2026-09-12 Quitar .pi/tasks del indice: es estado de sesion del agente, no del repo
bd8e40e 2026-09-12 Panel de ajustes debajo del dock y arreglo del reordenamiento de widgets
92454d5 2026-09-12 Fondos, launcher tipo rofi y cambio de modo con Shift+flechas
2d91b51 2026-09-13 Panel de volumen, pestañas del overlay y tres arreglos de la auditoría
51003dc 2026-09-13 Recuperación ante fallo fatal: el dock se relanza solo (A6, A5)
efbbb1a 2026-09-13 IPC: un cliente colgado ya no bloquea el listener (B3)
7bcdc19 2026-09-14 Binario: strip y dos dependencias declaradas sin uso
264f184 2026-09-14 Selector de fondos: el pico de memoria era una copia a resolución completa
d3f0ddc 2026-09-14 Extraer el raster a crates/dockyrs-canvas
cfcbca3 2026-09-14 Widgets: una tabla de despacho para que agregar uno no sea cirugia de 7 archivos
b07f969 2026-09-14 Widgets: cerrar la migracion -- los dos `match` desaparecen
4cb32f9 2026-09-14 AGENTS.md: la tabla de widgets y las dos trampas del workspace
7f0cae9 2026-09-14 Widgets: el click tambien pasa por la tabla
c66dd38 2026-09-14 Widgets: la etiqueta y el orden salen de la tabla
760e4b0 2026-09-14 Widgets: entrada genérica Custom con script y pastilla de texto
0bb88fa 2026-09-14 Editor de opciones por widget, calendario del reloj y teclado en los menús
40e4328 2026-09-15 Overlay: paneles debajo del dock, ancho único y click afuera
4f467db 2026-09-15 El tick deja de lanzar `niri` y el watcher de media se apaga sin widget
0823d47 2026-09-15 AGENTS.md: documenta el trabajo acumulado y las trampas nuevas
f88d41f 2026-09-16 Matugen del usuario para las apps, modo light y scheme-smart
17491b9 2026-09-16 El Overview de niri deja el dock fijo en pantalla
2a0276c 2026-09-16 El indicador del HUD de workspaces mide igual que el del dock
b9bf354 2026-09-18 Alinea el HUD de workspaces con el indicador del dock
8ec8757 2026-09-18 El arranque en frio ya no espera las lecturas del sistema
b585983 2026-09-18 Barra vertical: el reloj y el nub de la bateria ya entran
581b0dd 2026-09-18 Paneles del overlay adaptados al dock vertical
aa857e7 2026-09-18 El dock queda a la vista con el panel abierto en los CUATRO bordes
3b76590 2026-09-18 Documenta el trabajo acumulado y las trampas nuevas
769acf4 2026-09-20 Panel de notificaciones, isla dinamica y el tray/media fuera del hilo
4066e96 2026-09-20 AGENTS.md y AUDIT.md: el trabajo de notificaciones, isla y las trampas nuevas
f57d58f 2026-09-20 El toast de notificaciones ya no se queda mapeado si otro panel lo pisa
c4a257a 2026-09-20 AGENTS.md: el toast que quedaba mapeado y los tres caminos viejos
0559a45 2026-09-20 En el dock vertical los cuatro paneles del overlay comparten el alto (640)
24ce0a2 2026-09-20 AGENTS.md: el alto comun del overlay en el vertical
c1d60f6 2026-09-20 rustfmt: parte la suma de fixed_h en el launcher
664fcdb 2026-09-20 La banda de pestañas del overlay es clickeable
70dc70b 2026-09-20 AGENTS.md: la banda de pestañas clickeable
1e6d868 2026-09-20 scripts/click_at.py: click en una coordenada absoluta
4ac3013 2026-09-20 rustfmt: parte la firma de overlay_tab_at y una aserción de la banda
```

Por etapa: **11-13 sep** el backend de niri y los cimientos (paridad, per-output,
autohide, panel de ajustes, launcher, volumen, reexec, IPC); **14 sep** el crate del
raster y la tabla de widgets; **15-18 sep** el overlay (paneles, click afuera, vertical,
HUD alineado, arranque en frío); **20 sep** notificaciones, isla dinámica, los llamados
que pueden colgarse fuera del hilo, el alto común y la banda clickeable.

## 4. Compatibilidad y deuda

- **Hyprland no se rompe**: `src/compositor.rs` sólo elige backend; las ramas de
  Hyprland de `src/widgets.rs` / `src/ipc.rs` siguen ahí y `master` no tiene código que
  se haya borrado sin reemplazo funcional (las dos bajas de archivos son mudanzas al
  crate).
- **Config compatible**: `~/.config/dockyrs/config.json` sigue funcionando (los campos
  nuevos entran con `serde(default)`); con `--profile` se usa
  `config-<perfil>.json`.
- **Sincronizar con upstream**: `master` es espejo, así que el merge esperable es
  `git merge master` dentro de la rama. Los conflictos van a caer en los archivos más
  reescritos (§1): `app_search.rs`, `render/layout.rs`, `dock_menu_input.rs`,
  `menu_render/app_search.rs`, `main.rs`, `clipboard.rs`, `widgets.rs`, `dock_menu.rs`.
- **Deuda visible**: el README quedó a medio camino (mezcla inglés del upstream con
  bloques nuevos en castellano y no menciona notificaciones, isla, ni el alto común);
  `AUDIT.md` mantiene abiertos `B1` (auto-repeat de `Shift+flecha` cicla los modos en
  bucle) y parte de `A5`; los tests son **todos** del fork (upstream no trae ninguno),
  así que cualquier merge tiene que respetar `default-members` para no perderlos.
- **Costo estructural aceptado**: `cargo test --release` en la raíz cambió de alcance
  (ahora incluye el crate) y `[profile.dev.package."*"]` **no** alcanza a los miembros
  del workspace (`dockyrs-canvas` se compila en `opt-level = 1`): las dos cosas están
  documentadas en `Cargo.toml` y en `AGENTS.md`.
