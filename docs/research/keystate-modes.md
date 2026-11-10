# AKP153E: ¿existe un modo con pulsar/soltar? (ingeniería inversa estática)

Fecha: 2026-11-10. Solo lectura: no se abrió el HID ni se tocó Stream Dock. Fuentes:

- `C:\Program Files (x86)\Stream Dock AJAZZ Global\SDLibrary1.dll` (DLL, `0x18…`) y `Stream Dock AJAZZ.exe`
  (EXE, `0x14…`), con capstone y los decompilados Ghidra previos (ver `hid-protocol.md`).
- **Firmware**: `%APPDATA%\HotSpot\StreamDock\update\AKP153E.bin` (96 496 B, 2024-02-20, Cortex-M Thumb,
  base `0x08004000`, cadena `V2.293S.00…`; el sufijo está comprimido, así que no se puede confirmar que sea
  exactamente `.003`). Direcciones `0x080…` = firmware.

Etiquetas: **OBSERVED** = leído en código; **INFERRED** = deducido.

## Conclusión

**Con el firmware V2.293S no hay ningún comando, modo ni secuencia de init que haga llegar un evento al
pulsar o un byte de estado 1.** Es la propia estructura del firmware, no un ajuste:

1. El bucle de teclado del firmware (`0x08006ef4`, llamado desde el bucle principal `0x080164fc`) detecta la
   tecla, escribe `TX[9]=código`, llama a `0x08007188(1,key)` (efecto local) y luego **espera activamente
   mientras la tecla siga pulsada** (`bl 0x8007108; cmp r0,#0; bne`, `0x08006f96`, `0x08007026` y
   `0x080070a8`). Solo después envía el informe de 0x200 bytes (`0x08009b10`). OBSERVED.
2. `TX[10]` (el byte de estado) **no se escribe nunca**. El buffer `0x2000093c` solo lo tocan `main`
   (`0x080163ea`: `ACK\0\0OK`), `send_ack` (`0x0800a018`, bytes 0..6) y el bucle de teclado (solo `[9]`).
   Por eso vale siempre 0. OBSERVED.
3. Lo que pide el usuario (pulsar/soltar, larga/corta) sí existe, pero en la **variante de firmware V3
   ("293SV3")**, que la app reconoce como otro modelo. No se activa desde el host (ver §3).

## 1. Verbos `CRT` que acepta el firmware (dispatcher `0x08006618`)

Exige `[0..2]="CRT"`; el verbo está en `[5..7]`. Cualquier otro valor se ignora, en silencio.

| Verbo | Efecto en el firmware | Relación con las teclas |
|---|---|---|
| `CLE` | borra la tecla `[0x0b]` | — |
| `STP` | la primera vez pone `flag[0x2000010c]=1`. **Las teclas solo se reportan con este flag a 1** | Por eso `watch` necesita init (OBSERVED `0x0800669c`, `0x08006f30`) |
| `BAT`, `LOG`, `DRA` | cabeceras de imagen | — |
| `HAN` / `DIS` | pantalla apagada/encendida (`+6`,`+7` de `0x2000010c`). Con la pantalla apagada, una tecla solo la enciende y no se reporta | OBSERVED `0x08006f44..0x08006fc2` |
| `MOD` | **solo guarda** `[0x0a]` en `0x20000050` (valor por defecto `'2'`=0x32, `0x08016446`) | Solo se lee en la ruta de imagen `DRA` (`0x08009950`: `=='1'` → ACK). **No afecta a las teclas** |
| `LIG` | brillo `[0x0a]` | — |
| `APP`+`NEW` (`[5..10]`) | `0x08004c4c` y luego `0x080093f8` (pone el bit 31 de un registro de sistema): **probable reinicio o entrada al bootloader**. NO ENVIAR | — |
| `0xFF 0xFF 0xFF` | solo traza | — |
| `[3]='A',[4]='d'` | escribe datos (`0x080086c8`), quizá serie o config | No investigado; no enviar |

