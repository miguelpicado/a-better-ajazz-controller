# NO PROBADO. Genera una imagen parcheada SOLO para analisis estatico (diff/desensamblado).
# No existe en este equipo ninguna herramienta para flashear el AKP153E; no intentar flashear.
# Uso: python -I make_patch.py <backup/AKP153E.bin> <salida.bin>
import sys, struct, hashlib

BASE = 0x08004000
TX = 0x2000093C          # buffer del informe IN (0x200 B); [9]=tecla, [10]=estado
F_LINE = 0x08007188      # efecto local (linea amarilla): (on, key)
F_SEND = 0x08009B10      # envio del informe por el EP IN (copia sincrona; descarta si EP ocupado)
CAVE = 0x0801AB9C        # cadenas libjpeg "Read/Write from EMS failed" (EMS/XMS no se usa en MCU)
CAVE_END = 0x0801ABC8

def bl(src, dst, link=True):
    off = dst - (src + 4)
    assert -(1 << 24) <= off < (1 << 24) and off % 2 == 0
    s = (off >> 24) & 1
    i1 = (off >> 23) & 1; i2 = (off >> 22) & 1
    j1 = (~(i1 ^ s)) & 1; j2 = (~(i2 ^ s)) & 1
    imm10 = (off >> 12) & 0x3FF; imm11 = (off >> 1) & 0x7FF
    h1 = 0xF000 | (s << 10) | imm10
    h2 = (0xD000 if link else 0x9000) | (j1 << 13) | (j2 << 11) | imm11
    return struct.pack('<HH', h1, h2)

src, dst = sys.argv[1], sys.argv[2]
d = bytearray(open(src, 'rb').read())
assert hashlib.sha256(d).hexdigest() == '15e03f5b5a593d665a382d500d8acf23b7e5aed87f64e4b7da450d1a36f54f6f'
o = lambda a: a - BASE

REL = CAVE            # stub_release: TX[10]=0 y salto a send (r0=TX, r1=0x200 los pone el llamador)
PRS = CAVE + 8        # stub_press: linea amarilla + TX[10]=1 + send
LIT = CAVE + 0x20
code = bytearray()
code += struct.pack('<HH', 0x2200, 0x7282)            # movs r2,#0 ; strb r2,[r0,#10]
code += bl(REL + 4, F_SEND, link=False)                # b.w send
assert len(code) == 8
code += struct.pack('<H', 0xB510)                      # push {r4,lr}
code += bl(PRS + 2, F_LINE)                            # bl linea(1,key)  (r0=1, r1=key del llamador)
ldr_at = PRS + 6
imm = (LIT - ((ldr_at + 4) & ~3)) // 4
code += struct.pack('<H', 0x4800 | imm)                # ldr r0,=TX
code += struct.pack('<HH', 0x2101, 0x7281)             # movs r1,#1 ; strb r1,[r0,#10]
code += struct.pack('<HH', 0xF44F, 0x7100)             # mov.w r1,#0x200
code += bl(PRS + 0x10, F_SEND)                         # bl send(TX,0x200)  -> informe "pulsar"
code += struct.pack('<H', 0xBD10)                      # pop {r4,pc}
code += b'\x00' * (LIT - CAVE - len(code))
code += struct.pack('<I', TX)
assert CAVE + len(code) <= CAVE_END
d[o(CAVE):o(CAVE) + len(code)] = code

# 3 filas de la matriz: mismo patron en cada bloque
for site in (0x08006F92, 0x08007022, 0x080070A4):      # bl 0x8007188 (pulsar)
    assert bytes(d[o(site):o(site) + 4]) == bl(site, F_LINE)
    d[o(site):o(site) + 4] = bl(site, PRS)
for site in (0x08006FAA, 0x0800703A, 0x080070BC):      # bl 0x8009b10 (tras soltar)
    assert bytes(d[o(site):o(site) + 4]) == bl(site, F_SEND)
    d[o(site):o(site) + 4] = bl(site, REL)

open(dst, 'wb').write(d)
print('sha256', hashlib.sha256(d).hexdigest(), 'len', len(d))
