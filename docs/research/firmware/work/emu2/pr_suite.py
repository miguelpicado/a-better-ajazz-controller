# Validacion del parche pulsar/soltar (v2, v3) contra el original con el emulador del bucle de teclas (../emu/emu.py).
# Uso: python -I pr_suite.py   (desde el repo; requiere unicorn). Exit 0 = todos los invariantes cumplidos.
import sys, os, itertools, hashlib
HERE = os.path.dirname(os.path.abspath(__file__))
W = os.path.join(HERE, '..')
sys.path.insert(0, os.path.join(W, 'emu'))
from emu import Emu, summarize
from unicorn import UC_HOOK_CODE

IM = {'orig': os.path.join(W, '..', 'backup', 'AKP153E.bin'),
      'v2': os.path.join(W, 'AKP153E_pressrelease_v2_NO_PROBADO.bin'),
      'v3': os.path.join(W, 'AKP153E_pressrelease_v3_NO_PROBADO.bin')}
IMG = {k: open(p, 'rb').read() for k, p in IM.items()}
fails = []

def run(name, sc, t_end, **kw):
    e = Emu(IMG[name], sc, **kw)
    if name == 'v3':      # emu.py solo conoce el bucle de espera de v2 (0x0801ABBE); en v3 esta en 0x0801ABC4
        e.spins3 = 0
        def on_spin3(u, a, sz, _): e.spins += 1; e.advance(0.0001)
        e.u.hook_add(UC_HOOK_CODE, on_spin3, begin=0x0801ABC4, end=0x0801ABC4)
    e.probe_at = [(a + b) / 2 for (_, a, b) in sc]; e.probed = set()
    e.run(t_end)
    rep = summarize(e)
    bad = bool(e.unmapped or e.regfail) or any(r['hdr'] != '41434b00004f4b0000' or not r['rest_zero'] or r['len'] != 512 for r in rep)
    return e, [(r['key'], r['state'], r['t']) for r in rep], bad

def check(cond, msg):
    if not cond: fails.append(msg)
    print(('OK   ' if cond else 'FAIL ') + msg)

# 1) las 15 teclas, pulsar 100 ms: v2/v3 = [k,1][k,0]; original = [k,0]; sin errores; ISR-ACK durante la pulsacion
for k in range(1, 16):
    for n in ('v2', 'v3'):
        e, rep, bad = run(n, [(k, 10, 110)], 200)
        ok = [(a, b) for a, b, _ in rep] == [(k, 1), (k, 0)] and not bad
        isr = [(a, b) for (_, a, b) in e.isr_probe]
        if n == 'v3': ok &= all(b == 0 for _, b in isr)
        check(ok, f'{n} tecla {k}: informes {[(a,b) for a,b,_ in rep]} isr[9],[10]={isr}')
    e, rep, bad = run('orig', [(k, 10, 110)], 200)
    check([(a, b) for a, b, _ in rep] == [(k, 0)] and not bad, f'orig tecla {k}: {[(a,b) for a,b,_ in rep]}')

# 2) rebote (chatter) en pulsar y soltar: no hay antirrebote en el original; v2/v3 emiten pares coherentes
bounce = [(7, 10, 11), (7, 12, 13), (7, 14, 60), (7, 61, 62), (7, 63, 64)]
for n in ('orig', 'v2', 'v3'):
    e, rep, bad = run(n, bounce, 200)
    st = [(a, b) for a, b, _ in rep]
    print(f'     {n} rebote: {st}')
    if n != 'orig':
        # nunca dos "pulsar" seguidos sin "soltar", y termina soltado
        alt = all(st[i][1] != st[i + 1][1] for i in range(len(st) - 1)) and (not st or st[-1][1] == 0)
        check(alt and not bad, f'{n} rebote: alternancia pulsar/soltar y estado final soltado')

# 3) todas las parejas de teclas (acordes), 105 combinaciones
for n in ('v2', 'v3'):
    nrep = 0; nbad = 0; unpaired = 0
    for a, b in itertools.combinations(range(1, 16), 2):
        e, rep, bad = run(n, [(a, 10, 110), (b, 10, 110)], 160)
        st = [(k, s) for k, s, _ in rep]
        nbad += bad
        # cada pulsar debe tener su soltar con la misma tecla, en orden
        pend = {}
        for k, s in st:
            if s == 1: pend[k] = pend.get(k, 0) + 1
            else: pend[k] = pend.get(k, 0) - 1
        unpaired += sum(1 for v in pend.values() if v > 0)
        nrep += 1
    check(nbad == 0 and unpaired == 0, f'{n} acordes {nrep} parejas: errores={nbad} pulsar-sin-soltar={unpaired}')

# 4) EP ocupado: host lento (latencias en ms)
for lat in (0, 1, 3, 5, 6, 8, 20):
    for n in ('v2', 'v3'):
        e, rep, bad = run(n, [(7, 10, 22)], 100, host_latency=float(lat))
        st = [(a, b) for a, b, _ in rep]
        lost = [x for x in e.attempts if x['busy']]
        print(f'     {n} host {lat} ms, tap 12 ms: {st} descartados={[(x["k"], x["s"]) for x in lost]}')
        if lat <= 5: check(st == [(7, 1), (7, 0)] and not bad, f'{n} host {lat} ms: pulsar+soltar entregados')

# 5) pulsacion larga: 60 s, y sin reportes extra
for n in ('v2', 'v3'):
    e, rep, bad = run(n, [(9, 10, 60010)], 60100)
    check([(a, b) for a, b, _ in rep] == [(9, 1), (9, 0)] and not bad, f'{n} mantener 60 s: {[(a,b,t) for a,b,t in rep]} wdt={e.wdt}')

for n in IM: print(n, hashlib.sha256(IMG[n]).hexdigest(), len(IMG[n]))
print('FALLOS:' if fails else 'TODO OK', fails)
sys.exit(1 if fails else 0)
