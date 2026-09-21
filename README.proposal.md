# Docky.rs

<img width="784" height="88" alt="image" src="https://github.com/user-attachments/assets/7f72d11a-18c3-4a05-a313-33dea8de0d9f" />

A dock/bar for Wayland, written in Rust — for **niri** and Hyprland. Pinned apps, an app
launcher, a window switcher, a clipboard manager, a notification history, a wallpaper
picker, a volume panel, a calendar, a widget bar, volume/brightness OSDs, screenshots
and a screen-recording indicator.

> ### This is a fork
>
> Fork of [`Volcha-Z/Docky.rs`](https://github.com/Volcha-Z/Docky.rs), maintained by
> [NicoMosty](https://github.com/NicoMosty). Upstream targets Hyprland; this fork keeps
> Hyprland working and makes **niri** the reference compositor.
>
> What it adds on top of upstream:
>
> - **niri backend** (`--output` / `--profile`, one instance per monitor, workspaces,
>   keyboard layout, urgency and Overview read from niri's event stream).
> - **Autohide without reserving space**: the dock hides by drawing nothing, and the
>   hidden state is a small capsule ("idle island") showing the time, the battery and
>   whatever is playing or recording.
> - **An overlay with tabs**: Apps / Clipboard / Notifs / Wallpapers / Windows, opened
>   below the dock, with click-outside-to-close, a shared width (and, on a vertical
>   dock, a shared height) and clickable tabs.
> - **A settings panel** below the dock: edit layout, appearance, colors and the widget
>   bar live, with the panel open, and drag widgets to reorder them.
> - **More widgets** (`Mic`, `Recording`, `Custom` scripts) and a widget table so
>   adding one is a two-line change.
> - Robustness and perf work that upstream does not have: the dock relaunches itself on
>   a fatal protocol error, everything that can hang (D-Bus, `curl`, system reads) runs
>   with a timeout or off the drawing thread, and the cold start no longer waits for
>   the slow system reads.

The release binary is about **12 MB** (stripped) and the dock sits at about **13 MB of
RSS at rest** — around 30 MB counting the helper processes it spawns. Everything is
rasterized on the CPU on purpose (no GPU context), so the cost is paid only when
something changes: at rest it wakes up roughly once every 5 seconds.

## Requirements

- **Hyprland or niri**, on **Arch Linux or Void Linux** (`install.sh` knows both).
- PipeWire (`wpctl`) for the volume and mic widgets, `pactl` for the per-app volume
  panel.
- Rust (to build) — `install.sh` handles the toolchain and the rest of the packages.
- Optional, for the features that need them: `wf-recorder` (screen recording),
  `wtype` (clipboard auto-paste under niri), `matugen` (wallpaper theming — the
  installer adds it with `cargo install`), `brightnessctl` (brightness OSD),
  `playerctl` (media widget) and `swaybg`/`awww`/`swww` (wallpapers).

## Install

```sh
git clone https://github.com/NicoMosty/Docky.rs.git
cd Docky.rs
./install.sh
```

`install.sh` detects the distro, installs what is missing, builds the dock with
`cargo build --release`, makes Docky.rs the notification daemon (dropping the incumbent:
dunst, mako, swaync, fnott, …), writes the Hyprland binds and the blur rule, and starts
it. The binary ends up at `target/release/dockyrs`.

To build by hand instead:

```sh
cargo build --release
./target/release/dockyrs          # or target/release/dockyrs --output DP-2 --profile dp
```

## Run on niri

The installer does **not** touch your niri config: add the autostart line yourself
(see [`niri-config.kdl.example`](niri-config.kdl.example)).

```kdl
// One instance, single monitor:
spawn-at-startup "/path/to/Docky.rs/target/release/dockyrs"

// One instance per monitor, each with its own config and IPC socket:
spawn-at-startup "sh" "-c" "…/dockyrs --output DP-2 --profile dp"
spawn-at-startup "sh" "-c" "…/dockyrs --output HDMI-A-1 --profile hdmi"
```

- `--output <NAME>` pins the instance to one output (niri puts a single instance
  wherever it wants otherwise).
- `--profile <NAME>` gives the instance its own config
  (`~/.config/dockyrs/config-<name>.json`) and its own IPC socket
  (`$XDG_RUNTIME_DIR/dockyrs-<name>.sock`), so keybinds need `--profile dp` to reach the
  right one.
- Do **not** start `dockyrs-notifyd` next to another notification daemon (mako, dunst):
  both fight over `org.freedesktop.Notifications`.

Notes for niri specifically: the clipboard's auto-paste uses `wtype`; log out uses
`niri msg action quit`; the Hyprland blur rule does not exist there, so panels are
plain translucent.

## Using the dock

| Where | Action | What happens |
| --- | --- | --- |
| Anywhere on the dock | Move the pointer to the screen edge | Reveals the dock (autohide); it hides again when you leave |
| App icon | Left click | Launches it (or focuses the open window) |
| App icon | Left drag | Moves the icon along the bar (to another slot) |
| App icon | Right click | Per-icon menu (change icon, remove) |
| Empty dock area | Double left click | Wallpaper picker (center band only, so it does not fire by accident) |
| Workspace dot | Left click | Switches to that workspace (and focuses its monitor) |
| Workspace dot | Right click | **Settings panel** |
| Volume | Left click | Volume panel (one row per app, drag to set, per-row mute, output picker) |
| Volume | Wheel | ±5 % |
| Volume | Right click | `pavucontrol` |
| Mic | Left click | Toggles the microphone mute |
| Recording | Left click | Starts/stops `record-toggle.sh`; while recording, the dot and the elapsed time are a live activity of the island |
| Network / Bluetooth | Left click | Opens the settings app |
| Network / Bluetooth | Right click | The item's tray menu (they are hidden from the tray itself, the widget is the way in) |
| Tray icon | Left click | Activates the item; right click opens its menu |
| Clock | Hover | Calendar (←/→ change month, ESC closes) |
| Power | Left click | Power menu (suspend, log out, reboot, shut down) |
| Keyboard layout | Left click | Next layout |

### The overlay

One mini-app at a time, opened below the dock (`Mod+D` for the launcher from the
Hyprland binds, or any `--toggle-*` command):

| Tab | What it is |
| --- | --- |
| **Apps** | App launcher: sorts by usage when the box is empty, fuzzy matching, `Keywords`, `Name[es]`, `Terminal=true` |
| **Clipboard** | Clipboard history with image previews |
| **Notifs** | The notification history (the last 50) |
| **Wallpapers** | Thumbnail filmstrip of `wallpaper_dir` |
| **Windows** | The same list as Apps, built from niri's windows; `Enter` focuses the window |

- `Shift` + `←`/`→` cycles the tabs (with a vertical dock, `↑`/`↓` too), and the tabs
  themselves are clickable.
- `ESC` or a click outside closes it. While a panel is open the overlay is modal: the
  click does not reach the window below.
- Keyboard: arrows move, `Enter` activates, `ESC` closes. In the clipboard, `Delete`
  removes the selected entry and `BackSpace` edits the search box; the notification
  history also takes `PageUp`/`PageDown`/`Home`/`End`.

### The settings panel

Right click on the workspace indicator. Five tabs — **Layout**, **Appearance**,
**Colors**, **Widgets**, **System** — with the dock visible behind, so every change is
live. Everything is navigable with the keyboard (arrows, `Enter`, `ESC`), the widget
bar is edited by dragging the chips, and the panel has its own "Smooth Transitions"
switch to turn the animations off when you prefer them out of the way.

## Widgets

Add, remove and reorder them from the settings panel (**Widgets** tab). Out of the box:
power menu, battery, keyboard layout, network, bluetooth and volume on the left,
workspaces in the middle, tray and clock on the right.

| Widget | Shows | Click |
| --- | --- | --- |
| `Clock` | Time and date; a compact hour-only form inside the island | Hover opens the calendar |
| `Battery` | Percentage inside the icon: green while charging, orange in conservation mode, red ≤ 20 % while discharging | — |
| `Workspaces` | One dot per workspace, the urgent one in red | Left: switch · Right: settings |
| `Network` | Icon + SSID pill | Left: `nmrs` · Right: tray menu |
| `Bluetooth` | Icon, colored by state | Left: your bluetooth manager script if you have one, otherwise toggles the radio · Right: tray menu |
| `Volume` | Icon + percentage, red when muted | Left: panel · Wheel: ±5 % · Right: `pavucontrol` |
| `Mic` | Icon + `ON`/`MUTE` (red when muted) | Left: toggle mute |
| `Recording` | Red dot + `MM:SS` | Left: start/stop recording |
| `Media` | Now playing with the cover art | Left: play/pause |
| `Tray` | StatusNotifierItem icons | Left: activate · Right: menu |
| `KbdLayout` | Active keyboard layout | Left: next layout |
| `Cpu` / `Ram` | Usage | — / `btop` in a terminal |
| `Custom` | Output of a script, on a fixed interval (configured in `custom_widgets`) | — |
| `PowerMenu` | Power icon | Left: power menu |

## Configuration

Settings live at `~/.config/dockyrs/config.json`, created automatically on first run;
with `--profile <name>` the file is `config-<name>.json`. Almost everything is editable
from the settings panel, and those writes are what you see in the file. A few keys worth
knowing by hand: `dock_edge` (`Top`/`Bottom`/`Left`/`Right`), `dock_align`, `autohide`,
`autohide_delay_ms`, `wallpaper_dir`, `accent_from_wallpaper`, `matugen_*` and
`widgets`.

| Path | What |
| --- | --- |
| `~/.config/dockyrs/config.json` | Settings (per profile: `config-<profile>.json`) |
| `$XDG_RUNTIME_DIR/dockyrs[-<profile>].sock` | IPC socket the CLI talks to |
| `~/.cache/dockyrs/app-usage.json` | Launcher ranking |
| `~/.cache/dockyrs/thumbs/` | Wallpaper thumbnails, pre-scaled |
| `~/.cache/dockyrs-recording-path` | Set by `record-toggle.sh` while recording |
| `~/.config/wallpaper-path` | The current wallpaper, for `swaybg` setups |

## Commands (IPC)

Every command talks to the instance that is already running; add `--profile <name>` when
you run one per monitor. They are the same commands the keybinds use.

| Command | What it does |
| --- | --- |
| `--toggle-search` | Opens/closes the app launcher |
| `--toggle-clipboard` | Clipboard history |
| `--toggle-notifications` | Notification history |
| `--toggle-wallpaper` | Wallpaper picker |
| `--toggle-dock-menu` | Settings panel |
| `--osd-volume` / `--osd-brightness` | Shows the OSD (the keybind changes the value and then calls this) |
| `--screenshot-region` / `--screenshot-full` | Screenshot, region or full screen |
| `--test-notification` | Sends a test notification |
| `--notify TITLE BODY` | Sends a notification through the dock's daemon |
| `--output NAME` | Pins this instance to an output |
| `--profile NAME` | Separate config and socket for this instance |

## Keybinds

`install.sh` writes these for Hyprland (into `~/.config/hypr/dockyrs.lua`):

| Key | Action |
| --- | --- |
| `SUPER` + `D` | App search |
| `SUPER` + `W` | Wallpaper picker |
| `SUPER` + `V` | Clipboard manager |
| `SUPER` + `,` | Dock menu |
| `SUPER` + `R` | Relaunch dock |
| `SUPER` + `CTRL` + `SHIFT` + `1` | Test notification |
| `F8` | Screenshot region |
| `F9` | Screenshot full |
| `F10` | Toggle screen recording |
| `XF86Audio*`, `XF86AudioMicMute` | Volume/mute + volume OSD |
| `code:224` / `code:225` | Brightness step + brightness OSD |

On niri, bind the same commands yourself, for example:

```kdl
Mod+D { spawn "sh" "-c" "dockyrs --toggle-search"; }
Mod+Comma { spawn "sh" "-c" "dockyrs --toggle-dock-menu"; }
F10 { spawn "sh" "-c" "/path/to/Docky.rs/record-toggle.sh"; }
```

## Wallpaper based theming (optional)

With `accent_from_wallpaper` on, picking a wallpaper regenerates the dock colors with
matugen and can also regenerate colors for Dolphin, kitty and Powerlevel10k.

Two independent toggles in **Settings → Colors**:

- **Matugen Apps**: runs *your own* matugen (`~/.config/matugen/config.toml` and its
  templates: kitty, waybar, rofi, niri, gtk, …) with the dock's scheme, so apps that
  already follow matugen stay in sync with the dock. Off by default, since it rewrites
  other apps' configs.
- **Light Mode**: passes `--mode light` to matugen, for both the dock colors and the
  apps.

The wallpaper folder (`wallpaper_dir`, default `~/Pictures/Wallpapers`) is picked from
the panel, and the **Matugen Style** list includes `scheme-smart`.

## Tests and development

```sh
cargo test --release                 # 147 tests, including the canvas crate
cargo clippy --release --all-targets
cargo build --release                # `cargo test` does NOT refresh the binary
```

The dock can be driven without a mouse during development:
`scripts/pointer.py` injects a virtual pointer through uinput (and a virtual keyboard for
the menus), `scripts/click_at.py` clicks at an absolute screen coordinate (needed when a
panel is already open, since `pointer.py`'s sensor is the reveal), and
`scripts/sweep_vertical.py` sweeps a vertical dock. `scripts/fake_sni_hang.py` is a tray
app that never answers, to exercise the D-Bus timeouts.

Design notes — the code map, the decisions and the traps that each cost a session — are
in [`AGENTS.md`](AGENTS.md); the robustness audit with its status lives in
[`AUDIT.md`](AUDIT.md).

## License

MIT, see LICENSE.

## Wallpaper selector

Place your wallpapers here: `~/Pictures/Wallpapers` (you might need to create the folder
yourself) — or point `wallpaper_dir` anywhere from the settings panel.

---

## Notas de la propuesta (borrar antes de publicar)

Este archivo es una propuesta para reemplazar `README.md`: no está commiteado y no toca
el README actual. Lo que cambió respecto del README vigente:

1. **Un solo idioma** (inglés, como el cuerpo de upstream): el README actual mezcla el
texto original con bloques en castellano (`Fork.`, `Autostart (niri)`, `Tests`).
2. **Se declara el fork arriba** y se listan las diferencias con upstream, que es lo
primero que mira alguien que llega al repo.
3. **“Using the dock”**: la tabla de gestos, que no existía en ninguna parte.
4. **Widgets**: la lista completa con lo que hace cada click.
5. **Overlay y panel de ajustes** como secciones propias (pestañas, atajos, teclado).
6. **Comandos IPC y ejemplos de keybind de niri** (antes sólo estaban los de Hyprland,
escritos por `install.sh`).
7. **Rutas** (`config.json`, `config-<perfil>.json`, sockets, cachés).
8. **Tests y herramienta de prueba** (`scripts/pointer.py`, `click_at.py`,
`sweep_vertical.py`, `fake_sni_hang.py`) + puntero a `AGENTS.md` / `AUDIT.md`.
9. Se arregló la última línea del selector de fondos (“u might need”) y se sacó la
frase de RAM/binario de upstream, reemplazada por los números medidos.

Datos que tomé del código para que la tabla de gestos y widgets no mienta
(`src/widget.rs`, `src/config.rs`, `src/menu/controls.rs`, `src/app/pointer.rs`,
`install.sh`): el click de `Network` abre `nmrs`; `Bluetooth` corre tu script de waybar
si existe y si no togglea la radio; `Right` de `Volume` abre `pavucontrol`; `Cpu` no
tiene click y `Ram` abre `btop`; el menú del ícono es *change icon* / *remove*; el
portapapeles usa `Delete` para borrar la entrada, no `Home`/`End`.

Lo que **no** agregué y quizá quieras: capturas de pantalla del dock (haría falta subir
imágenes al repo), una sección de “Roadmap”/“Known issues” (hoy eso vive en `AUDIT.md`),
y el badge de build/CI (no hay CI en el repo).
