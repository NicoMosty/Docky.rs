#!/usr/bin/env python3
"""Puntero virtual por uinput para clickear el dock en un punto exacto.

Por qué existe: no hay forma de clickear un punto preciso sin esto. libinput
aplica aceleración a los deltas relativos, así que apuntar "a X px" con un
movimiento grande NO es determinista. Con pasos chicos sí lo es: medido en este
repo, 3 px relativos por paso dan ~3.05 px reales (~1.02x), de forma repetible.

El truco: en vez de calcular la geometría del dock, el script LEE EL LOG de la
app para saber cuándo el puntero ya entró en su superficie (línea " reveal").
Así se detiene en el borde izquierdo real de la barra, sin importar dónde esté
ni cuánto mida.

Uso:
    # la app tiene que loguear en debug:
    #   RUST_LOG=debug ./target/release/dockyrs >/tmp/menu.log 2>&1
    python3 scripts/pointer.py --extra 115 --button R   # -> Some(Workspaces)
    python3 scripts/pointer.py --extra 0 --button L

    --extra N  pasos de ~3px a la derecha desde el borde revelado
               (x local aproximado = 3.05 * N)
    --button   R = click derecho, L = izquierdo
    --y N      pasos de 2px hacia abajo desde el borde superior (default 8 -> y~18,
               dentro de los rects de widgets; con --y 0 el click queda en y~0)
    --then-dx N / --then-dy M
               después del primer click, se desplaza N pasos de ~3px en x y M de
               ~2px en y (admiten negativos) y hace un click IZQUIERDO. Sirve para
               encadenar dos clicks en la misma sesión de dispositivo, por ejemplo
               abrir el panel de ajustes y después clickear un control suyo.
    --double   hace un DOBLE click: dos clicks separados ~80ms, dentro de los 400ms
               que pide el dock para reconocer el gesto.

    --shift-arrow left|right|up|down [--times N]
               en vez de clickear, teclea Shift+esa flecha (N veces, default 1): es el
               ciclo de modos del overlay, que sin esto no se puede ejercitar sin
               teclado. Con el dock VERTICAL la banda de pestañas es una columna, así
               que además de ←/→ andan ↑/↓ (el gesto que sigue a la banda).
               Ej.: abrir el launcher por IPC y `--shift-arrow left` para caer en
               "ventanas" (Apps -> Windows), o `--shift-arrow down` con el dock al
               costado para pasar a "Clipboard".

    --scroll up|down [--times N]
               en vez de clickear, gira la RUEDA sobre el punto alcanzado (default
               N=1). Sirve para el volumen del dock (rueda = ±5% sobre el widget de
               volumen). Necesita que el dock esté visible: el reveal se hace igual.

    --key up|down|left|right|enter|escape|backspace [--times N]
               en vez de clickear, teclea esa tecla N veces (taps sueltos, sin
               auto-repeat) con el TECLADO virtual. Es lo que ejercita la navegación
               de los menús: imprime la última línea `teclado:` del log, que dice qué
               quedó resaltado (o si Escape cerró). Igual que --shift-arrow, necesita
               un modo abierto (la superficie con el teclado en Exclusive).

    --hover    en vez de clickear, deja el puntero donde llegó y espera: es lo que
               ejercita lo que se abre con hover (el calendario del reloj, 450ms).
               Imprime la última línea `calendario:` del log. Para saber dónde caer,
               primero se busca el widget con un click normal (imprime el hit).
               Ej.:
                   dockyrs --toggle-dock-menu   # no: el panel tapa el hover
                   sudo python3 scripts/pointer.py --extra 600 --button L   # -> Some(Clock)
                   sudo python3 scripts/pointer.py --extra 600 --hover        # calendario: abre
                   sudo python3 scripts/pointer.py --key right --times 2      # mes siguiente

Imprime el hit que devolvió la app (`-> Some(Battery)`, `-> None`, …), que es
lo que permite saber dónde cayó el click sin adivinar.

Requiere que el dock pueda pasar a oculto: si la corrida anterior abrió el
panel de ajustes, cerralo antes (click afuera, o `dockyrs --toggle-dock-menu`),
porque mientras el panel está abierto la app nunca se oculta.
"""

