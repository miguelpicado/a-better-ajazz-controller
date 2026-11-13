# Evidencia A1/A2: DRA estado 3, DRA 0xB1 (estado 4), MOD 0x31 + DRA 0xB4 y DRA estado 3 + STP contra el modelo RA8876.
# Ejecuta el firmware real. Uso: python -I scenarios_a.py   (exit 0 = comprobaciones OK)
import sys, os
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
from fw import FW, make_jpeg
from ra8876 import diff_bbox, px, W, H, PAGE
from unicorn import UC_HOOK_CODE
from unicorn.arm_const import UC_ARM_REG_R0
IMG = os.path.join(HERE, '..', '..', 'backup', 'AKP153E.bin')
fails = []
def check(c, m):
    print(('OK   ' if c else 'FAIL ') + m)
    if not c: fails.append(m)

def newfw():
    fw = FW(IMG); fw.errs = []
    def ee(u, a, s, _):
        err = fw.r32(u.reg_read(UC_ARM_REG_R0)); fw.errs.append(fw.r32(err + 0x14))
    fw.u.hook_add(UC_HOOK_CODE, ee, begin=0x08016A10, end=0x08016A10)   # error_exit de libjpeg (0x08005842)
    fw.send(fw.cle(0xFF)); fw.send(fw.stp())
    fw.ref = [fw.lcd.page(p) for p in range(3)]
    return fw
def jp(w, h, c=(0, 200, 0)): return make_jpeg(w, h, lambda x, y: c, quality=95)
def diffs(fw): return [diff_bbox(fw.lcd.page(p), fw.ref[p]) for p in range(3)]
def show(n, fw, r):
    l = fw.lcd
    print(f'     {n}: {r[0]} estado={fw.state()} canvas={l.canvas():#x} MISA={l.misa():#x} filas-escritas={len(l.pix_log)} BTE={len(l.bte_log)} libjpeg_msg={fw.errs} diff(p0,p1,p2)={diffs(fw)}')

# ---- 480 px de ancho (como las pruebas de hardware 480x107)
f = newfw(); r = f.send(f.dra(0, jp(480, 107), 0, 200, 480, 107)); show('DRA estado 3, 480x107', f, r)
check(r[0] == 'ok' and f.errs == [56] and not f.lcd.pix_log and diffs(f) == [None] * 3, 'DRA3 480 px: libjpeg "Insufficient memory (case 4)" (msg 56), 0 filas escritas, ninguna pagina cambia -> pantalla como estaba (negra tras CLE ff)')
r = f.send(f.bat(8, jp(85, 85, (255, 0, 0)))); show('  luego BAT key 8 85x85', f, r)
check(r[0] == 'ok' and len(f.lcd.pix_log) == 85, 'tras el fallo de DRA3 el BAT siguiente funciona en emulacion (sin fuga de heap; el CUELGUE observado NO se reproduce: UNKNOWN)')

f = newfw(); r = f.send(f.dra(0xB1, jp(480, 107), 0, 200, 480, 107)); show('DRA 0xB1 (estado 4), 480x107', f, r)
check(r[0] == 'ok' and f.errs == [56] and f.lcd.canvas() == 0x258780 and diffs(f) == [None] * 3, 'DRA 0xB1 480 px: mismo fallo; deja canvas=0x258780 y no copia nada; sin cuelgue en emulacion (UNKNOWN en hardware)')

f = newfw(); f.send(f.mod(0x31)); r = f.send(f.dra(0xB4, jp(480, 107), 0, 200, 480, 107)); show('MOD 0x31 + DRA 0xB4, 480x107', f, r)
d = diffs(f)
check(f.errs == [56] and d[0] == (0, 0, 479, 853) and d[2] is None and f.lcd.page(0) == f.lcd.page(2), 'MOD31+DRA 0xB4 480 px: decodificacion falla, PERO la copia pagina 2 -> 0 (480x854) se ejecuta: la pantalla muestra la pagina 2 sin escribir (SDRAM sin inicializar = ruido) y no cambia con mas tiras')

# ---- 240 px de ancho (cabe en el heap)
f = newfw(); r = f.send(f.dra(0, jp(240, 107), 0, 200, 240, 107)); show('DRA estado 3, 240x107', f, r)
check(f.errs == [] and diffs(f)[0] == (0, 200, 239, 306) and diffs(f)[1:] == [None, None], 'DRA3 240 px: pinta (0,200)-(239,306) directo en la pagina frontal')
f.send(f.stp()); show('  luego STP', f, ('ok',))
check(diffs(f)[0] is None or diff_bbox(f.lcd.page(0), f.ref[0]) is None, 'STP posterior copia pagina 1 sobre la 0 y BORRA lo pintado por DRA3 (hay que repintarlo tras cada STP)')

f = newfw(); f.send(f.mod(0x31)); r = f.send(f.dra(0xB4, jp(240, 107), 0, 200, 240, 107)); show('MOD 0x31 + DRA 0xB4, 240x107', f, r)
d = diffs(f)
check(f.errs == [] and d[2] == (0, 200, 239, 306) and d[0] == (0, 0, 479, 853), 'MOD31+DRA 0xB4 240 px: la tira cae en pagina 2 y la copia 480x854 sobrescribe TODA la pantalla (teclas incluidas)')

# DRA3 de 85x85 en la zona lateral (A2: sin cambiar firmware)
f = newfw(); r = f.send(f.dra(0, jp(85, 85, (255, 255, 0)), 40, 10, 85, 85)); show('DRA estado 3, 85x85 en (40,10)', f, r)
check(diffs(f)[0] == (40, 10, 124, 94), 'DRA3 85x85 en (40,10) pinta exactamente (40,10)-(124,94)')
print('FALLOS' if fails else 'TODO OK', fails); sys.exit(1 if fails else 0)
