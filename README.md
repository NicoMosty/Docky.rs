# Docky.rs

<img width="784" height="88" alt="image" src="https://github.com/user-attachments/assets/7f72d11a-18c3-4a05-a313-33dea8de0d9f" />

A dock/bar for Wayland, written in Rust. Pinned apps, Apps search, Clipboard manager, Screenshot tool, a wallpaper picker, a widget bar (clock, workspaces, media, CPU/RAM, tray), volume and brightness OSDs, and notifications.

> **Fork.** Este repo es un fork de [`Volcha-Z/Docky.rs`](https://github.com/Volcha-Z/Docky.rs), mantenido por [NicoMosty](https://github.com/NicoMosty). El original es para Hyprland; este fork agrega **backend de niri** (el compositor de referencia acá), autohide sin reservar espacio, un HUD de workspaces al cambiar de workspace, el panel de ajustes debajo del dock, y varios arreglos de UI. Licencia MIT, igual que el original.

The built binary is around 13MB and it sits around 10-65mb of RAM while running.

## Requirements

* Hyprland o niri
* Arch Linux or Void Linux

## Install

```sh
git clone https://github.com/NicoMosty/Docky.rs.git
cd Docky.rs
./install.sh
```

`install.sh` detects your distro, installs everything the dock needs, and builds it with `cargo build --release`. The finished binary ends up at `target/release/dockyrs`.

## Config

Settings live at `~/.config/dockyrs/config.json` and are created automatically the first time you run the dock. There is nothing you need to set up by hand.

## Autostart (niri)

En niri se arranca con `spawn-at-startup` (ver `niri-config.kdl.example`):

```kdl
spawn-at-startup "/ruta/a/Docky.rs/target/release/dockyrs"
```

Con una sola salida alcanza esa línea. Con varias salidas, una instancia por monitor
con `--output <nombre>` y `--profile <perfil>` (cada perfil usa
`~/.config/dockyrs/config-<perfil>.json`).

No arranques `dockyrs-notifyd` junto a otro daemon de notificaciones (mako, dunst):
los dos pelean por `org.freedesktop.Notifications`.

## Tests

```sh
cargo test --release                              # incluye los del reordenamiento de widgets
cargo clippy --release --all-targets
```

`scripts/pointer.py` inyecta un puntero virtual por uinput para clickear el dock en un
punto exacto durante las pruebas; ver su docstring.

## Wallpaper based theming (optional)

If you turn on `accent_from_wallpaper` in the config, picking a new wallpaper through the dock will also regenerate colors for Dolphin, kitty, and a Powerlevel10k using matugen.

## License

MIT, see LICENSE.

## Keybinds

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

## Wallpaper selector

Place your wallpapers here ~/Pictures/Wallpapers u might need to create the folders yourself!