import argparse
import fcntl
import os
import struct
import sys
import time

UI_SET_EVBIT = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_SET_RELBIT = 0x40045566
# _IOW('U', 3, struct uinput_setup) -> tamaño 92, NO 4. Con el número mal el
# ioctl falla y el script muere en silencio (los eventos nunca se emiten).
UI_DEV_SETUP = 0x405C5503
UI_DEV_CREATE = 0x5501

EV_SYN, EV_KEY, EV_REL = 0, 1, 2
SYN_REPORT = 0
REL_X, REL_Y = 0, 1
BTN_LEFT, BTN_RIGHT = 0x110, 0x111
KEY_LEFTSHIFT, KEY_LEFT, KEY_RIGHT = 42, 105, 106
# ----- teclas que usan los menús (flechas, Enter, ESC) -----
KEY_UP, KEY_DOWN, KEY_ENTER, KEY_ESC, KEY_BACKSPACE = 103, 108, 28, 1, 14
KEY_CODES = {
    "up": KEY_UP,
    "down": KEY_DOWN,
    "left": KEY_LEFT,
    "right": KEY_RIGHT,
    "enter": KEY_ENTER,
    "escape": KEY_ESC,
    "backspace": KEY_BACKSPACE,
}
REL_WHEEL = 8

DEFAULT_LOG = "/tmp/menu.log"
REVEAL_TAG = " reveal"
HIT_TAG = "click derecho"
OVERLAY_TAG = "overlay:"
KEY_TAG = "teclado:"
CAL_TAG = "calendario:"


def emit(fd, etype, code, value):
    """struct input_event: timeval (dos longs) + u16 + u16 + i32 = 24 bytes."""
    os.write(fd, struct.pack("qqHHi", 0, 0, etype, code, value))


def sync(fd):
    emit(fd, EV_SYN, SYN_REPORT, 0)


def rel(fd, dx, dy):
    if dx:
        emit(fd, EV_REL, REL_X, dx)
    if dy:
        emit(fd, EV_REL, REL_Y, dy)
    sync(fd)


def count_tag(path, tag):
    try:
        with open(path, "r", errors="replace") as fh:
            return fh.read().count(tag)
    except OSError:
        return 0


def last_hit(path):
    try:
        with open(path, "r", errors="replace") as fh:
            lines = [line for line in fh if HIT_TAG in line]
    except OSError:
        return None
    if not lines:
        return None
    return lines[-1].split(HIT_TAG, 1)[1].strip()


def last_overlay(path):
    try:
        with open(path, "r", errors="replace") as fh:
            lines = [line for line in fh if OVERLAY_TAG in line]
    except OSError:
        return None
    return lines[-1].strip() if lines else None


def last_key(path):
    try:
        with open(path, "r", errors="replace") as fh:
            lines = [line for line in fh if KEY_TAG in line]
    except OSError:
        return None
    return lines[-1].split(KEY_TAG, 1)[1].strip() if lines else None


def last_calendar(path):
    try:
        with open(path, "r", errors="replace") as fh:
            lines = [line for line in fh if CAL_TAG in line]
    except OSError:
        return None
    return lines[-1].split(CAL_TAG, 1)[1].strip() if lines else None


def make_keyboard(name=b"dockyrs-keys"):
    """Teclado virtual: el otro id de producto, para no confundirlo con el puntero."""
    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
    for code in (KEY_LEFTSHIFT, *KEY_CODES.values()):
        fcntl.ioctl(fd, UI_SET_KEYBIT, code)
    buf = bytearray(struct.pack("4H80sI", 0x03, 0x1234, 0x5679, 1, name[:79], 0))
    fcntl.ioctl(fd, UI_DEV_SETUP, buf)
    fcntl.ioctl(fd, UI_DEV_CREATE)
    return fd


