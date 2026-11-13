# Ejecuta todas las validaciones de emu2 desde el repo (venv con unicorn + pillow). Exit 0 = todo OK.
import subprocess, sys, os
H = os.path.dirname(os.path.abspath(__file__)); W = os.path.join(H, '..')
def run(*a):
    print('>>', ' '.join(os.path.basename(x) for x in a), flush=True)
    return subprocess.call([sys.executable, '-I', *a], cwd=H)
rc = 0
rc |= run(os.path.join(H, 'make_lcd_patch.py'), os.path.join(W, '..', 'backup', 'AKP153E.bin'), os.path.join(W, 'AKP153E_sidestrip_NO_PROBADO.bin'))
rc |= run(os.path.join(W, 'make_patch_v3.py'), os.path.join(W, '..', 'backup', 'AKP153E.bin'), os.path.join(W, 'AKP153E_pressrelease_v3_NO_PROBADO.bin'))
for s in ('pr_suite.py', 'scenarios_a.py', 'heap_probe.py', 'bat_evidence.py'): rc |= run(os.path.join(H, s))
print('EXIT', 1 if rc else 0); sys.exit(1 if rc else 0)
