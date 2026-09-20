# AUDIT.md — Auditoría general de Docky.rs

> Auditoría del código y de las implementaciones del repo, pensada para que **otro
> agente la ejecute**. Cada hallazgo trae archivo:línea, evidencia citada, impacto,
> el fix mínimo y cómo verificarlo. No hay refactors especulativos: si un hallazgo
> no se pudo verificar, está marcado como tal.
>
> Fecha: 2025-09-13. Alcance: `src/` completo (22.5k líneas, 78 archivos `.rs`),
> `scripts/`, `install.sh`, `README.md`, `AGENTS.md`. Base: rama `niri-backend`.
>
> **Este documento lista SÓLO lo que sigue abierto.** Los hallazgos ya cerrados
> (A1, A2, A3, A4, A6, A7, B1, B2, B3, B10, C3, C4, C6, C8, D1, D2, D3, D9, D10, D11,
> D12 y D13), los de la Ronda 2 (B4, D4, D5, D6, D7, D8) y los de la Ronda 3 (B7, B8,
> B11, B12, C2, C7) se movieron a `AGENTS.md` → “Cerrado de AUDIT.md”, con lo que se hizo
> y cómo se verificó; los números de sección que faltan abajo son justamente esos. Queda
> **C5** a medias y las **decisiones** de §3 (A5 en parte, B6, C1).

---

## 0. Contrato para el agente que implemente

1. **Leer `AGENTS.md` antes de tocar nada.** Las "trampas" numeradas son invariantes
   reales, no sugerencias. La §10 de este documento las resume como checklist.
2. **Un hallazgo = un cambio atómico + su guard.** El repo tiene una convención
   fuerte: cada bug corregido deja un test que falla si vuelve la regresión
   (`render::layout::hit_layout_tests`, `app::draw::popup_dismiss_tests`,
   `app::pointer::wheel_tests`, `tray::tray_ignore_tests`, …). Respetarla.
3. **No commitear en `master`.** `master` es espejo exacto de `upstream/master`;
   todo va a `niri-backend`.
4. **Compilar y verificar, no confiar en diagnósticos cacheados** (trampa 7):
   `cargo build --release` + `cargo test --release` son la autoridad.
5. **No editar la config de niri del usuario** (`~/.config/niri/…`).
6. **Un hallazgo cerrado SE MUDA**: se borra de este documento y va a `AGENTS.md` →
   “Cerrado de AUDIT.md”, con lo que se hizo, la evidencia y el commit. Este documento
   lista **sólo lo abierto**: si algo queda acá, es que falta. (Así se hizo el 2026-09-20,
   que movió 13 hallazgos en la limpieza, 9 al cerrar la Ronda 1, 6 en la Ronda 2 y 6 en
   la Ronda 3.) Si un hallazgo queda abierto **a propósito**, no se borra: pasa a
   “Decisiones” en §3 con el porqué, para que se lea como elección y no como deuda.
7. **Estado del árbol de trabajo** (ver §2): limpio y pusheado. Si encontrás algo sin
   commitear, primero mirá si `AGENTS.md` lo describe: puede ser trabajo a medias de
   otra sesión. No lo reviertas sin leerlo.
8. El orden de implementación sugerido está en **§8** (los bloqueos del event loop y la
   geometría ya se cerraron; lo que queda es churn, E/S y limpieza).

### Alcance y cobertura

Alcance: `src/` completo (78 archivos, 22.5k líneas), `scripts/`, `install.sh` y docs. Cada hallazgo se verificó contra el código; `cargo test` (37 pass) y `cargo clippy` (17 warnings) se re-ejecutaron sobre este árbol. Al 2026-09-20 el árbol tiene **147 tests** y **16 warnings** de clippy (18 contando el target de tests, todos preexistentes).

Cobertura más débil (leída por encima): la matemática interna de `menu/theme_picker.rs`, `menu/controls.rs` y `menu/widget_chips.rs` — la pasada sobre ellas no encontró nada.