def tap_key(fd, key, times):
    """Un tap por vez (release entre medio): N pasos discretos, sin auto-repeat.

    El auto-repeat del kernel sería otra cosa: acá cada paso tiene que llegar
    como un press suelto, que es lo que la app trata como una flecha.
    """
    for _ in range(times):
        emit(fd, EV_KEY, key, 1)
        sync(fd)
        time.sleep(0.05)
        emit(fd, EV_KEY, key, 0)
        sync(fd)
        time.sleep(0.12)


def tap_shift_arrow(fd, key, times):
    """Shift+flecha cicla el overlay; hace falta un modo abierto (teclado Exclusive).

    Con el dock vertical la banda de pestañas es una columna y las flechas que la
    corren son ↑/↓ además de ←/→.
    """
    for _ in range(times):
        emit(fd, EV_KEY, KEY_LEFTSHIFT, 1)
        emit(fd, EV_KEY, key, 1)
        sync(fd)
        time.sleep(0.05)
        emit(fd, EV_KEY, key, 0)
        emit(fd, EV_KEY, KEY_LEFTSHIFT, 0)
        sync(fd)
        time.sleep(0.45)


def make_device(name=b"dockyrs-test"):
    """Crea el puntero virtual y devuelve su fd (cerrarlo lo destruye)."""
    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    for bit in (EV_KEY, EV_REL):
        fcntl.ioctl(fd, UI_SET_EVBIT, bit)
    for code in (BTN_LEFT, BTN_RIGHT):
        fcntl.ioctl(fd, UI_SET_KEYBIT, code)
    for code in (REL_X, REL_Y):
        fcntl.ioctl(fd, UI_SET_RELBIT, code)
    fcntl.ioctl(fd, UI_SET_RELBIT, REL_WHEEL)
    buf = bytearray(struct.pack("4H80sI", 0x03, 0x1234, 0x5678, 1, name[:79], 0))
    fcntl.ioctl(fd, UI_DEV_SETUP, buf)
    fcntl.ioctl(fd, UI_DEV_CREATE)
    return fd


def park_away(fd):
    """Se aleja de la franja y espera a que el dock se oculte.

    El sensor detecta la transición oculto -> visible: si el dock ya estaba
    visible (por ejemplo el cursor quedó parado sobre la franja en la corrida
    anterior) no hay línea ' reveal' y el script espera para siempre. Arrancar
    siempre desde oculto lo hace idempotente.
    """
    for _ in range(6):
        rel(fd, -600, 600)  # esquina inferior izquierda: bien lejos de la franja
        time.sleep(0.01)
    time.sleep(2.0)  # > autohide_delay (1200ms) para que llegue a ocultarse


def reveal_and_click(
    fd,
    log,
    extra,
    button,
    y_steps,
    then_dx=0,
    then_dy=0,
    double=False,
    wheel=None,
    times=1,
    hover=False,
):
    """Se aleja, satisface la esquina, baja a la franja, barre hasta que el dock
    se revela, avanza `extra` pasos y clickea (o gira la rueda si wheel).
    Devuelve si detectó el reveal."""
    park_away(fd)
    for _ in range(25):
        rel(fd, -40, -40)
        time.sleep(0.005)
    for _ in range(y_steps):
        rel(fd, 0, 2)
        time.sleep(0.008)
    base = count_tag(log, REVEAL_TAG)
    revealed = False
    for _ in range(700):
        rel(fd, 3, 0)
        time.sleep(0.006)
        if count_tag(log, REVEAL_TAG) > base:
            revealed = True
            break
    time.sleep(0.15)
    for _ in range(extra):
        rel(fd, 3, 0)
        time.sleep(0.012)
    time.sleep(0.25)
    if hover:
        # ----- sin click: lo que se prueba es el hover (el calendario se abre
        # después de CALENDAR_HOVER_MS), así que hay que dejarlo quieto -----
        time.sleep(1.0)
        return revealed
    if wheel is not None:
        # ----- rueda en el punto alcanzado (REL_WHEEL: +1 arriba, -1 abajo) -----
        for _ in range(times):
            emit(fd, EV_REL, REL_WHEEL, 1 if wheel == "up" else -1)
            sync(fd)
            time.sleep(0.08)
        time.sleep(0.4)
        return revealed
    emit(fd, EV_KEY, button, 1)
    sync(fd)
    time.sleep(0.06)
    emit(fd, EV_KEY, button, 0)
    sync(fd)
    if double:
        # el dock reconoce el gesto si pasan menos de 400ms y el puntero se movió
        # menos de 12px: el segundo click va pegado al primero, en el mismo punto
        time.sleep(0.08)
        emit(fd, EV_KEY, button, 1)
        sync(fd)
        time.sleep(0.03)
        emit(fd, EV_KEY, button, 0)
        sync(fd)
    time.sleep(0.5)
    # ----- segundo click opcional (izquierdo), tras desplazarse del primero -----
    if then_dx or then_dy:
        for _ in range(abs(then_dx)):
            rel(fd, 3 if then_dx > 0 else -3, 0)
            time.sleep(0.012)
        for _ in range(abs(then_dy)):
            rel(fd, 0, 2 if then_dy > 0 else -2)
            time.sleep(0.012)
        time.sleep(0.25)
        emit(fd, EV_KEY, BTN_LEFT, 1)
        sync(fd)
        time.sleep(0.06)
        emit(fd, EV_KEY, BTN_LEFT, 0)
        sync(fd)
        time.sleep(0.5)
    return revealed


