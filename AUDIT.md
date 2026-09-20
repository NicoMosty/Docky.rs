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
> (A1, A2, A3, A4, A6, A7, B3, C4, C8, D1, D2, D3, D11) se movieron a `AGENTS.md`
> → “Cerrado de AUDIT.md” el 2026-09-20, con lo que se hizo y cómo se verificó; los
> números de sección que faltan abajo son justamente esos. A5, C5 y D12 quedaron a
> medias: lo pendiente sigue en su sección, con una nota arriba.

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
   lista **sólo lo abierto**: si algo queda acá, es que falta. (Así se hizo la limpieza
   del 2026-09-20, que movió 13 hallazgos.)
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

**Pendientes: 26 hallazgos** (1 ALTA parcial, 12 MEDIA y 13 BAJA). Lo ya cerrado está en
`AGENTS.md` → “Cerrado de AUDIT.md”: **A1, A2, A3, A4, A6, A7, B3, C4, C8, D1, D2, D3 y
D11**, con la evidencia de cómo se verificó cada uno. Lo que quedó a medias (A5, C5, D12)
tiene una nota al principio de su sección.

| ID | Sev. | Título | Archivo |
| --- | --- | --- | --- |
| **A5** | ALTA | Cualquier panic mata el dock; no hay recuperación ni supervisión | varios (§4.7) |
| **B1** | MEDIA | Shift+flecha en auto-repeat cicla los 4 modos en bucle | `handlers.rs:323` |
| **B2** | MEDIA | Socket IPC en `/tmp` si falta `XDG_RUNTIME_DIR` (accesible a otros usuarios) | `ipc.rs:28` |
| **B4** | MEDIA | Watcher de niri: `niri msg` por cada evento de ventana | `ipc.rs:303` |
| **B5** | MEDIA | Conexión D-Bus del tray cacheada sin reconexión → tray muerto permanente | `tray.rs:187` |
| **B6** | MEDIA | `repo_dir()` depende de la ruta del binario: theming se pierde sin aviso | `app/mod.rs:494` |
| **B7** | MEDIA | `configure` no reconcilia `new_size` con lo dibujado → clicks corridos | `handlers.rs:154` |
| **B8** | MEDIA | Carátula: metadata MPRIS no confiable → request saliente + escritura en caché | `widgets.rs:932` |
| **B10** | MEDIA | Elegir fuente pisa `gtk-*.ini`/`kdeglobals`/`kitty.conf` sin escritura atómica | `app/fonts.rs:63,110` |
| **B11** | MEDIA | `dockyrs-notifyd` ignora `--profile`: con instancias por perfil no llega ninguna notificación | `bin/dockyrs-notifyd.rs:13` |
| **B12** | MEDIA | Pegar del portapapeles lee con `read_to_end` sin tope → OOM con texto enorme | `clipboard/paste.rs:56` |
| **D4** | MEDIA | Caché de carátulas en disco sin tope (crece con cada tema nuevo) | `widgets.rs:925` |
| **D5** | MEDIA | El `Watcher` del tray no purga items muertos; los re-resuelve todos cada 2 s | `tray.rs:348` |
| **D6** | MEDIA | `draw_text_clipped` aloca y zero-llena un `Pixmap` por frame (~30 fps) | `render/media.rs:8` |
| **D7** | MEDIA | `draw_widgets` reparte el layout 2-3 veces por frame | `render/layout.rs:319,497,584` |
| **D8** | MEDIA | Etiqueta de RAM duplicada entre reparto y dibujo (trampa 12) | `render/cpu_ram.rs:207` / `widget.rs:888` |
| **C1** | BAJA | 12 funciones con 8-11 argumentos (síntoma: `DrawArgs` a medio migrar) | §6.1 |
| **C2** | BAJA | `DockMenuMode.closing` es código muerto | `app/dock_menu.rs:349` |
| **C3** | BAJA | `Config::load` descarta apps+settings si el JSON no parsea | `config.rs:506` |
| **C5** | BAJA | Huecos de tests (watchers/timers) — el resto ya está cubierto | §6.5 |
| **C6** | BAJA | `notify` por IPC truncado a 1024 bytes sin avisar | `ipc.rs:121` |
| **C7** | BAJA | `dockyrs-notifyd` detiene dunst/mako/swaync/fnott/wired en cada arranque | `bin/dockyrs-notifyd.rs:78` |
| **D9** | BAJA | `nearest_tray_index` resta `count - 1` (underflow latente) | `render/tray.rs:7` |
| **D10** | BAJA | `read_cpu` suma `guest`/`guest_nice` (doble conteo) | `widgets.rs:477` |
| **D12** | BAJA | Código muerto/no-op en render (`ICON_OVERSAMPLE = 1.0` sigue vivo) | §6.10 |
| **D13** | BAJA | `IconCache::get` aloca un `String` por lookup, incluso con hit | `icon_cache.rs:26` |