Lo revisado **sin hallazgos** está en el Anexo §7 (verificado, sin acción): no mezclar con lo pendiente (§§3–6).

---

## 1. Evidencia recolectada

| Verificación | Resultado |
| --- | --- |
| `cargo test --release` | **37 passed, 0 failed** (bin `dockyrs`; `dockyrs-notifyd` 0 tests) |
| `cargo clippy --release --all-targets` | **17 warnings, 0 errores** en el bin (12 × `too_many_arguments`, + `unnecessary_find_map`, 2 × `needless_range_loop`, `manual_clamp`, `collapsible_match`; el build de tests suma `items_after_test_module` y `field_reassign_with_default`) |
| Inventario | 78 archivos `.rs`, 22 521 líneas; el más grande `src/widgets.rs` (1055) |
| Superficies layer | **4** `create_layer_surface` (dock, menú de icono, popup, screenshot) |
| Hilos | ~21 `std::thread::spawn`; 5 `static` compartidos (`tray::CONN`, `PINNED_OUTPUT`, `widgets::LAST`, `text::OPTS`, `text::FAMILIES`) |
| Subprocesos | 60+ `Command::new(...)`; `run_with_timeout` en 8 sitios |
| `unwrap()/expect()` | 94 ocurrencias; las peligrosas listadas en la tabla de A5 |
| `TODO/FIXME/HACK` | 0. Sólo 6 `ponytail:` documentados |

Comandos exactos:

```sh
cargo test --release
cargo clippy --release --all-targets
rg -n 'thread::spawn|Command::new|\.expect\(|\.unwrap\(\)' src/
```

---

## 2. Estado del repositorio (leer antes de empezar)

El árbol está **limpio** y todo el trabajo hasta la fecha está commiteado y pusheado en
`niri-backend` (lineal sobre `master`, que es espejo de `upstream`). Antes de empezar:

```sh
cargo build --release   # SIEMPRE antes de reiniciar: `cargo test` no refresca el binario
cargo test --release    # 147 tests; incluye el crate del raster (workspace)
```

Si `git status` muestra algo sin commitear cuando retomes este documento, **no** lo
descartes: puede ser trabajo a medias de otra sesión (mirá si `AGENTS.md` lo describe
antes de tocarlo).

---

## 3. Resumen ejecutivo

**Pendientes: 2 hallazgos** (B5, MEDIA, y C5, BAJA y a medias con los tests de
`Dock::drag_to`/`icon_at`). Lo cerrado está en `AGENTS.md` → “Cerrado de AUDIT.md”,
**34 hallazgos** con la evidencia de cómo se verificó cada uno.

| ID | Sev. | Título | Archivo |
| --- | --- | --- | --- |
| **B5** | MEDIA | Conexión D-Bus del tray cacheada sin reconexión → tray muerto permanente | `tray.rs:187` |
| **C5** | BAJA | Huecos de tests: quedan los de drag/`icon_at` | §6.5 |

### Decisiones (no se hace, con el porqué)

Estos tres quedan **abiertos a propósito**: no son trabajo pendiente, son el estado que
eligió el repo. Si algún día cambia el supuesto, se retoman.

| ID | Por qué no se hace |
| --- | --- |
| **A5** (parte) | El `catch_unwind` cubre el dispatch y no los drenajes de IPC posteriores al `match`: `main.rs` documenta que ahí se arregla **en la fuente**, no envolviendo más código. Y los ~60 `.lock().unwrap()` no son alcanzables: un panic mientras se tiene el lock ya disparó el `reexec` (o mató el proceso), así que el veneno no sobrevive. |
| **B6** | `repo_dir()` sale de la ruta del binario y sólo afecta a los `sync-*.sh` de theming, que en este setup no se usan (el dock corre desde el repo). El fix real sería un `warn` cuando no encuentra los scripts. |
| **C1** | Pasar `DrawArgs` a todo `render/` son ~57 sitios para no prevenir ningún fallo: la invariante ya vive en el adaptador (`hit_layout`). Medido y descartado. |

