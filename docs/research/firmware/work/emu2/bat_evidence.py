# Evidencia B2: BAT de JPEG de prueba en firmware original vs parche S (tira lateral). Usa el codigo real del firmware.
# Uso: python -I bat_evidence.py   (exit 0 = todas las comprobaciones OK)
import sys, os, hashlib
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
from fw import FW, make_jpeg
from ra8876 import diff_bbox, px, W, H
ORIG = os.path.join(HERE, '..', '..', 'backup', 'AKP153E.bin')
PATCH = os.path.join(HERE, '..', 'AKP153E_sidestrip_NO_PROBADO.bin')
fails = []
def check(c, m):
    print(('OK   ' if c else 'FAIL ') + m)
    if not c: fails.append(m)

# colores BGR en memoria de video (el firmware invierte RGB->BGR); JPEG q90 de color plano ~exacto
SEG = [(198, (255, 0, 0)), (149, (0, 255, 0)), (133, (0, 0, 255))]   # key16, key17, key18 (w, RGB)
H_STRIP = 100

def run(path):
    fw = FW(path); fw.send(fw.cle(0xFF)); fw.send(fw.stp())
    p0 = fw.lcd.page(0); p1 = fw.lcd.page(1)
    for key, (w, c) in zip((16, 17, 18), SEG):
        assert fw.send(fw.bat(key, make_jpeg(w, H_STRIP, lambda x, y: c, quality=95)))[0] == 'ok'
    after_bat_p1 = fw.lcd.page(1)
    fw.send(fw.stp())
    return fw, p0, p1, after_bat_p1

fo, o0, o1, ob1 = run(ORIG)
fp, p0b, p1b, pb1 = run(PATCH)

# 1) parche: rectangulo continuo (0,0)-(479,99) en pagina 1 y, tras STP, en pagina 0 (pantalla)
bb1 = diff_bbox(pb1, p1b); bb0 = diff_bbox(fp.lcd.page(0), p0b)
check(bb1 == (0, 0, 479, H_STRIP - 1), f'parche: BAT 16/17/18 pinta en pagina 1 el rectangulo {bb1}')
check(bb0 == (0, 0, 479, H_STRIP - 1), f'parche: tras STP la pagina frontal cambia en {bb0}')
scr = fp.lcd.display()
def near(a, b, t=12): return all(abs(x - y) <= t for x, y in zip(a, b))
ok = True; bad = 0
for y in (0, 50, 99):
    for x in range(W):
        w0 = 198 if x < 198 else (347 if x < 347 else 480)
        col = SEG[0 if x < 198 else (1 if x < 347 else 2)][1]
        if not near(px(scr, x, y), col[::-1]): bad += 1       # BGR
check(bad == 0, f'parche: 3 filas x 480 px: segmentos 198/149/133 contiguos y de su color ({bad} pixeles fuera)')
check(px(scr, 0, 0) != px(p0b, 0, 0) and px(scr, 479, 99) != px(p0b, 479, 99), 'parche: esquinas (0,0) y (479,99) cambian')
check(px(scr, 100, 100) == px(p0b, 100, 100), 'parche: la fila 100 (justo debajo) no cambia')

# 2) original: mismos JPEG -> celdas separadas y recortadas a 80 px de ancho
bo1 = diff_bbox(ob1, o1)
print('     original: bbox pagina 1 tras los mismos 3 BAT =', bo1)
check(bo1 != (0, 0, 479, H_STRIP - 1), 'original: NO produce el rectangulo continuo')
for key, x in ((16, 49), (17, 198), (18, 347)):
    pass
# huecos en el original: columna x=130..197 (entre celdas) sin tocar
gap = all(px(ob1, x, 50) == px(o1, x, 50) for x in range(130, 198))
check(gap, 'original: hueco entre celdas (x=130..197) sin pintar -> no es continuo')

# 3) el resto de teclas no cambia: BAT 85x85 en las 15 teclas y comparacion byte a byte de la pagina 1
def sweep(path):
    fw = FW(path); fw.send(fw.cle(0xFF))
    for k in range(1, 16):
        jp = make_jpeg(85, 85, lambda x, y, k=k: ((k * 17) % 256, (x * 3) % 256, (y * 3) % 256), quality=90)
        assert fw.send(fw.bat(k, jp))[0] == 'ok'
    return fw.lcd.page(1), fw
so, fwo = sweep(ORIG); sp, fwp = sweep(PATCH)
check(so == sp, 'teclas 1..15 (BAT 85x85): pagina 1 identica byte a byte entre original y parche')
bb = diff_bbox(so, fwo.lcd.page(0)); print('     teclas 1..15 cubren bbox', bb)
check(fwo.lcd.oob == [] and fwp.lcd.oob == [] and not fwo.unmapped and not fwp.unmapped, 'sin accesos fuera de mapa ni a memoria de video fuera de rango')

# 4) diff de bytes
a, b = open(ORIG, 'rb').read(), open(PATCH, 'rb').read()
d = [i for i in range(len(a)) if a[i] != b[i]]
check(len(a) == len(b) == 96496 and len(d) == 7, f'tamano {len(b)} y {len(d)} bytes distintos en {[hex(0x08004000 + i) for i in d]}')
print('sha256 orig ', hashlib.sha256(a).hexdigest()); print('sha256 patch', hashlib.sha256(b).hexdigest())
print('FALLOS' if fails else 'TODO OK', fails); sys.exit(1 if fails else 0)
