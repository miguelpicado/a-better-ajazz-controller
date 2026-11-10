# Emulacion Unicorn del bucle de teclas AKP153E (0x08006EF4). SOLO FICHEROS.
# Uso: python -I emu.py <imagen.bin> [--json]
import sys, struct, json, random
from unicorn import *
from unicorn.arm_const import *

BASE = 0x08004000
TX = 0x2000093C
KEYLOOP = 0x08006EF4
F_LINE = 0x08007188
F_DELAY = 0x0800576C
F_SCREEN = 0x08007C34
F_READCOL = 0x08007108
F_EPSEND = 0x08009B1C
SENTINEL = 0x08000100
USB = 0x4004C000          # MUSB-like: INDEX en +0x0E
CSR = 0x4004C010          # [0x200000EC] -> +2 bit0 = TxPktRdy (ocupado)
FIFO_LO, FIFO_HI = 0x4004C024, 0x4004C044
ROWP_A, ROWP_B = 0x40014000, 0x4001D000   # filas: A.bit0, A.bit1, B.bit1 (activa a 0)
COLP_G = 0x40012000       # +0x0E bits0..2 -> columnas 2,3,4
COLP_E = 0x40019000       # EPORT +4: bit0 -> col 5, bit4 -> col 1

TABLE = bytes.fromhex('00030609 0c0f0104 070a0d02 05080b0e'.replace(' ', ''))
KEYPOS = {}
for row, add in ((0, 10), (1, 5), (2, 0)):
    for col in range(1, 6):
        KEYPOS[TABLE[col + add]] = (row, col)