---

## 4. Severidad ALTA

Los números que faltan (4.1–4.6 y 4.8) son hallazgos **ya cerrados**: ver `AGENTS.md` →
“Cerrado de AUDIT.md”. Queda uno, y sólo en parte: el listado de abajo describe el estado
**original**, y el guard de arriba dice qué falta.

---

### 4.7 — Un panic mata el dock, sin recuperación {#a5}

> **Parcialmente resuelto.** Cerrados los 3 panics alcanzables (ver `AGENTS.md` →
> “Cerrado de AUDIT.md”):
> `percent_decode` decodifica sobre bytes, el buffer oculto de `draw_hidden` ya no
> paniquea, y el click de apps usa `icons.get()`. Además hay `catch_unwind` en el
> dispatch con `reexec()`. **Lo que sigue abierto**: los `catch_unwind` NO cubren
> los drenajes de IPC posteriores al `match` en `main.rs`, y los `.lock().unwrap()`
> (≈60) siguen ahí. El listado de abajo describe el estado **original**.

No hay `catch_unwind`, no hay supervisor, no hay reinicio. niri arranca dockyrs con
`spawn-at-startup`, que **no lo reinicia**. Sitios alcanzables en runtime:

| Archivo:línea | Código | Cuándo |
| --- | --- | --- |
| `widgets.rs:906-921` | `&s[i + 1..i + 3]` en `percent_decode` | **panic real**: `%` seguido de un carácter multibyte (ver D2) |
| `app/draw.rs:105,111,141,175` | `.expect("failed to create/attach … shm buffer")` | ENOSPC / OOM de shm |
| `app/handlers.rs:195,201` | `.expect("get pointer")` / `.expect("get keyboard")` | el seat retira la capability entre el evento y la llamada |
| `app/pointer.rs:367` | `self.dock.icons[idx].app.exec` | `idx` de `press_icon_index` sin revalidar (índice directo, sin `.get`) |
| `app/draw.rs:151`, `tray.rs:90,100,364`, `app/dock_menu.rs:273`, `app/volume_panel.rs:24` | `.lock().unwrap()` | **cascada**: un panic en el hilo D-Bus del tray envenena el mutex y el próximo frame paniquea en el hilo principal |
| `menu_render/mod.rs:48,98`, `render/mod.rs:157,253`, `icon_cache.rs:86`, `thumbnail_cache.rs:99` | `pb.finish().unwrap()` | fallo de decodificación de ícono/miniatura |
| `app/*.rs` | `Pixmap::new(w,h).unwrap()` | sólo en OOM (todos tienen guard `width <= 0`) |

**Fix mínimo por bloque:**

1. `percent_decode`: `if let Some(hex) = s.get(i + 1..i + 3) && let Ok(byte) = …`
   (el `get` devuelve `None` si no es frontera de char, y no paniquea).
2. `.expect` de shm: `let Ok((buffer, canvas)) = … else { log::warn!(…); return; };`
   (un frame descartado es mejor que un proceso muerto).
3. `handlers.rs:195,201`: `if let Ok(p) = self.seat_state.get_pointer(…)`.
4. `pointer.rs:367`: `if let Some(icon) = self.dock.icons.get(idx)`.
5. `.lock().unwrap()` del tray: helper `fn tray_lock(&self) -> MutexGuard<…>` con
   `unwrap_or_else(|e| e.into_inner())` — recuperar del envenenamiento, no propagarlo.
6. `pb.finish().unwrap()`: `let Some(pb) = pb.finish() else { return; };`

**Verificación:** D2 tiene test (`assert_eq!(percent_decode("a%€"), "a%€")`, hoy
paniquea; `percent_decode("%20") == " "`). Para el resto: `rg -n 'expect\(|\.unwrap\(\)' src/`
debe devolver sólo sitios del arranque (donde fallar es correcto) y tests.

---

