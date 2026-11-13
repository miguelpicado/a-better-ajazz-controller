# Disparo del bootloader EEEF:2929 (REA estático, 2026-11-13)

Solo lectura, sin tocar HID/USB. Informe **recortado** a petición del coordinador: la pregunta 4 se resolvió
con hardware (`updatetool-origin.md` §11: `bootwatch` vio `EEEF:2929` a los 519 ms y la herramienta flasheó el original).
Entradas: `AKP153E.bin` SHA-256 `15e03f5b…54f6f`; `FirmwareUpgradeTool.exe` `f02eb5b7…ed606`.
Scripts (scratchpad, capstone 5): `dis.py` (Thumb), `scan.py`/`xr.py` (literales y xrefs), `xd.py` (x64).
Bootloader `0x08000000–0x08003FFF` **no volcado**: nada de él se afirma aquí como hecho.

## 1. Paquete que exige el firmware (OBSERVED)
Parser CRT `0x08006618` (único llamador `0x08009856`, solo si se recibieron **exactamente 512 B** y el byte de
estado `[0x20000xxx+1]==0`). Buffer RX `0x2000073C`. Para APP/NEW, offsets comparados:

| off | valor | | off | valor |
|---|---|---|---|---|
| 0..2 | `C R T` | | 7 | `P` |
| 3,4 | **no comparados** (0 en la herramienta) | | 8 | `N` |
| 5 | `A` | | 9 | `E` |
| 6 | `P` | | 10 | `W` |

Resto ignorado. Cadena de decisión: `0x08006858` (`A`) → `0x08006870` (`P`,`P`) → `0x0800687E` (`N`,`E`,`W`).
Acción: `0x08007C34` (pantalla), `0x08004C4C` (flash), `0x080093F8` (reset: bit 31 de `0x40002000`).

`0x08004C4C`: lee 10 palabras de `0x0807E000`, borra el sector, escribe `[0]`, `~[1]`, `[2]`, `~[3]` y
`[0x0807E010]=0x21436587` (literal `0x08004CD0`). **No restaura las palabras 5..9.** Los retornos de borrado
(`0x080058FC`) y escritura (`0x0800592C`) **se ignoran**: un fallo de flash pasaría en silencio (INFERRED riesgo).
Imprime `APP_Updata_Flag = %2x`.

Escritores/lectores (búsqueda de palabras, literales pc-relativos y movw/movt; sin alias dinámicos):
- `0x0807E000..EFFF`: solo `0x08004C4C` (4 literales `0x08004CCC`). `0x21436587`: solo `0x08004CD0`.
  `0x08004C4C` solo se llama desde `0x080068A0`. **La app nunca lee ni borra el aviso** → lo consume el bootloader (INFERRED).
- Único reset: `0x080093F8` (`0x40002000`), solo desde `0x080068A8`. No hay otro verbo CRT ni petición vendor que lo active
  (UNKNOWN: ROM `0x04005xxx`, watchdog).

## 2. Arranque de la app (OBSERVED)
- `main` (~`0x080163E8`) → `0x08005378`: lee la palabra `0x08003800` (bootloader), la compara con un valor derivado
  de 8 B leídos por `0x0800E768` (ID del chip); si **no coinciden** entra en un bucle infinito dibujando (`0x08005400..26`).
  No toca el aviso de actualización.
- Bucle principal `0x080164EA`: solo flags (`0x08006EF4`, pantalla). **No se vio ninguna comprobación de tecla/pin al arrancar**
  que fuerce el bootloader (EPORT `0x08005F14` solo genera eventos; inits no revisados línea a línea → UNKNOWN parcial).

## 3. Herramienta (OBSERVED)
`0x140013130(this,1)`: (a) `hid_open(0xEEEF,0x2929)`; si falla (b) `0x14000D9D0`: `hid_open(VID,PID)` decimal de argv,
escribe **0x201 B** (`00`, `CRT\0\0APPNEW`, resto 0), **ignora el retorno**; (c) `hid_close` + `QThread::msleep(1000)`;
(d) bucle `hid_open(EEEF,2929)` hasta 3001 veces con `msleep(1)`+`processEvents`; si agota, aviso
"failed to get device! … unplugging and plugging". Cada `hid_open` enumera el sistema, así que el tiempo real no es 3 s exactos (INFERRED).
No hay ramas por VID/PID ni por versión. La app 2.10.185 solo comprobaba `findDevice(EEEF,2929)` antes de lanzarla (según `updatetool-origin.md` §4;
su binario ya no está en el disco, no re-verificado). Nuestro intento **sí** replicó el paquete exacto.

## 4. Hipótesis (resuelta por hardware, §11 de updatetool-origin)
| # | Hipótesis | Estado |
|---|---|---|
| 1 | Bootloader presente, ventana de enumeración corta (hidwatch 100 ms la perdió; la herramienta falló por temporización/enumeración) | **Respaldada** por el hardware (519 ms) |
| 2-4 | Otro VID/PID, flag borrado, bootloader ausente | Descartadas en esta unidad |

## Abiertos
Bootloader sin volcar (qué verifica, tiempo de espera, si borra el aviso); por qué la herramienta no lo atrapó (hid_open lento o ventana corta, no medido);
ROM `0x04005xxx` y watchdog como otros resets; inits de arranque sin revisar completos.