def main():
    parser = argparse.ArgumentParser(
        description="Puntero virtual por uinput para clickear el dock en un punto exacto."
    )
    parser.add_argument("--log", default=os.environ.get("DOCKYRS_LOG", DEFAULT_LOG))
    parser.add_argument("--extra", type=int, default=0)
    parser.add_argument("--button", choices=("L", "R"), default="R")
    parser.add_argument("--y", type=int, default=8, dest="y_steps")
    parser.add_argument("--then-dx", type=int, default=0)
    parser.add_argument("--then-dy", type=int, default=0)
    parser.add_argument("--double", action="store_true")
    parser.add_argument("--shift-arrow", choices=("left", "right", "up", "down"))
    parser.add_argument("--key", choices=tuple(KEY_CODES))
    parser.add_argument("--scroll", choices=("up", "down"))
    parser.add_argument("--hover", action="store_true")
    parser.add_argument("--times", type=int, default=1)
    args = parser.parse_args()

    if args.key:
        fd = make_keyboard()
        try:
            time.sleep(1.6)
            tap_key(fd, KEY_CODES[args.key], args.times)
        finally:
            os.close(fd)
        time.sleep(0.3)
        print(last_key(args.log) or "la tecla no llegó a la app (¿hay un menú abierto?)")
        return 0

    if args.shift_arrow:
        fd = make_keyboard()
        try:
            time.sleep(1.6)
            tap_shift_arrow(
                fd,
                {
                    "left": KEY_LEFT,
                    "right": KEY_RIGHT,
                    "up": KEY_UP,
                    "down": KEY_DOWN,
                }[args.shift_arrow],
                args.times,
            )
        finally:
            os.close(fd)
        time.sleep(0.4)
        print(last_overlay(args.log) or "no hubo cambio de modo (¿hay algún modo abierto?)")
        return 0

    fd = make_device()
    try:
        time.sleep(1.6)  # el dispositivo tarda en ser reconocido
        revealed = reveal_and_click(
            fd,
            args.log,
            args.extra,
            BTN_LEFT if args.button == "L" else BTN_RIGHT,
            args.y_steps,
            args.then_dx,
            args.then_dy,
            args.double,
            args.scroll,
            args.times,
            args.hover,
        )
    finally:
        os.close(fd)

    if not revealed:
        print(
            "no se detectó el reveal: ¿la app corre con RUST_LOG=debug y --log apunta a su log? "
            "¿quedó el panel de ajustes abierto de una corrida anterior?"
        )
        return 1
    time.sleep(0.2)
    if args.hover:
        line = last_calendar(args.log)
        print(line or "el calendario no se abrió (¿quedó el puntero sobre el reloj?)")
        return 0 if line else 1
    print(last_hit(args.log) or "el click no llegó al dock (fuera de la superficie)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