## 5. Severidad MEDIA — robustez, corrección y E/S

Faltan 5.1 (B1), 5.2 (B2), 5.3 (B3), 5.4 (B4), 5.7 (B7), 5.8 (B8), 5.9 (D2), 5.10 (D3),
5.11 (D4), 5.12 (D5), 5.13 (D6), 5.14 (D7), 5.15 (D8), 5.16 (B10), 5.17 (B11) y 5.18
(B12): ya cerrados, ver `AGENTS.md` → “Cerrado de AUDIT.md”. Lo que queda de esta
severidad es **B5** (reconexión D-Bus del tray) y **B6** (decisión, ver §3); los números se
conservan para no romper las referencias del §10 y del anexo.

---

### 5.5 — Conexión D-Bus del tray cacheada, sin reconexión {#b5}

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

---

### 5.6 — `repo_dir()` depende de la ruta del binario {#b6}

**Archivo:** `src/app/mod.rs:494-499`

```rust
fn repo_dir() -> std::path::PathBuf {
    std::env::current_exe().ok()
        .and_then(|p| p.parent()?.parent()?.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}
```

Asume `…/Docky.rs/target/release/dockyrs` y sube 3 niveles. Con un binario instalado
(`~/.local/bin/dockyrs`, `/usr/bin/dockyrs`) devuelve `~/.local` o `/`, y los cuatro
llamadores intentan spawnear `sync-dolphin-theme.sh` / `sync-kitty-theme.sh` /
`sync-p10k-theme.sh` que **no existen ahí**, con `let _ = …spawn()` → falla **en
silencio**. El usuario activa `accent_from_wallpaper` y los temas no se regeneran, sin
ningún aviso.

**El bloque de 3 spawns está copiado 4 veces** (mismo `repo_dir()`, mismos tres
nombres, mismo `let _ =`): `dock_menu_input.rs:152`, `:191`, `:226` y
`wallpaper_picker.rs:133`. O sea que el chequeo de `repo_dir()` es barato, pero el
fix tiene que aplicarse en los 4 (o extraer un `sync_external_themes()` único, que es
lo que este repo haría: `choose_theme` ya tiene las tres ramas duplicando el bloque).

**Fix mínimo:** `log::warn!` con la ruta intentada cuando el `spawn` falla.
Opcionalmente `DOCKYRS_SCRIPTS_DIR` por env o resolver los scripts en `$PATH`.
Decisión de producto; el mínimo es **no fallar en silencio**.

**Verificación:** `cp target/release/dockyrs /tmp/` y correrlo desde ahí: al elegir un
fondo con `accent_from_wallpaper` debe aparecer el warn en el log.

---

---

---

---

## 6. Severidad BAJA

Faltan 6.2 (C2), 6.3 (C3), 6.4 (C4), 6.6 (C6), 6.7 (D9), 6.8 (D10), 6.10 (D12), 6.11 (D13),
6.12 (C7) y 6.13 (C8): ya cerrados. 6.1 (C1) y 6.5 (C5) están en §3: C1 es decisión y C5
quedó a medias (en su sección está dicho qué falta).

### 6.1 — 12 funciones con 8-11 argumentos (clippy `too_many_arguments`) {#c1}

```text
src/menu_render/mod.rs:134      (8/7)
src/menu_render/mod.rs:157      (10/7)
src/menu_render/osd.rs:192      (8/7)
src/render/layout.rs:279        (10/7)   draw_widgets
src/render/clock_battery.rs:18  (10/7)
src/render/clock_battery.rs:70  (8/7)
src/render/media.rs:160         (8/7)
src/render/power_bluetooth.rs:45(8/7)
src/render/cpu_ram.rs:157       (11/7)
src/render/syswidgets.rs:3      (8/7)
src/render/mod.rs:496           (8/7)
src/render/mod.rs:518           (9/7)
```

