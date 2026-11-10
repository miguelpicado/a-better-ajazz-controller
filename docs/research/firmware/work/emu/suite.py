# Bateria de escenarios para original / parche(s). Uso: python -I suite.py <orig.bin> <parche.bin> [...]
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.argv = [sys.argv[0]] + sys.argv[1:]
from emu import Emu, summarize, KEYPOS

imgs = [(p.split('/')[-1], open(p, 'rb').read()) for p in sys.argv[1:]]

def sc(name, keys, t_end, **kw):
    return (name, keys, t_end, kw)

S = []
for k in range(1, 16):
    S.append(sc(f'tecla {k} 100ms', [(k, 10, 110)], 200))
S += [
    sc('tap 3ms (< antirrebote)', [(7, 10, 13)], 100),
    sc('tap 12ms', [(7, 10, 22)], 100),
    sc('tap 25ms', [(7, 10, 35)], 100),
    sc('rafaga 5x tecla 1 (30ms on / 30ms off)', [(1, 10 + 60 * i, 40 + 60 * i) for i in range(5)], 400),
    sc('2 teclas a la vez, misma fila (2 y 5)', [(2, 10, 110), (5, 10, 110)], 200),
    sc('2 teclas a la vez, filas distintas (1 y 2)', [(1, 10, 110), (2, 10, 110)], 200),
    sc('solapadas misma fila: 2 [10,200) 5 [60,150)', [(2, 10, 200), (5, 60, 150)], 300),
    sc('solapadas filas distintas: 3 [10,200) 4 [60,150)', [(3, 10, 200), (4, 60, 150)], 300),
    sc('mantener 60 s (tecla 9)', [(9, 10, 60010)], 60100),
    sc('host lento 5ms, tap 12ms', [(7, 10, 22)], 100, host_latency=5.0),
    sc('host lento 1ms, tap 12ms', [(7, 10, 22)], 100, host_latency=1.0),
    sc('host lento 20ms, tap 12ms (supera la espera)', [(7, 10, 22)], 100, host_latency=20.0),
    sc('host no lee (latencia inf)', [(7, 10, 110), (8, 200, 300)], 400, host_latency=1e9),
    sc('pantalla apagada (flag 0x20000112=1)', [(4, 10, 110)], 200, screen_off=1),
    sc('sin STP (flag 0x2000010C=0)', [(4, 10, 110)], 200, stp=0),
]

def run(img, keys, t_end, kw):
    e = Emu(img, keys, **kw)
    e.probe_at = [t0 + (t1 - t0) / 2 for (_, t0, t1) in keys]; e.probed = set()
    e.run(t_end)
    return e

def fmt(rep):
    return ' '.join(f"[{r['key']},{r['state']}@{r['t']:.0f}]" + ('' if r['hdr'] == '41434b00004f4b0000' and r['rest_zero'] and r['len'] == 512 else '!BAD') for r in rep)

anyerr = False
for name, keys, t_end, kw in S:
    print(f'## {name}')
    for iname, img in imgs:
        e = run(img, keys, t_end, kw)
        rep = summarize(e)
        drops = [a for a in e.attempts if a['busy']]
        lines = [(ev[2], ev[3]) for ev in e.events if ev[0] == 'line']
        scr = [ev for ev in e.events if ev[0] == 'screen']
        err = e.unmapped + e.regfail
        anyerr |= bool(err)
        print(f'  {iname[:34]:34s} informes: {fmt(rep) or "-"}  | descartados(EP ocupado): {[(a["k"], a["s"]) for a in drops] or "-"}'
              f'  | linea: {lines}  | pantalla: {len(scr)}  | wdt: {e.wdt}  | ISR-ACK veria [9],[10]: {e.isr_probe}'
              f'  | espera: {e.spins}  | errores: {err or "ninguno"}')
print('ERRORES' if anyerr else 'sin excepciones / accesos fuera de mapa / fallos de registros')
