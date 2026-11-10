# NO PROBADO EN HARDWARE. Variante v2 del parche pulsar/soltar (ver ../patch-emulation.md).
# Diferencia con v1 (make_patch.py): antes de cada envio espera, con limite, a que el EP IN 2 quede libre
# (TxPktRdy=0), para que el informe "soltar" no se descarte si el host aun no ha leido el de "pulsar".
# Usa 88 B de cadenas muertas de libjpeg (EMS/XMS, mensajes 34/35/73/74) en 0x0801AB9C..0x0801ABF3.
# Uso: python -I make_patch_v2.py <backup/AKP153E.bin> <salida.bin>
import sys, struct, hashlib

BASE = 0x08004000
TX = 0x2000093C          # informe IN (0x200 B); [9]=tecla, [10]=estado
P_USB = 0x200000E8       # -> base USB (INDEX en +0x0E)
P_CSR = 0x200000EC       # -> CSR indexado (+2 bit0 = TxPktRdy), igual que 0x08009B5E
F_LINE = 0x08007188
F_SEND = 0x08009B10
CAVE, CAVE_END = 0x0801AB9C, 0x0801ABF4
SPIN = 0x10000           # iteraciones maximas de espera (~1 M ciclos)

def bl(src, dst, link=True):
    off = dst - (src + 4)
    assert -(1 << 24) <= off < (1 << 24) and off % 2 == 0
    s = (off >> 24) & 1
    i1 = (off >> 23) & 1; i2 = (off >> 22) & 1
    j1 = (~(i1 ^ s)) & 1; j2 = (~(i2 ^ s)) & 1
    h1 = 0xF000 | (s << 10) | ((off >> 12) & 0x3FF)
    h2 = (0xD000 if link else 0x9000) | (j1 << 13) | (j2 << 11) | ((off >> 1) & 0x7FF)
    return struct.pack('<HH', h1, h2)

def b16(src, dst):        # B<c> T2 incondicional
    off = dst - (src + 4); assert -2048 <= off < 2048 and off % 2 == 0
    return struct.pack('<H', 0xE000 | ((off >> 1) & 0x7FF))

def bcond(src, dst, cond):  # B<c> T1
    off = dst - (src + 4); assert -256 <= off < 256 and off % 2 == 0
    return struct.pack('<H', 0xD000 | (cond << 8) | ((off >> 1) & 0xFF))

def ldr_lit(src, rt, lit):  # LDR Rt,[PC,#imm] T1
    imm = lit - ((src + 4) & ~3); assert 0 <= imm < 1024 and imm % 4 == 0
    return struct.pack('<H', 0x4800 | (rt << 8) | (imm // 4))

def build():
    REL = CAVE                 # 6 B
    PRS = REL + 6              # 22 B
    WAIT = PRS + 22            # 36 B
    LIT = (WAIT + 36 + 3) & ~3 # 3 literales
    L_TX, L_USB, L_CSR = LIT, LIT + 4, LIT + 8
    c = bytearray()
    # stub_release: TX[10]=0 ; b wait_send   (r0=TX, r1=0x200 del llamador)
    c += struct.pack('<HH', 0x2200, 0x7282)                  # movs r2,#0 ; strb r2,[r0,#10]
    c += b16(REL + 4, WAIT)
    assert len(c) == PRS - CAVE
    # stub_press: linea(1,key) ; TX[10]=1 ; wait_send(TX,0x200)
    c += struct.pack('<H', 0xB510)                           # push {r4,lr}
    c += bl(PRS + 2, F_LINE)
    c += ldr_lit(PRS + 6, 0, L_TX)                           # ldr r0,=TX
    c += struct.pack('<HH', 0x2101, 0x7281)                  # movs r1,#1 ; strb r1,[r0,#10]
    c += struct.pack('<HH', 0xF44F, 0x7100)                  # mov.w r1,#0x200
    c += bl(PRS + 0x10, WAIT)
    c += struct.pack('<H', 0xBD10)                           # pop {r4,pc}
    assert len(c) == WAIT - CAVE
    # wait_send(r0=buf, r1=len): espera acotada a TxPktRdy(EP2)=0 y salta a send. Toca r2,r3 (y r4 salvado).
    c += struct.pack('<H', 0xB510)                           # push {r4,lr}
    c += struct.pack('<HH', 0xF44F, 0x3480)                  # mov.w r4,#0x10000
    LOOP = WAIT + 6
    c += ldr_lit(LOOP, 2, L_USB)                             # ldr r2,=P_USB
    c += struct.pack('<H', 0x6812)                           # ldr r2,[r2]
    c += struct.pack('<H', 0x2302)                           # movs r3,#2
    c += struct.pack('<H', 0x7393)                           # strb r3,[r2,#14]  INDEX=2 (como 0x08009B26)
    c += ldr_lit(LOOP + 8, 2, L_CSR)                         # ldr r2,=P_CSR
    c += struct.pack('<H', 0x6812)                           # ldr r2,[r2]
    c += struct.pack('<H', 0x7892)                           # ldrb r2,[r2,#2]
    c += struct.pack('<H', 0x07D2)                           # lsls r2,r2,#31
    DONE = LOOP + 22
    c += bcond(LOOP + 16, DONE, 0x0)                         # beq done
    c += struct.pack('<H', 0x3C01)                           # subs r4,#1
    c += bcond(LOOP + 20, LOOP, 0x1)                         # bne loop
    assert WAIT + (len(c) - (WAIT - CAVE)) == DONE
    c += struct.pack('<HH', 0xE8BD, 0x4010)                  # pop.w {r4,lr}
    c += bl(DONE + 4, F_SEND, link=False)                    # b.w send
    assert CAVE + len(c) == WAIT + 36
    c += b'\x00' * (LIT - CAVE - len(c))
    c += struct.pack('<III', TX, P_USB, P_CSR)
    assert CAVE + len(c) <= CAVE_END
    return c, REL, PRS, WAIT, LOOP

if __name__ == '__main__':
    src, dst = sys.argv[1], sys.argv[2]
    d = bytearray(open(src, 'rb').read())
    assert hashlib.sha256(d).hexdigest() == '15e03f5b5a593d665a382d500d8acf23b7e5aed87f64e4b7da450d1a36f54f6f'
    o = lambda a: a - BASE
    assert bytes(d[o(CAVE):o(CAVE_END)]) == (b'Read from EMS failed\0\0\0\0Write to EMS failed\0'
                                            b'Read from XMS failed\0\0\0\0Write to XMS failed\0')
    code, REL, PRS, WAIT, LOOP = build()
    d[o(CAVE):o(CAVE) + len(code)] = code
    for site in (0x08006F92, 0x08007022, 0x080070A4):
        assert bytes(d[o(site):o(site) + 4]) == bl(site, F_LINE)
        d[o(site):o(site) + 4] = bl(site, PRS)
    for site in (0x08006FAA, 0x0800703A, 0x080070BC):
        assert bytes(d[o(site):o(site) + 4]) == bl(site, F_SEND)
        d[o(site):o(site) + 4] = bl(site, REL)
    open(dst, 'wb').write(d)
    print('REL', hex(REL), 'PRS', hex(PRS), 'WAIT', hex(WAIT), 'LOOP', hex(LOOP), 'code', len(code))
    print('sha256', hashlib.sha256(d).hexdigest(), 'len', len(d))
