# NO PROBADO EN HARDWARE. Parche "identidad" para bisecar v1/v3 (ver ../patch-emulation.md, ../lcd-emulation.md).
# Los 6 sitios de llamada del bucle de teclas se desvian a dos stubs en el hueco 0x0801AB9C que solo hacen
# `b.w` a la funcion original (linea 0x08007188 / envio 0x08009B10). El comportamiento deberia ser IDENTICO al original:
# si esta imagen rompe las teclas, el problema es ejecutar codigo en el hueco (o el desvio), no el informe extra de v1/v3.
# Uso: python -I make_patch_identity.py <backup/AKP153E.bin> <salida.bin>
import sys, hashlib
from make_patch_v3 import BASE, CAVE, CAVE_END, F_LINE, F_SEND, bl

PRS, REL = CAVE, CAVE + 4      # stub_press: b.w linea ; stub_release: b.w send

if __name__ == '__main__':
    src, dst = sys.argv[1], sys.argv[2]
    d = bytearray(open(src, 'rb').read())
    assert hashlib.sha256(d).hexdigest() == '15e03f5b5a593d665a382d500d8acf23b7e5aed87f64e4b7da450d1a36f54f6f'
    o = lambda a: a - BASE
    assert bytes(d[o(CAVE):o(CAVE_END)]) == (b'Read from EMS failed\0\0\0\0Write to EMS failed\0'
                                            b'Read from XMS failed\0\0\0\0Write to XMS failed\0')
    d[o(PRS):o(PRS) + 4] = bl(PRS, F_LINE, link=False)
    d[o(REL):o(REL) + 4] = bl(REL, F_SEND, link=False)
    for site in (0x08006F92, 0x08007022, 0x080070A4):
        assert bytes(d[o(site):o(site) + 4]) == bl(site, F_LINE)
        d[o(site):o(site) + 4] = bl(site, PRS)
    for site in (0x08006FAA, 0x0800703A, 0x080070BC):
        assert bytes(d[o(site):o(site) + 4]) == bl(site, F_SEND)
        d[o(site):o(site) + 4] = bl(site, REL)
    open(dst, 'wb').write(d)
    print('sha256', hashlib.sha256(d).hexdigest(), 'len', len(d))