`CONNECT`, `SET`, `QUCMD`, `VER`, `KEY`, `ICO`/`DE`, `ULEND` y `M_V` **no tienen handler** en este firmware.
Eso explica que no tuvieran efecto en las pruebas en vivo. OBSERVED.

## 2. Verbos que puede construir el host (DLL y EXE)

| Verbo / layout (payload) | Dónde | Modelos |
|---|---|---|
| `LIG [0a]=b`, `CLE [0b]=k / 0xFF / 'D','C'`, `STP [09]=pág`, `BAT`, `DIS/HAN [0a]=1`, `CONNECT`, `HAN` (sin CRT), `VER`, `LOG`, `DRA` | DLL, ver `hid-protocol.md` §2 | — |
| `MOD [0a]=modo` | DLL `0x180016930` (índices 0,1,2,5,6,7,0x0a verificados) | El EXE lo llama **solo** para 0108D/CH, CyberAxonD3, X106, DUO87, TS12P8, ZX-5539, 321D, H3, con modos **ASCII** `'1'/'2'/'3'` (`0x14011c7a5…`, `0x14011fdca`, `0x140248c97`). El EXE construye a mano otro MOD para N1 (`0x14011b255`, setting `actionLocation` 0xaa/0xbb/0xcc) |
| `SET [0a]=a [0b]=b` | DLL `0x18001d850` | EXE `0x140248e50`: "Direction" de 0108D/CyberAxonD3 |
| `QUCMD [0a..0e]` | DLL `0x180016b20`/`0x18001f130` | **El EXE no lo llama nunca** |
| `KEY …` (bajada/subida) | DLL `0x180015670`/`0x180015f80` | Emulación de teclado K1Pro |
| `ULEND`, `ICO`/`DE` (`sendSettingsPackHead`) | DLL `0x180019eb0`, `0x18001fe50` | Subida de logo y ajustes; `sendSettingsPackHead` no tiene llamadas desde el EXE |
| `sleep`, `GUI`, `fix`, `KEYNUM`, `LOGE3`/`LOGA`, `SETLB`/`LBLIG` | EXE (`0x140084118`, `0x1402002d2`, `0x14011ef31`, `0x1401ebdf6`, `0x1402186a7`, `0x14021cf9d`, `0x140144e43`, `0x140200ee9`) | K1Pro/K1ProUS (teclado, skins, barra de luz) y N1/N4 |
| `M_V`+página (4 B, sin CRT) | EXE `0x140217337` y otros | Vídeo/GIF de 0108D/X106/DUO87/321D/H3. El firmware del AKP153E lo descarta (no empieza por 'C') |

Ningún verbo se llama "key mode", "report", "press", "debounce" ni "long press". Los textos "LongPress" y
"Key Press To Key Release" del EXE/DLL pertenecen a la interfaz de acciones (macros), no al protocolo.

## 3. Quién recibe el estado 0/1: el EXE, no la DLL

La DLL (`readDataFromHidDevice` `0x18001c5d0`) pasa `[9],[10]` tal cual a `sigHandleKeyEvents` para todos
los modelos, excepto: `StreamDock-300` (otro formato: `[0]`, `[2]`, estado fijado a 1) y la terna
`StreamDock[293]`/`XF-A3501`/`StreamDock[295]` (antirrebote de 200 ms; INFERRED: ese firmware repite
mientras se mantiene, como el que documenta deckhand). La DLL **no ramifica por versión de firmware** en
las teclas (`V1./V2./V3.` solo se analiza para `StreamDock[293]`, y solo para mostrarlo).

El slot de teclas del EXE (`0x1400a1500`, key `[rbp+0x1140]`, state `[rbp+0x1148]`):