Prioridad de ejecución: **A5 (el resto del guard) → D2/D9/D10 (panics y aritmética) →
B4/D5/D6/D7 (churn) → B2/B5/B6/B7/B8/B10/B11/B12/C6/C7 (E/S y seguridad) → B1 y el
resto de C/D → D4/D8/D12/D13.** El detalle, en §8.

---

## 4. Severidad ALTA

Los números que faltan (4.1–4.6 y 4.8) son hallazgos **ya cerrados**: ver `AGENTS.md` →
“Cerrado de AUDIT.md”. Queda uno, y sólo en parte: el listado de abajo describe el estado
**original**, y el guard de arriba dice qué falta.

---

---

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

Faltan 5.3 (B3), 5.9 (D2) y 5.10 (D3): ya cerrados, ver `AGENTS.md` → “Cerrado de
AUDIT.md”. El resto sigue abierto, y los números se conservan para no romper las
referencias del §10 y del anexo.

### 5.1 — El auto-repeat de Shift+flecha cicla modos en bucle {#b1}

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

---

### 5.2 — Socket IPC en `/tmp` si falta `XDG_RUNTIME_DIR` {#b2}

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

---

### 5.4 — Watcher de niri: `niri msg` por cada evento de ventana {#b4}

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

### 5.7 — `configure` no reconcilia el tamaño con lo dibujado {#b7}

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

---

### 5.8 — Carátula: metadata MPRIS no confiable {#b8}

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

---

---

### 5.11 — La caché de carátulas en disco crece sin tope {#d4}

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

---

### 5.12 — El `Watcher` del tray no purga items muertos {#d5}

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

---

### 5.13 — `draw_text_clipped` aloca un `Pixmap` por frame {#d6}

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

---

### 5.14 — `draw_widgets` reparte el layout 2-3 veces por frame {#d7}

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

---

### 5.15 — Etiqueta de RAM duplicada entre reparto y dibujo (trampa 12) {#d8}

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

---

### 5.16 — Elegir fuente pisa la config del usuario sin escritura atómica {#b10}

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

---

### 5.17 — `dockyrs-notifyd` ignora el perfil: no entrega notificaciones {#b11}

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

---

### 5.18 — Pegar del portapapeles lee sin tope {#b12}

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

---

## 6. Severidad BAJA

Faltan 6.4 (C4) y 6.13 (C8): ya cerrados. 6.5 (C5) y 6.10 (D12) quedaron a medias: en su
sección está dicho qué falta.

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

### 6.2 — `DockMenuMode.closing` es código muerto {#c2}

El campo se declara en `src/app/mod.rs:199` (dentro de `DockMenuMode`, `:190`) y se
inicializa en `src/app/dock_menu.rs:130`; `dock_menu.rs:401` lo lee. Nunca se pone en
`true`: `close_dock_menu` (`dock_menu.rs:184`) anula el modo de una, así que la rama
`if closing && anim <= 0.0` (`dock_menu.rs:405`) es inalcanzable.

**Precisión sobre `AGENTS.md`:** dice que el campo muerto está en *"`WsFlashMode`/
`DockMenuMode`"*. En `WsFlashMode` **sí** se usa (`ws_flash.rs:81` lo pone en `true`,
`:205` lo lee). El muerto es sólo `DockMenuMode.closing`. Corregir el texto junto con
el código: borrar el campo y la rama inalcanzable.

### 6.3 — `Config::load` descarta apps y ajustes si el JSON no parsea {#c3}

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

### 6.5 — Huecos de tests {#c5}

> **Parcialmente resuelto.** De la lista de abajo ya están cubiertos: 1 (el reparto de
> `workspaces.rs` tiene `workspace_hit_tests`), 2 en la parte de `percent_decode`
> (`percent_decode_tests`), 3 (`ipc.rs` tiene 3 tests, incluido el filtro del
> `event-stream`) y 5 en parte (los timers siguen sin test propio). **Lo que falta:**
> los timers (`spawn_*_timer`), `wallpaper_program()`, `resolve_art_path` (B8),
> `tray_geometry` con `widget_scale ≠ 1` y `Dock::drag_to`/`icon_at`. Con eso los tests
> pasaron de 37 a **147**.

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