class Emu:
    def __init__(self, img, scenario, host_latency=0.0, stp=1, screen_off=0, line_mode='stub', seed=1):
        self.img = img; self.sc = scenario; self.lat = host_latency
        self.t = 0.0; self.reports = []; self.attempts = []; self.events = []
        self.fifo = bytearray(); self.pending = None
        self.periph = {}; self.unmapped = []; self.regfail = []; self.wdt = 0
        self.isr_probe = []; self.rng = random.Random(seed); self.line_mode = line_mode
        u = self.u = Uc(UC_ARCH_ARM, UC_MODE_THUMB | UC_MODE_MCLASS)
        u.ctl_set_cpu_model(UC_CPU_ARM_CORTEX_M4)
        u.mem_map(0x08000000, 0x80000, UC_PROT_READ | UC_PROT_EXEC)
        u.mem_write(BASE, img)
        u.mem_write(SENTINEL, b'\x00\xbf\x00\xbf')
        u.mem_map(0x20000000, 0x40000)
        rw = open(__import__('os').path.join(__import__('os').path.dirname(__import__('os').path.abspath(__file__)), '..', 'rwdata.bin'), 'rb').read()
        u.mem_write(0x20000000, rw)
        u.mem_map(0x40000000, 0x100000)       # perifericos genericos (registrados)
        w = lambda a, v: u.mem_write(a, struct.pack('<I', v))
        w(0x200000E8, USB); w(0x200000EC, CSR)
        w(0x200000D8, 0x20030000); w(0x20030000 + 0x14, CSR)
        w(0x20000058, 192000000)
        u.mem_write(0x2000010C, bytes([stp])); u.mem_write(0x20000112, bytes([screen_off]))
        u.mem_write(TX, b'ACK\x00\x00OK\x00\x00' + bytes(0x200 - 9))
        u.mem_write(ROWP_A + 8, b'\xff'); u.mem_write(ROWP_B + 8, b'\xff')
        u.hook_add(UC_HOOK_MEM_READ | UC_HOOK_MEM_WRITE, self.on_periph, begin=0x40000000, end=0x400FFFFF)
        u.hook_add(UC_HOOK_MEM_UNMAPPED, self.on_unmapped)
        u.hook_add(UC_HOOK_INTR, self.on_intr)
        for a in (F_LINE, F_DELAY, F_SCREEN, F_READCOL, F_EPSEND):
            u.hook_add(UC_HOOK_CODE, self.on_fn, begin=a, end=a)
        # v2: cada vuelta del bucle de espera = 0.1 us (~20 ciclos)
        self.spins = 0
        def on_spin(u, a, s, _):
            self.spins += 1; self.advance(0.0001)
        u.hook_add(UC_HOOK_CODE, on_spin, begin=0x0801ABBE, end=0x0801ABBE)
        # sitios de llamada parcheados: registros antes/despues
        self.sites = {}
        for s in (0x08006F92, 0x08006FAA, 0x08007022, 0x0800703A, 0x080070A4, 0x080070BC):
            u.hook_add(UC_HOOK_CODE, self.on_site_pre, begin=s, end=s)
            u.hook_add(UC_HOOK_CODE, self.on_site_post, begin=s + 4, end=s + 4)

    def regs(self, names):
        return [self.u.reg_read(r) for r in names]
    CS = [UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7, UC_ARM_REG_R8,
          UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11, UC_ARM_REG_SP]

    def on_site_pre(self, u, a, s, _):
        self.sites[a] = self.regs(self.CS)
    def on_site_post(self, u, a, s, _):
        pre = self.sites.pop(a - 4, None)
        if pre is not None and pre != self.regs(self.CS):
            self.regfail.append((hex(a - 4), pre, self.regs(self.CS)))

    def pressed(self):
        return [k for (k, t0, t1) in self.sc if t0 <= self.t < t1]

    def advance(self, ms):
        self.t += ms
        if self.pending is not None and self.t >= self.pending + self.lat:
            b = self.u.mem_read(CSR + 2, 1)[0]
            self.u.mem_write(CSR + 2, bytes([b & ~1])); self.pending = None
        for (pt, done) in [(p, None) for p in self.probe_at if p <= self.t and p not in self.probed]:
            self.probed.add(pt)
            tx = self.u.mem_read(TX, 11)
            self.isr_probe.append((round(self.t, 2), tx[9], tx[10]))
    probe_at = []; probed = set()

    def on_periph(self, u, acc, a, size, val, _):
        key = (a & ~3, 'W' if acc == UC_MEM_WRITE else 'R')
        self.periph[key] = self.periph.get(key, 0) + 1
        if acc == UC_MEM_WRITE:
            if FIFO_LO <= a < FIFO_HI:
                self.fifo.append(val & 0xFF)
            elif a == CSR + 2 and (val & 1):
                self.reports.append({'t': round(self.t, 2), 'data': bytes(self.fifo)})
                self.fifo = bytearray(); self.pending = self.t
            elif a == 0x40005006:
                self.wdt += 1
            return
        # lecturas: modelo de matriz activa-baja
        ra = u.mem_read(ROWP_A + 8, 1)[0]; rb = u.mem_read(ROWP_B + 8, 1)[0]
        active = set()
        if not ra & 1: active.add(0)
        if not ra & 2: active.add(1)
        if not rb & 2: active.add(2)
        low = set()
        for k in self.pressed():
            r, c = KEYPOS[k]
            if r in active: low.add(c)
        if COLP_G + 0x0C <= a <= COLP_G + 0x0F:
            v = 0xFF
            for c, bit in ((2, 0), (3, 1), (4, 2)):
                if c in low: v &= ~(1 << bit)
            u.mem_write(COLP_G + 0x0E, bytes([v]))
        if COLP_E + 4 <= a <= COLP_E + 7:
            v = 0xFF
            if 5 in low: v &= ~1
            if 1 in low: v &= ~0x10
            u.mem_write(COLP_E + 4, bytes([v]))

    def on_unmapped(self, u, acc, a, size, val, _):
        self.unmapped.append((hex(u.reg_read(UC_ARM_REG_PC)), hex(a), acc)); return False
    def on_intr(self, u, intno, _):
        self.unmapped.append(('INTR', intno, hex(u.reg_read(UC_ARM_REG_PC)))); u.emu_stop()

    def ret(self):
        lr = self.u.reg_read(UC_ARM_REG_LR); self.u.reg_write(UC_ARM_REG_PC, lr | 1)

    def on_fn(self, u, a, s, _):
        if a == F_DELAY:
            self.advance(float(u.reg_read(UC_ARM_REG_R0))); self.ret()
        elif a == F_READCOL:
            self.advance(0.02)
        elif a == F_SCREEN:
            self.events.append(('screen', round(self.t, 2), u.reg_read(UC_ARM_REG_R0))); self.ret()
        elif a == F_LINE:
            r0, r1 = self.regs([UC_ARM_REG_R0, UC_ARM_REG_R1])
            self.events.append(('line', round(self.t, 2), r0, r1))
            if self.line_mode == 'stub':   # devuelve con caller-saved y flags basura
                for r in (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3, UC_ARM_REG_R12):
                    u.reg_write(r, 0xDEAD0000 | self.rng.randrange(0x10000))
                u.reg_write(UC_ARM_REG_APSR, self.rng.choice([0, 0xF0000000, 0x40000000, 0x20000000]))
                self.advance(2.0); self.ret()
        elif a == F_EPSEND:
            r0, r1, r2 = self.regs([UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2])
            busy = u.mem_read(CSR + 2, 1)[0] & 1
            snap = bytes(u.mem_read(r1, min(r2, 0x200))) if 0x20000000 <= r1 < 0x20040000 else b''
            self.attempts.append({'t': round(self.t, 2), 'ep': r0, 'buf': hex(r1), 'len': r2, 'busy': busy,
                                  'k': snap[9] if snap else None, 's': snap[10] if snap else None})

    def run(self, t_end, budget=40_000_000):
        u = self.u
        sp0 = 0x200321C0
        calls = 0
        while self.t < t_end:
            canary = [0x4A4A0000 + i for i in range(8)]
            for r, v in zip(self.CS[:8], canary): u.reg_write(r, v)
            u.reg_write(UC_ARM_REG_SP, sp0); u.reg_write(UC_ARM_REG_LR, SENTINEL | 1)
            try:
                u.emu_start(KEYLOOP | 1, SENTINEL, count=budget)
            except UcError as e:
                self.unmapped.append(('UCERR', str(e), hex(u.reg_read(UC_ARM_REG_PC)))); break
            pc = u.reg_read(UC_ARM_REG_PC)
            if pc != SENTINEL:
                self.unmapped.append(('NORETURN/budget', hex(pc), round(self.t, 2))); break
            after = self.regs(self.CS)
            if after[:8] != canary or after[8] != sp0:
                self.regfail.append(('keyloop-return', [hex(x) for x in after]))
            calls += 1
        self.calls = calls
        return self

def summarize(e):
    rep = []
    for r in e.reports:
        d = r['data']
        rep.append({'t': r['t'], 'len': len(d), 'hdr': d[:9].hex(), 'key': d[9] if len(d) > 10 else None,
                    'state': d[10] if len(d) > 10 else None, 'rest_zero': all(b == 0 for b in d[11:])})
    return rep

if __name__ == '__main__':
    img = open(sys.argv[1], 'rb').read()
    e = Emu(img, [(5, 10, 110)]).run(200)
    print(summarize(e)); print(e.events); print(e.attempts); print(e.unmapped, e.regfail, e.wdt, e.calls)
    print(sorted((hex(a), m, n) for (a, m), n in e.periph.items()))
