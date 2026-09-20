#!/usr/bin/env python3
"""Ubica los widgets del dock VERTICAL (borde Left) inyectando clicks reales.

`pointer.py` barre la franja de arriba (dock en `Top`), asi que con el dock al
costado no ve el reveal. Aca: se parkea por clamping en (0,0), se baja hasta el
blob de la isla (centrado en la vertical) para revelar, se baja al extremo de la
barra y se sube de a poco clickeando con el boton derecho. El log del dock es el
sensor: cada click imprime `dock: click derecho (x,y) -> Some(Kind)`, con las
coordenadas LOGICAS de la superficie, que es la verdad de donde cayo.

Uso: scripts/sweep_vertical.py [--clicks 40] [--px 3] [--log /tmp/dockyrs.log]
"""
import argparse
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
import pointer as P  # noqa: E402


def ultimo_hit(log):
    h = P.last_hit(log)
    return h or "(sin click registrado)"


def click_derecho(fd):
    P.emit(fd, P.EV_KEY, P.BTN_RIGHT, 1)
    P.sync(fd)
    time.sleep(0.05)
    P.emit(fd, P.EV_KEY, P.BTN_RIGHT, 0)
    P.sync(fd)
    time.sleep(0.16)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--log", default="/tmp/dockyrs.log")
    ap.add_argument("--clicks", type=int, default=40)
    ap.add_argument("--px", type=int, default=4, help="pixeles por paso (par)")
    ap.add_argument("--dir", choices=["down", "up"], default="down")
    args = ap.parse_args()

    fd = P.make_device()
    kb = P.make_keyboard()
    try:
        P.park_away(fd)
        # ----- `park_away` deja el puntero abajo a la izquierda (clamping), asi que
        # para entrar al dock HAY QUE SUBIR: el blob de la isla esta centrado en la
        # vertical. Se sube de a poco y se corta en el primer ` reveal` del log. -----
        base = P.count_tag(args.log, P.REVEAL_TAG)
        revelado = False
        for _ in range(400):
            P.rel(fd, 0, -2)
            time.sleep(0.003)
            if P.count_tag(args.log, P.REVEAL_TAG) > base:
                revelado = True
                break
        time.sleep(0.35)
        print(f"reveal: {revelado}")
        if not revelado:
            return
        # ----- desplazarse CLICKEANDO en cada paso: asi cada posicion queda
        # registrada en el log (que es el sensor) y si el puntero se sale de la
        # barra se ve enseguida (un paso sin linea de click). Se arranca en el blob
        # de la isla, que esta en el medio de la barra: bajar llega al slot `Right`
        # (Tray + Clock en este setup) y subir al `Left`. -----
        signo = 2 if args.dir == "down" else -2
        for i in range(args.clicks):
            click_derecho(fd)
            print(f"  paso {i:3d} ({i * args.px}px hacia {args.dir}) -> {ultimo_hit(args.log)}")
            P.tap_key(kb, P.KEY_ESC, 1)
            for _ in range(max(1, args.px // 2)):
                P.rel(fd, 0, signo)
                time.sleep(0.002)
            time.sleep(0.06)
    finally:
        os.close(fd)
        os.close(kb)


if __name__ == "__main__":
    main()
