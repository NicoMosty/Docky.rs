#!/usr/bin/env python3
"""Click en una coordenada ABSOLUTA de la pantalla, con el puntero virtual de uinput.

`pointer.py` no sirve para esto por dos motivos:

- su sensor es el `reveal`, así que necesita que el dock pase de oculto a visible. Con
  un panel del overlay abierto el dock ya está visible y no hay transición: el script
  se queda esperando para siempre.
- barre la franja de ARRIBA (pensada para el dock `Top`). Con el dock lateral o en una
  salida ultrawide el punto de llegada no es una coordenada conocida.

Acá el puntero se parkea en la esquina superior IZQUIERDA (clamping) y de ahí se mueve en
deltas conocidos hasta (x, y), con los mismos pasos y tiempos que `pointer.py`
(~3.05 px reales por paso de 3 y ~2.04 por paso de 2, que es la aceleración de libinput
medida). El error es de unos pocos px: alcanza para una banda de pestañas o un widget,
no para un borde de 1 px.

Ejemplos (con el panel abierto por IPC, que es cuando no hay reveal):

    dockyrs --toggle-search                     # abre el launcher (dock lateral o Top)
    python3 scripts/click_at.py 47 346          # click en la 2ª pestaña (dock Left)
    python3 scripts/click_at.py 1720 47         # click en la 3ª pestaña (dock Top, 3440)
    python3 scripts/click_at.py 110 358 --only-move   # sólo hover (mueve y espera)

Imprime lo que la app logueó del click si el dock corre con `RUST_LOG=debug`
(`--log` para apuntar a otro archivo), que es el sensor: `overlay: click en la banda
(px,py) -> <Modo>` o `dock: click (x,y) -> Some(Kind)`.
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pointer as P  # noqa: E402


def click_at(fd, x: float, y: float, only_move: bool = False) -> None:
    # ----- park + subir a la esquina: queda en (0, 0) de la salida donde estaba -----
    P.park_away(fd)
    for _ in range(25):
        P.rel(fd, -40, -40)
        time.sleep(0.005)
    for _ in range(round(x / 3.05)):
        P.rel(fd, 3, 0)
        time.sleep(0.008)
    for _ in range(round(y / 2.04)):
        P.rel(fd, 0, 2)
        time.sleep(0.008)
    time.sleep(0.3)
    if only_move:
        # ----- hover: dejarlo quieto (lo que se pruebe necesita su plazo) -----
        time.sleep(1.0)
        return
    P.emit(fd, P.EV_KEY, P.BTN_LEFT, 1)
    P.sync(fd)
    time.sleep(0.06)
    P.emit(fd, P.EV_KEY, P.BTN_LEFT, 0)
    P.sync(fd)
    time.sleep(0.6)


def main() -> None:
    ap = argparse.ArgumentParser(
        description="Click en una coordenada absoluta con el puntero virtual de uinput."
    )
    ap.add_argument("x", type=float)
    ap.add_argument("y", type=float)
    ap.add_argument("--log", default=os.environ.get("DOCKYRS_LOG", P.DEFAULT_LOG))
    ap.add_argument(
        "--only-move",
        action="store_true",
        help="no clickea: deja el puntero ahí (para el hover)",
    )
    args = ap.parse_args()

    antes = P.count_tag(args.log, "click")
    fd = P.make_device()
    try:
        click_at(fd, args.x, args.y, args.only_move)
    finally:
        os.close(fd)
    if args.only_move:
        print(f"puntero en ({args.x:g}, {args.y:g})")
        return
    print(P.last_hit(args.log) or P.last_overlay(args.log) or "el click no llegó a la app")
    if P.count_tag(args.log, "click") == antes:
        print(f"(ojo: no hay línea nueva de click en {args.log})", file=sys.stderr)


if __name__ == "__main__":
    main()