Son **12 sitios**, no 9: faltaban `menu_render/mod.rs:134` y `menu_render/osd.rs:192.
El repo ya tiene el patrón correcto —`menu_render::DrawArgs`— pero la migración quedó
a medias:`draw_widgets` y los widgets de `render/` siguen pasando todo suelto. **No
es un bug**: es el warning más grande del proyecto y la causa de que agregar un widget
toque 6 firmas. Extender `DrawArgs` (o un `WidgetArgs`) a`render/`, **después** de los
fixes de §4/§5: es churn de firmas que ensucia los diffs.

Además hay 5 lints fuera de esta lista (verificados con
`cargo clippy --release --all-targets`): `unnecessary_find_map` (`widgets.rs:281`),
`needless_range_loop` ×2 (`menu/palette_controls.rs:44`, `menu/align_picker.rs:17`),
`manual_clamp` (`dock.rs:89`), `collapsible_match` (`clipboard/mod.rs:245`); y en el
build de tests `items_after_test_module` (`wallpaper.rs`) y `field_reassign_with_default`
(`menu/widget_chips.rs:236`, código de test).

### 6.5 — Huecos de tests {#c5}

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

---

## 7. Anexo — verificado sin acción (no mezclar con lo pendiente)

| Verificado | Por qué no se toca |
| --- | --- |
| `run_with_timeout` (`widgets.rs:4`) | patrón correcto (poll `try_wait` + `kill`); los que no lo usaban (A1, A4, A7) ya están arreglados |
| `clipboard/storage.rs` | atómica a `.tmp`, `0600`, dir `0700`, magic + topes |
| `tray::argb_to_pixmap` (`tray.rs:126`) | valida dims y `data.len()` antes de indexar |
| `layout_widgets` + `clamp` (`render/layout.rs:120-124`) | `min ≤ max` por construcción; no simplificar |
| `enforce_keyboard` (`dock_menu.rs:156`) | idempotente; solución de la trampa 9 |
| `ws_flash` (`render/workspaces.rs` `ws_flash_pill`) | el HUD reusa el rectángulo de `hit_layout` del dock: no hay escala que ajustar, era el bug ya resuelto |
| offsets `menu`/`menu_render` | dibujo y hit test comparten función en los 7 casos (`app_search_strip_y`, `clip_content_y`, `wallpaper_hit_test`, `overlay_tabs_h`, `volume_track_*`, `volume_pct_from_x`, `build_volume_controls`) |
| `find_menu`/`activate`/`send_menu_event` (`tray.rs`) | un hilo por click de usuario; aceptable |
| poll 2 s del tray (`tray.rs:350`) | hilo propio; los items muertos se purgan (era D5, ya cerrado) |
| `pactl_json` (`widgets.rs:459`) | techo de 400 ms documentado en el `ponytail:` |
| media wide (`layout.rs:133-146`) | reserva de más intencional; mismo hit test |
| `on_setting_changed` + `Release` (`popup_menu.rs:355`, `dock_menu_input.rs:523-539`) | throttle de 80 ms pero el valor final siempre se guarda |
| aritmética de color (`menu_render/mod.rs:221`, `dock_menu_input.rs:173`) | `f32` con cast que satura; sin overflow |
| `screenshot/encode.rs` | `checked_mul`, `.get()`, respeta `stride` |
| `place_widget` (`widget_chips.rs:86-92`) | índice acotado; el `insert` no paniquea |
| selección de región (`screenshot/region.rs`) | clampa a la superficie, rects ≥ 3×3 |
| input hex/nombre (`dock_menu_input.rs`) | `get_mut`, 6 hex / 24 chars, filtros |
| `choose_theme` presets/custom | sin underflow por guardas de rango |
| hit tests de dropdowns | misma geometría que el dibujo |

## 8. Plan de implementación sugerido

Cada etapa es verificable por sí sola, en orden de impacto para el usuario. Las etapas de
congelamientos, geometría, panics chicos, churn y E/S **ya están hechas** (ver `AGENTS.md`
→ “Cerrado de AUDIT.md”, con las mediciones). Queda **una** cosa:

### Etapa pendiente — tests de drag (C5)

`Dock::drag_to` y `Dock::icon_at` no tienen test propio: hace falta un fixture de `Dock`
con dos o tres iconos y verificar que el centro de cada uno devuelve su índice (el mismo
contrato que `hit_layout_tests` para los widgets, trampa 10). Es lo único que queda del
documento; lo demás son las **decisiones** de §3 (A5 en parte, B6 y C1).

---

## 9. Verificación end-to-end (después de cada etapa)

```sh
cargo build --release                      # SIEMPRE antes de reiniciar (trampa 5)
cargo test --release
cargo clippy --release --all-targets

