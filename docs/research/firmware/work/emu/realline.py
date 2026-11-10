import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from emu import Emu, summarize
for p in sys.argv[1:]:
    e = Emu(open(p,'rb').read(), [(5, 10, 110)], line_mode='real')
    e.run(200, budget=20_000_000)
    print(p.split('/')[-1], summarize(e), e.unmapped[:3], e.regfail[:3], 'calls', e.calls)
    print('  periph distintos', len(e.periph), sorted(hex(a) for (a,m) in e.periph if not (0x40012000<=a<0x4001e000 or 0x4004c000<=a<0x4004c100 or a==0x40005004))[:30])
