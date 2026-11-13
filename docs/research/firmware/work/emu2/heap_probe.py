# Anchura maxima de JPEG decodificable por el firmware (libjpeg 9a con heap malloc estatico 0x20028CC4..0x200301C0 = 29 948 B,
# malloc 0x08016658). Ejecuta el codigo REAL: BAT de un JPEG de WxH y cuenta filas pintadas. Uso: python -I heap_probe.py <fw.bin>
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fw import FW, make_jpeg
img = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'backup', 'AKP153E.bin')
fw = FW(img); fw.send(fw.cle(0xFF))

def rows(w, sub, h=16):
    n = len(fw.lcd.pix_log)
    r = fw.send(fw.bat(8, make_jpeg(w, h, lambda x, y: (x % 256, y * 8 % 256, 90), subsampling=sub)))
    assert r[0] == 'ok', r
    return len(fw.lcd.pix_log) - n

res = {}
for sub, name in ((0, '4:4:4'), (2, '4:2:0')):
    lo, hi = 85, 480                       # lo funciona, hi se comprueba
    if rows(hi, sub) == 16: lo = hi
    while hi - lo > 4 and lo != 480:
        mid = (lo + hi) // 2
        if rows(mid, sub) == 16: lo = mid
        else: hi = mid
    res[name] = lo
    print(f'{name}: ancho maximo decodificado ~{lo} px (primer fallo <= {hi}); 480 px -> filas {rows(480, sub)}')
# una decodificacion fallida, hay fuga de heap? (BAT 85x85 despues)
rows(480, 0); print('tras fallo 480 px, BAT 85 px pinta filas:', rows(85, 0))
sys.exit(0)