### 6.6 — `notify` por IPC truncado a 1024 bytes {#c6}

`ipc.rs:71-80`: `read(&mut buf)` con `buf = [0u8; 1024]`, y `notify` mete título +
cuerpo separados por `\u{1f}` en ese buffer. Un cuerpo largo se corta **en silencio**
(no hay bucle de lectura ni longitudes). **Fix mínimo:** leer hasta EOF acumulando en
un `Vec`, o truncar con `…` visible en vez de cortar en seco.

### 6.7 — `nearest_tray_index` resta `count - 1` {#d9}

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

### 6.8 — `read_cpu` suma `guest`/`guest_nice` (doble conteo) {#d10}

`src/widgets.rs:173-174`

```rust
let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
let total: u64 = fields.iter().sum();
```

En `/proc/stat`, `guest` ya está incluido en `user` (y `guest_nice` en `nice`): sumar
los 10 campos lo cuenta doble y sesga el porcentaje. **Fix:**
`let total: u64 = fields.iter().take(8).sum();`. **Verificación:** test con la línea
sintética `cpu  100 0 100 800 0 0 0 0 200 0 0 0` → total 1000, no 1200.

### 6.10 — Código muerto / no-op en render {#d12}

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

### 6.11 — `IconCache::get` aloca un `String` por lookup {#d13}

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

### 6.12 — `dockyrs-notifyd` detiene los otros daemons de notificaciones {#c7}

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
| poll 2 s del tray (`tray.rs:350`) | hilo propio; el problema es la lista sin purgar (D5) |
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

Cada etapa es verificable por sí sola, en orden de impacto para el usuario. Las etapas
1 y 4 (menos B4/D4/D5) **ya están hechas**: ver `AGENTS.md` → “Cerrado de AUDIT.md”.

### Etapa pendiente — Panics y aritmética (A5, D9, D10)

`percent_decode` con `get` (hecho), quitar los `expect`/`unwrap` de la tabla de A5,
helper de lock del tray que sobreviva al envenenamiento, `catch_unwind` alrededor de los
drenajes de IPC después del `match`, `saturating_sub` en `nearest_tray_index` y no sumar
`guest`/`guest_nice` en `read_cpu`.

**Guard:** `nearest_tray_index` con 0 (D9) y el test sintético de `/proc/stat` (D10).

### Etapa pendiente — Churn (B4, D4, D5, D6, D7, D8)

Leer los workspaces del **payload** del evento de niri (el `WorkspacesChanged` ya trae la
lista; hoy cada evento dispara un `niri msg --json workspaces`), purga del `Watcher` por
`name_has_owner`, Pixmap reusable en `draw_text_clipped`, layout calculado una sola vez,
tope de la caché de carátulas y una sola `ram_label` para reparto y dibujo.

**Guard:** test del tope de la caché y de la etiqueta única (D8, trampa 12).

### Etapa pendiente — E/S y seguridad (B2, B5, B6, B7, B8, B10, B11, B12, C6, C7)

Socket con permisos y timeout de lectura, reconexión D-Bus, no fallar en silencio con
los `sync-*.sh`, `warn` en el desajuste de `configure`, validación de URLs, lectura
completa y acotada de `notify`/paste, escritura atómica de la config de fuentes, perfil
en `dockyrs-notifyd` y que deje de matar otros daemons.

**Guard:** tests de escritura atómica (el original sobrevive), del tope de paste y de
`socket_path` con perfil.

### Etapa pendiente — Limpieza (B1 y el resto de C/D: C1, C2, C3, C5, D12, D13)

Auto-repeat de Shift+flecha, `DrawArgs` extendido a `render/`, `DockMenuMode.closing`,
no-op de render (`ICON_OVERSAMPLE`), `IconCache` sin alloc por hit, backup del config
ilegible y cerrar los huecos de tests que quedan (watchers/timers).

**No empezar por acá.** Las etapas de arriba cambian lo que el usuario siente; esta sola
no arregla nada visible.

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
   (`volume_content_len`, `VOLUME_ICON_*`): la de RAM todavía no (D8).
10. El teclado de la superficie compartida pasa siempre por `enforce_keyboard()`; no
    setear `KeyboardInteractivity` a mano en caminos nuevos sin justificarlo (hay 17
    sitios manuales hoy en `app_*`, `screenshot/*` y `main.rs`: candidatos a unificar).
11. Comentarios y commits en español, descriptivos, sin prefijos `feat:`.
