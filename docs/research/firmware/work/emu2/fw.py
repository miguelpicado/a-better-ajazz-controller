# Arnes de emulacion Unicorn del firmware AKP153E con modelo RA8876 (ra8876.py). SOLO FICHEROS: no toca HID ni hardware.
# Ejecuta el codigo REAL del firmware (parser CRT 0x08006618, receptor 0x0800983C, BAT/DRA/CLE/STP, libjpeg,
# escritor de filas 0x08007D28, BTE) y sustituye solo las primitivas de bus del LCD por el modelo.
# Requiere: pip install unicorn  (venv).  Uso como biblioteca desde scenarios_a.py / bat_evidence.py.
import os, struct, io
from unicorn import *
from unicorn.arm_const import *
from ra8876 import RA8876, PAGE, W, H

BASE = 0x08004000
HERE = os.path.dirname(os.path.abspath(__file__))
RWDATA = os.path.join(HERE, '..', 'rwdata.bin')
SENT = 0x08000100
RX = 0x2000073C                 # buffer de recepcion USB (512 B)
ST = 0x2000010C                 # estructura de estado: +1 estado, +0xA clave, +0xE bytes recibidos, +0x10 n.o de paquete
PBUF = 0x20014C38               # datos de imagen (0x14000 B)
LEN = 0x200000CC                # longitud anunciada
MODE = 0x20000050               # modo (MOD)
F_IDX, F_RD, F_WR, F_ST = 0x080076C4, 0x080076C8, 0x080076CC, 0x0800788A
F_CS, F_SPIB, F_DMAARM, F_DMASEND = 0x08006194, 0x08008930, 0x08008870, 0x080087F4
F_DELAY, F_LOG, F_WDT, F_ACK = 0x0800576C, 0x0800A4E0, 0x0800A474, 0x0800A018
F_LINE_RECT = 0x080064E4
SP0 = 0x200321C0


