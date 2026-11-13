# Modelo MINIMO de un controlador RA8876 (INFERRED, ver docs/research/hid/lcd-init-rea.md §0). SOLO FICHEROS.
# Modela lo que el firmware AKP153E usa: registros 0x20..0x29 (imagen principal), 0x50..0x5D (lienzo y ventana
# activa), 0x5F..0x62 (posicion de escritura), puerto de datos 0x04, relleno de rectangulo (0x68..0x6F, 0x76, 0xD2..0xD4)
# y BTE 0x90..0xB4 (solo copia con ROP S0, 24 bpp). Memoria de video: PAGE = 480*854*3 por pagina.
import random

W, H = 480, 854
BPP = 3
PAGE = W * H * BPP            # 0x12C3C0
NPAGES = 4                    # paginas 0, 1 (0x12C3C0), 2 (0x258780) y una de reserva


def u16(r, a): return r[a] | (r[a + 1] << 8)
def u32(r, a): return r[a] | (r[a + 1] << 8) | (r[a + 2] << 16) | (r[a + 3] << 24)


class RA8876:
    def __init__(self, seed=1, init='pattern', busy_scale=1):
        self.regs = bytearray(256)
        self.idx = 0
        rng = random.Random(seed)
        self.mem = bytearray(PAGE * NPAGES)
        if init == 'pattern':                      # contenido previo reconocible por pagina
            for p, v in enumerate((0x10, 0x20, 0x30, 0x40)):
                self.mem[p * PAGE:(p + 1) * PAGE] = bytes([v]) * PAGE
        elif init == 'noise':
            self.mem[:] = rng.randbytes(len(self.mem))
        self.cx = self.cy = 0                      # cursor de escritura (0x5F..0x62)
        self.busy = 0                              # lecturas de estado restantes con "core busy"
        self.busy_scale = busy_scale
        self.t_ms = 0.0
        self.log = []                              # eventos (str)
        self.oob = []                              # accesos fuera de memoria
        self.unmodeled = []                        # cosas que el modelo no sabe hacer
        self.nwrites = 0
        self.n_bte = self.n_fill = 0
        self.bte_log = []; self.fill_log = []; self.pix_log = []   # (cursor, nbytes) por cada transferencia de datos

    # ---- helpers de registros
    def canvas(self): return u32(self.regs, 0x50)
    def cw(self): return u16(self.regs, 0x54)
    def misa(self): return u32(self.regs, 0x20)
    def miw(self): return u16(self.regs, 0x24)
    def aw(self): return (u16(self.regs, 0x56), u16(self.regs, 0x58), u16(self.regs, 0x5A), u16(self.regs, 0x5C))

    # ---- interfaz del bus (lo que ve el firmware)
    def write_index(self, i): self.idx = i & 0xFF

    def write_data(self, v):
        i = self.idx; v &= 0xFF
        self.regs[i] = v
        self.nwrites += 1
        if 0x5F <= i <= 0x62:
            self.cx = u16(self.regs, 0x5F); self.cy = u16(self.regs, 0x61)
        elif i == 0x90 and v & 0x10:
            self._bte()
        elif i == 0x76 and v & 0x80:
            self._fill(v)
        elif i == 0xB6 and v & 1:
            self.unmodeled.append('DMA (0xB6 bit0) start: no modelado, se da por completado')
            self.regs[0xB6] &= ~1

    def read_data(self):
        i = self.idx
        v = self.regs[i]
        return v

    def status(self):
        s = 0x04                                    # bit2 = SDRAM listo
        if self.busy > 0:
            self.busy -= 1; s |= 0x08               # bit3 = core ocupado
        return s

    # ---- memoria
    def _addr_ok(self, a, n):
        if a < 0 or a + n > len(self.mem):
            self.oob.append((a, n)); return False
        return True

    def pixel_write(self, data):
        """Puerto de datos 0x04 en modo grafico: cada pixel = 3 bytes (BGR tal cual), cursor dentro de la ventana activa."""
        base, cw = self.canvas(), self.cw()
        ax, ay, aw, ah = self.aw()
        self.pix_log.append(((self.cx, self.cy), len(data), base, (ax, ay, aw, ah)))
        for o in range(0, len(data) - len(data) % 3, 3):
            a = base + (self.cy * cw + self.cx) * 3
            if self._addr_ok(a, 3):
                self.mem[a:a + 3] = data[o:o + 3]
            self.cx += 1
            if self.cx >= ax + aw:
                self.cx = ax; self.cy += 1
                if self.cy >= ay + ah:
                    self.cy = ay

    def _depth_ok(self):
        r = self.regs
        ok = ((r[0x92] >> 5) & 7) in (2, 3) and ((r[0x92] >> 2) & 7) in (2, 3) and (r[0x92] & 3) == 2
        return ok

    def _bte(self):
        r = self.regs
        op, rop = r[0x91] & 0xF, r[0x91] >> 4
        s0, s0w, s0x, s0y = u32(r, 0x93), u16(r, 0x97), u16(r, 0x99), u16(r, 0x9B)
        ds, dw, dx, dy = u32(r, 0xA7), u16(r, 0xAB), u16(r, 0xAD), u16(r, 0xAF)
        w, h = u16(r, 0xB1), u16(r, 0xB3)
        self.n_bte += 1
        self.bte_log.append(dict(op=op, rop=rop, s0=s0, s0w=s0w, s0xy=(s0x, s0y), dst=ds, dw=dw, dxy=(dx, dy), wh=(w, h)))
        self.regs[0x90] &= ~0x10
        if op != 2 or rop != 0xC:
            self.unmodeled.append('BTE op=%#x rop=%#x no modelado' % (op, rop)); return
        if w == 0 or h == 0:
            self.unmodeled.append('BTE con ancho/alto 0')
        for row in range(h):
            sa = s0 + ((s0y + row) * s0w + s0x) * 3
            da = ds + ((dy + row) * dw + dx) * 3
            n = w * 3
            if self._addr_ok(sa, n) and self._addr_ok(da, n):
                self.mem[da:da + n] = self.mem[sa:sa + n]
        self.busy = max(self.busy, (w * h * 3) // 4096 * self.busy_scale + 1)

    def _fill(self, v):
        r = self.regs
        x0, y0, x1, y1 = u16(r, 0x68), u16(r, 0x6A), u16(r, 0x6C), u16(r, 0x6E)
        col = bytes([r[0xD4], r[0xD3], r[0xD2]])     # B,G,R en memoria (el firmware escribe R en 0xD2)
        base, cw = self.canvas(), self.cw()
        self.n_fill += 1
        self.fill_log.append(dict(v=v, rect=(x0, y0, x1, y1), canvas=base, color=col.hex()))
        if v != 0xE0:
            self.unmodeled.append('DCR0 %#x no modelado' % v)
        ax, ay, aw, ah = self.aw()                # el dibujo geometrico se recorta a la ventana activa (INFERRED)
        x0c, x1c = max(x0, ax), min(x1, ax + aw - 1)
        y0c, y1c = max(y0, ay), min(y1, ay + ah - 1)
        for y in range(y0c, y1c + 1):             # extremo inclusivo (convencion RA8876)
            a = base + (y * cw + x0c) * 3
            n = (x1c - x0c + 1) * 3
            if n > 0 and self._addr_ok(a, n):
                self.mem[a:a + n] = col * (x1c - x0c + 1)
        self.regs[0x76] &= ~0x80
        self.busy = max(self.busy, ((x1 - x0 + 1) * (y1 - y0 + 1) * 3) // 4096 * self.busy_scale + 1)

    # ---- vistas
    def page(self, p): return bytes(self.mem[p * PAGE:(p + 1) * PAGE])

    def display(self):
        """Lo que se ve: imagen principal desde MISA, ancho MIW, ventana principal (x,y) = 0 asumida."""
        a, w = self.misa(), self.miw() or W
        if not self._addr_ok(a, PAGE):
            return None
        return bytes(self.mem[a:a + PAGE]) if w == W else None


def px(buf, x, y):
    o = (y * W + x) * 3
    return tuple(buf[o:o + 3])


def diff_bbox(a, b):
    """Rectangulo (x0,y0,x1,y1 inclusivo) de pixeles distintos entre dos paginas, o None."""
    x0 = y0 = 10 ** 9; x1 = y1 = -1
    rowb = W * 3
    for y in range(H):
        ra, rb = a[y * rowb:(y + 1) * rowb], b[y * rowb:(y + 1) * rowb]
        if ra == rb:
            continue
        v = int.from_bytes(ra, 'little') ^ int.from_bytes(rb, 'little')
        lo = ((v & -v).bit_length() - 1) // 8 // 3
        hi = (v.bit_length() - 1) // 8 // 3
        x0, x1 = min(x0, lo), max(x1, hi); y0 = min(y0, y); y1 = max(y1, y)
    return None if x1 < 0 else (x0, y0, x1, y1)


def count_color(buf, rgb_bgr, rect=None):
    """Numero de pixeles de color BGR dado (opcionalmente dentro de rect)."""
    n = 0
    x0, y0, x1, y1 = rect if rect else (0, 0, W - 1, H - 1)
    for y in range(y0, y1 + 1):
        row = buf[(y * W + x0) * 3:(y * W + x1 + 1) * 3]
        for i in range(0, len(row), 3):
            if tuple(row[i:i + 3]) == rgb_bgr:
                n += 1
    return n
