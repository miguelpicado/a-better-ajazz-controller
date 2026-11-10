// SOLO OBSERVACION. Frida attach a "Stream Dock AJAZZ.exe" (no spawn, no escribe nada).
// Registra: lanzamiento de procesos, QProcess::start, URLs Qt (QUrl), ficheros abiertos con
// rutas de actualizacion y las primeras 16 B de cada WriteFile/HidD_SetFeature (para ver si
// aparece CRT..APP..NEW). No modifica argumentos ni valores de retorno.
'use strict';

function qstr(p) {               // Qt5 QString* -> JS string
  try {
    const d = p.readPointer();
    const size = d.add(4).readS32();
    const off = d.add(16).readS64();
    return d.add(off).readUtf16String(size);
  } catch (e) { return '<qstr?>'; }
}
function hex16(p, n) {
  try { return Array.from(new Uint8Array(p.readByteArray(Math.min(n, 16)))).map(b => ('0' + b.toString(16)).slice(-2)).join(' '); }
  catch (e) { return '?'; }
}
const RX = /updat|upgrad|firmware|\.bin|\.ud|key123|UpDateTool/i;
const handles = {};

const k32 = Process.getModuleByName('kernel32.dll');
Interceptor.attach(k32.getExportByName('CreateProcessW'), {
  onEnter(a) { send({ ev: 'CreateProcessW', app: a[0].isNull() ? null : a[0].readUtf16String(), cmd: a[1].isNull() ? null : a[1].readUtf16String(), cwd: a[7].isNull() ? null : a[7].readUtf16String() }); }
});
Interceptor.attach(k32.getExportByName('CreateFileW'), {
  onEnter(a) { this.p = a[0].isNull() ? '' : a[0].readUtf16String(); },
  onLeave(r) {
    if (RX.test(this.p)) send({ ev: 'CreateFileW', path: this.p, h: r.toString() });
    if (/\\\\\?\\hid#/i.test(this.p)) handles[r.toString()] = this.p;
  }
});
Interceptor.attach(k32.getExportByName('WriteFile'), {
  onEnter(a) {
    const h = a[0].toString();
    if (handles[h]) {
      const head = hex16(a[1], a[2].toInt32());
      if (/^00 43 52 54/.test(head)) send({ ev: 'HID WriteFile CRT', head, len: a[2].toInt32() });   // 'CRT'
    }
  }
});
try {
  const hid = Process.getModuleByName('hid.dll');
  Interceptor.attach(hid.getExportByName('HidD_SetFeature'), { onEnter(a) { send({ ev: 'HidD_SetFeature', head: hex16(a[1], a[2].toInt32()) }); } });
} catch (e) { send({ ev: 'info', msg: 'hid.dll no cargada aun' }); }

const qc = Process.getModuleByName('Qt5Core.dll');
for (const e of qc.enumerateExports()) {
  if (e.name.startsWith('?start@QProcess@@QEAAXAEBVQString@@AEBVQStringList@@')) {
    Interceptor.attach(e.address, { onEnter(a) { send({ ev: 'QProcess::start', program: qstr(a[1]) }); } });
  }
  if (e.name.startsWith('?setWorkingDirectory@QProcess@@')) {
    Interceptor.attach(e.address, { onEnter(a) { send({ ev: 'QProcess::setWorkingDirectory', dir: qstr(a[1]) }); } });
  }
  if (e.name === '??0QUrl@@QEAA@AEBVQString@@W4ParsingMode@0@@Z') {
    Interceptor.attach(e.address, { onEnter(a) { const u = qstr(a[1]); if (RX.test(u)) send({ ev: 'QUrl', url: u }); } });
  }
}
// Mensajes de la app (QMessageBox::information) -> clave de texto mostrada
try {
  const qw = Process.getModuleByName('Qt5Widgets.dll');
  for (const e of qw.enumerateExports()) {
    if (e.name.startsWith('?information@QMessageBox@@SA?AW4StandardButton@1@PEAVQWidget@@AEBVQString@@1')) {
      Interceptor.attach(e.address, { onEnter(a) { send({ ev: 'QMessageBox::information', title: qstr(a[1]), text: qstr(a[2]) }); } });
    }
  }
} catch (e) { }
send({ ev: 'ready' });