| Grupo (UUID) | Salto | Uso de `state` |
|---|---|---|
| **V3/V25**: `StreamDock[293SV3]`, `APK153_293SV3`, **`APK153E_293SV3`**, `APK153R_293SV3`, `XFA3503AV3Device`, `CN001V3Device`, `MSD-ONE293SV3`, `293V3`, `293V25`, `Stock293V3`, `SS550V3`, `M18V25(E)`, `M18V3`, `TS15B3`, `293V3E`, `VSD293V3`… | `0x1400a8ebb` → `0x1400a95b9` | **Sí**: `0x1400a96d4` `state==0` → soltar (`0x1400a99d9`), `state==1` → pulsar (`0x1400a9723`, gestiona rotate/backtoparent/openchild/page…), otro valor → acción directa |
| **293S/V2**: `StreamDock[293S]`, `A3503A`, `293S-R`, `KB8IN1-1`, `KB-2`, `MSD-ONE`, `GK150`, `AKP153`, **`AKP153E`**, `AKP153R`, `TS115`, `DK01`, `*_D15`… | `0x1400a8d69` | **No**: dispara siempre (`0x1400a8eb1`) |
| N3/AKP03, N4/AKP05, K1Pro… | otros (`0x1400a84e4`, `0x1400a6c65`, `0x1400a5b71`) | Sí (0/1, diales) |

**Qué decide el grupo**: el UUID sale de una tabla que asocia la *cadena de producto HID* con el UUID
(`0x1400f7b50`). La cadena la fija el firmware en el descriptor USB, así que el host no puede cambiarla:

| Product string | UUID | Evidencia |
|---|---|---|
| `"AJAZZ AKP153E"` | `AKP153E` (el nuestro, `DeviceUUID=AKP153E`) | `0x1400f860b`/`0x1400f8624` |
| `"AJAZZ AKP153E "` (con espacio final) | `APK153E_293SV3` | `0x1400f87cd`/`0x1400f87e6`; nombre visible en `0x1400fccb5` |

La variante V3 también abre con otro filtro de interfaz (usagePage `0xFFA0`, lista en `0x14023b8a0…`).
INFERRED: el AKP153E con firmware "293SV3" envía pulsar(1) y soltar(0), y por eso el EXE distingue el
estado solo para ese UUID.

## 4. Datos del coordinador (deckhand, companion-bridge)

- **deckhand** (`V2.293.00.001`, 293S de Mirabox `0x5548:0x6674`): repite el informe mientras la tecla
  está pulsada. Es **otro firmware**. El nuestro espera a que se suelte y envía un único informe (§Conclusión
  punto 1). Encaja con el antirrebote de 200 ms que la DLL aplica solo a los UUID `StreamDock[293]`/`[295]`/
  `XF-A3501` (INFERRED).
- **`M_V`+página**: en la app es un paquete de 4 bytes sin `CRT`, usado solo en widgets de vídeo/GIF de
  0108D/X106/DUO87/321D/H3. El dispatcher del AKP153E exige `CRT`, así que lo ignora (OBSERVED). No hay
  ningún init distinto por versión para el AKP153E.
- **`LEGACY_INIT`** de companion-bridge (`00 01 01 00…`, `00 01 02 64 00…`): ni la DLL ni el EXE lo
  generan, y el firmware lo descarta porque `[0]!='C'` (OBSERVED). Sobra para el AKP153E.

## 5. Candidatos (ninguno probado en el dispositivo)

| # | Candidato | Confianza |
|---|---|---|
| — | Comando host→dispositivo que active pulsar/soltar en V2.293S | **No existe** (OBSERVED: firmware §Conclusión y §1) |
| 1 | Firmware "293SV3" para AKP153E (cadena de producto con espacio final → UUID `APK153E_293SV3`) | Existencia INFERRED por la tabla del EXE; no se encontró binario V3 en local. Actualizar el firmware es arriesgado (posible brick) y queda fuera de este análisis |
| 2 | `CRT MOD` con ASCII `'1'..'3'` (las pruebas usaron 0..4 binario) | Descartado: el firmware solo guarda el byte y no lo usa en las teclas (OBSERVED `0x0800685e`, `0x08009950`) |

En software, lo único posible es tratar el informe como "clic al soltar". La duración de la pulsación no
puede medirse: el firmware no envía nada al pulsar.

Scripts y salidas (scratch, sin commit): `…\scratchpad\{fx,lay,xr,calls,iatx,fw,fwd,lit,gref,blx}.py`.