class FW:
    def __init__(self, img_path, lcd=None, line_hook=False):
        self.img = open(img_path, 'rb').read()
        self.lcd = lcd or RA8876()
        self.cs_low = False
        self.mem_mode = False            # entre "0x80" (escritura de datos) y CS alto
        self.ack = 0; self.logs = 0; self.wdt = 0
        self.unmapped = []; self.events = []; self.delay_ms = 0.0
        self.spi_bytes = []
        self.rects = []                  # llamadas a 0x080064E4 (marco de la tecla)
        self.min_sp = SP0
        u = self.u = Uc(UC_ARCH_ARM, UC_MODE_THUMB | UC_MODE_MCLASS)
        u.ctl_set_cpu_model(UC_CPU_ARM_CORTEX_M4)
        u.mem_map(0x08000000, 0x80000, UC_PROT_ALL)
        u.mem_write(BASE, self.img)
        u.mem_write(SENT, b'\x00\xbf\x00\xbf')
        u.mem_map(0x20000000, 0x40000)
        u.mem_write(0x20000000, open(RWDATA, 'rb').read())
        u.mem_map(0x40000000, 0x100000)
        u.mem_write(0x40013005, bytes([0xFF]))      # UART de depuracion (printf 0x0800A500 -> 0x0800DAF0 espera TXE bit6): siempre listo
        u.hook_add(UC_HOOK_MEM_UNMAPPED, self._unm)
        u.hook_add(UC_HOOK_INTR, lambda uc, n, _: (self.unmapped.append(('INTR', n, hex(uc.reg_read(UC_ARM_REG_PC)))), uc.emu_stop()))
        h = {F_IDX: self._idx, F_RD: self._rd, F_WR: self._wr, F_ST: self._st, F_CS: self._cs, F_SPIB: self._spib,
             F_DMAARM: self._ret, F_DMASEND: self._dma, F_DELAY: self._delay, F_LOG: self._log, F_WDT: self._wdt,
             F_ACK: self._ackf}
        if line_hook:
            h[F_LINE_RECT] = self._rect
        for a, f in h.items():
            u.hook_add(UC_HOOK_CODE, f, begin=a, end=a)
        # estado de arranque razonable (OBSERVED en main 0x08016400; aqui solo lo imprescindible)
        w = lambda a, v: u.mem_write(a, struct.pack('<I', v))
        w(0x20000058, 192000000)
        u.mem_write(MODE, b'\x32')       # modo de arranque (0x080164CA)

    # ---- hooks
    def _unm(self, u, acc, a, size, val, _):
        self.unmapped.append((hex(u.reg_read(UC_ARM_REG_PC)), hex(a), acc)); return False

    def _sp(self, u, a, s, _):           # pila minima (profundidad maxima usada)
        sp = u.reg_read(UC_ARM_REG_SP)
        if sp < self.min_sp: self.min_sp = sp

    def _ret(self, u, *a):
        u.reg_write(UC_ARM_REG_PC, u.reg_read(UC_ARM_REG_LR) | 1)

    def _idx(self, u, *a): self.lcd.write_index(u.reg_read(UC_ARM_REG_R0)); self._ret(u)
    def _wr(self, u, *a): self.lcd.write_data(u.reg_read(UC_ARM_REG_R0)); self._ret(u)
    def _rd(self, u, *a): u.reg_write(UC_ARM_REG_R0, self.lcd.read_data()); self._ret(u)
    def _st(self, u, *a): u.reg_write(UC_ARM_REG_R0, self.lcd.status()); self._ret(u)

    def _cs(self, u, *a):
        port, lvl = u.reg_read(UC_ARM_REG_R0), u.reg_read(UC_ARM_REG_R1)
        if port == 0xC:
            self.cs_low = (lvl == 0)
            if lvl: self.mem_mode = False
        self._ret(u)

    def _spib(self, u, *a):
        b = u.reg_read(UC_ARM_REG_R0) & 0xFF
        self.spi_bytes.append(b)
        if b == 0x80 and self.cs_low and self.lcd.idx == 0x04:
            self.mem_mode = True
        u.reg_write(UC_ARM_REG_R0, 0); self._ret(u)

    def _dma(self, u, *a):
        p, n = u.reg_read(UC_ARM_REG_R0), u.reg_read(UC_ARM_REG_R1)
        if self.mem_mode:
            self.lcd.pixel_write(bytes(u.mem_read(p, n)))
        else:
            self.events.append(('dma-sin-modo-memoria', hex(p), n))
        self._ret(u)

    def _delay(self, u, *a): self.delay_ms += u.reg_read(UC_ARM_REG_R0); self._ret(u)
    def _log(self, u, *a): self.logs += 1; self._ret(u)
    def _wdt(self, u, *a): self.wdt += 1; self._ret(u)
    def _ackf(self, u, *a): self.ack += 1; self._ret(u)

    def _rect(self, u, *a):
        r = [u.reg_read(x) for x in (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3)]
        sp = u.reg_read(UC_ARM_REG_SP)
        st = struct.unpack('<4I', bytes(u.mem_read(sp, 16)))
        self.rects.append((tuple(r), st))
        self._ret(u)

    # ---- memoria
    def rd(self, a, n): return bytes(self.u.mem_read(a, n))
    def wr(self, a, b): self.u.mem_write(a, bytes(b))
    def r8(self, a): return self.rd(a, 1)[0]
    def r16(self, a): return struct.unpack('<H', self.rd(a, 2))[0]
    def r32(self, a): return struct.unpack('<I', self.rd(a, 4))[0]

    # ---- ejecucion
    def call(self, addr, args=(), budget=60_000_000, sp=SP0):
        u = self.u
        regs = [UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3]
        for r, v in zip(regs, args): u.reg_write(r, v)
        u.reg_write(UC_ARM_REG_SP, sp); u.reg_write(UC_ARM_REG_LR, SENT | 1)
        self.min_sp = sp
        try:
            u.emu_start(addr | 1, SENT, count=budget)
        except UcError as e:
            self.unmapped.append(('UCERR', str(e), hex(u.reg_read(UC_ARM_REG_PC))))
            return ('fault', u.reg_read(UC_ARM_REG_PC))
        pc = u.reg_read(UC_ARM_REG_PC)
        if pc != SENT:
            return ('hang', pc)          # presupuesto agotado: bucle sin salida (la ISR USB no vuelve)
        return ('ok', u.reg_read(UC_ARM_REG_R0))

    # ---- USB: paquetes de 512 B hacia el receptor 0x0800983C (el mismo que llama la ISR via 0x08009F8C)
    def rx_packet(self, pkt):
        assert len(pkt) == 512
        self.wr(RX, pkt)
        self.wr(ST + 0xE, struct.pack('<H', 0x200))
        return self.call(0x0800983C)

    def send(self, packets, label=''):
        res = None
        for p in packets:
            res = self.rx_packet(p)
            if res[0] != 'ok':
                break
        return res

    @staticmethod
    def crt(verb, extra=()):
        p = bytearray(512); p[:3] = b'CRT'; p[5:5 + len(verb)] = verb.encode()
        for o, v in extra: p[o] = v
        return bytes(p)

    @staticmethod
    def chunks(data):
        return [bytes(data[i:i + 512]).ljust(512, b'\0') for i in range(0, len(data), 512)]

    def bat(self, key, jpeg):
        assert len(jpeg) < 65536
        return [self.crt('BAT', [(0xA, len(jpeg) >> 8), (0xB, len(jpeg) & 255), (0xC, key)])] + self.chunks(jpeg)

    def dra(self, key, jpeg, x, y, w, h):
        n = len(jpeg)
        ex = [(9, n >> 16), (0xA, (n >> 8) & 255), (0xB, n & 255), (0xC, key),
              (0xD, w >> 8), (0xE, w & 255), (0xF, h >> 8), (0x10, h & 255),
              (0x11, x >> 8), (0x12, x & 255), (0x13, y >> 8), (0x14, y & 255)]
        return [self.crt('DRA', ex)] + self.chunks(jpeg)

    def mod(self, m): return [self.crt('MOD', [(0xA, m)])]
    def cle(self, key): return [self.crt('CLE', [(0xB, key)])]
    def stp(self): return [self.crt('STP')]

    def state(self):
        return dict(state=self.r8(ST + 1), key=self.r8(ST + 0xA), mode=self.r8(MODE), flag106=self.r8(0x20000106))


def make_jpeg(w, h, fn, quality=90, subsampling=0):
    """JPEG baseline 4:4:4 (como el que genera orcadeck). fn(x,y)->(r,g,b). Pillow."""
    from PIL import Image
    im = Image.new('RGB', (w, h))
    im.putdata([fn(x, y) for y in range(h) for x in range(w)])
    b = io.BytesIO(); im.save(b, 'JPEG', quality=quality, subsampling=subsampling, optimize=False, progressive=False)
    return b.getvalue()