# comprobar que el binario probado es el nuevo (trampa 5)
ls -l --time-style=+%H:%M:%S target/release/dockyrs
ps -o lstart= -p $(pgrep -x dockyrs)

# reiniciar
pkill -x dockyrs
setsid ./target/release/dockyrs >/tmp/dockyrs.log 2>&1 </dev/null &

# pruebas de UI sin mouse
python3 scripts/pointer.py --help          # puntero virtual (y teclado) por uinput
python3 scripts/click_at.py 47 346         # click en una coordenada absoluta
RUST_LOG="debug,zbus=warn,tracing=warn,sctk=info" ./target/release/dockyrs >/tmp/menu.log 2>&1
```

Líneas de log que sirven como sensor (todas ya existen):

- `dock: click derecho (x,y) -> Some(Kind)` / `dock: click (x,y) -> Some(Kind)`
- `dock: rueda (x,y) subir=bool -> Some(Kind)`
- `autohide:N reveal|hide|leave (franja)|timeout … borrowed=`
- `wsflash: show … visible= placed= borrowed=` y `wsflash: refresh before=… after=…`
- `overlay: X -> Y` (cambio de modo)
- `popup: t=… frame=Xms`
- `tray: <path> setup=Xms getlayout=Yms`
- `dock: configure new_size=… base=… applied=…`

Estado de las superficies (para verificar que el teclado no quede preso):

```sh
niri msg --json layers      # mira namespace, layer y keyboard_interactivity
```

---

## 10. Apéndice — invariantes que no hay que romper

Resumen de `AGENTS.md` como checklist de revisión:

1. Una superficie compartida para dock + modos; el popup del tray y el selector de
   screenshot tienen superficies propias.
2. Nunca `attach(NULL)` **en la superficie compartida del dock**: ocultar = buffer
   transparente (`draw_hidden`). El popup y el selector de screenshot sí se desmapean
   a propósito.
3. La superficie del dock es el disparador del autohide; la input region queda activa
   incluso oculto.
4. Si un modo cambia el tamaño, anotarlo en `applied_size`/`applied_geom` o restaurarlo
   con `relayout_dock`. `sync_autohide_surfaces` **nunca** toca el tamaño.
5. Al mover el panel debajo del dock, las coordenadas del puntero son de la
   **superficie**: traducir con `panel_local()`.
6. `cargo test`/`clippy` no regeneran `target/release/dockyrs`.
7. El orden de widgets sale de `settings.widgets`; `place_widget` inserta, no `push`.
8. Los hit tests reparten con `render::hit_layout()`/`hit_scale()`, **incluida su
   geometría interna** (D8, la etiqueta de RAM duplicada, es el caso vivo hoy).
9. Geometría compartida entre reparto y dibujo en **una** función
   (`volume_content_len`, `VOLUME_ICON_*`, `ram_label`): la de RAM era el último caso
   (D8, ya cerrado).
10. El teclado de la superficie compartida pasa siempre por `enforce_keyboard()`; no
    setear `KeyboardInteractivity` a mano en caminos nuevos sin justificarlo (hay 17
    sitios manuales hoy en `app_*`, `screenshot/*` y `main.rs`: candidatos a unificar).
11. Comentarios y commits en español, descriptivos, sin prefijos `feat:`.
