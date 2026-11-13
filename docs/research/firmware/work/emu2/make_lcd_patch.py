# NO PROBADO EN HARDWARE. Parche "tira lateral continua" (S) para AKP153E V2.293S.00.003 (ver ../../lcd-emulation.md).
# Redefine las 3 celdas laterales de BAT (teclas 16, 17, 18) como 3 segmentos contiguos que cubren todo el ancho del panel
# (x = 0..479): tecla 16 x=0 w=198, tecla 17 x=198 w=149, tecla 18 x=347 w=133; y = Y0 (por defecto 0). Solo cambia
# constantes inmediatas `movs rN,#imm8` de la tabla de 0x08006D2A (tbb). No toca STP/CLE/linea/teclas 1..15.
# Uso: python -I make_lcd_patch.py <backup/AKP153E.bin> <salida.bin> [--y 0]
import sys, hashlib
ORIG_SHA = '15e03f5b5a593d665a382d500d8acf23b7e5aed87f64e4b7da450d1a36f54f6f'
BASE = 0x08004000

def movs(rd, imm):
    assert 0 <= imm <= 255
    return bytes([imm, 0x20 | rd])

# (direccion, bytes originales, bytes nuevos, descripcion)
def edits(y0=0):
    E = []
    # tecla 16: str r3,[sp] @6E24 ; movs r3,#0x50 @6E26 ; movs r2,#0xc @6E28 ; movs r1,#0x31 @6E2A
    E += [(0x08006E26, movs(3, 0x50), movs(3, 198), 'key16 w 0x50 -> 198 (0xC6)'),
          (0x08006E28, movs(2, 0x0C), movs(2, y0), 'key16 y 12 -> %d' % y0),
          (0x08006E2A, movs(1, 0x31), movs(1, 0), 'key16 x 49 -> 0')]
    # tecla 17: movs r3 @6E36 ; movs r2 @6E38 ; movs r1,#0xc6 @6E3A (x=198 se conserva)
    E += [(0x08006E36, movs(3, 0x50), movs(3, 149), 'key17 w 0x50 -> 149 (0x95)'),
          (0x08006E38, movs(2, 0x0C), movs(2, y0), 'key17 y 12 -> %d' % y0)]
    # tecla 18: movs r3 @6E46 ; movs r2 @6E48 ; x = r1 por defecto 0x15B=347 (movw @08006D0A, se conserva)
    E += [(0x08006E46, movs(3, 0x50), movs(3, 133), 'key18 w 0x50 -> 133 (0x85)'),
          (0x08006E48, movs(2, 0x0C), movs(2, y0), 'key18 y 12 -> %d' % y0)]
    return E

def apply(d, y0=0):
    for a, old, new, _ in edits(y0):
        o = a - BASE
        assert bytes(d[o:o + 2]) == old, (hex(a), bytes(d[o:o + 2]).hex(), old.hex())
        d[o:o + 2] = new
    return d

if __name__ == '__main__':
    src, dst = sys.argv[1], sys.argv[2]
    y0 = int(sys.argv[sys.argv.index('--y') + 1]) if '--y' in sys.argv else 0
    d = bytearray(open(src, 'rb').read())
    assert hashlib.sha256(d).hexdigest() == ORIG_SHA, 'no es el firmware V2.293S.00.003 esperado'
    apply(d, y0)
    open(dst, 'wb').write(d)
    for a, old, new, dsc in edits(y0): print('%08X: %s -> %s   %s' % (a, old.hex(), new.hex(), dsc))
    print('sha256', hashlib.sha256(d).hexdigest(), 'len', len(d))
