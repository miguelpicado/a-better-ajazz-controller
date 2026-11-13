# AKP153E: logo de arranque, secuencias de registros del LCD y panel lateral continuo (REA estático, 2026-11-13)

Firmware: `docs/research/firmware/backup/AKP153E.bin` (`V2.293S.00.003`, base `0x08004000`, Cortex-M4), desensamblado
con capstone 5 (Python 3.13). App: `SDLibrary1.dll` (sha256 `b8be3108…9af19`, x86-64) y `Stream Dock AJAZZ.exe`, con
capstone/pefile (el REA no tiene proveedor de decompilación: Ghidra no configurado). **Nada de esto toca el
dispositivo**: no se abrió el HID, no se ejecutó `orcadeck` ni se escribió en `%APPDATA%`.
**OBSERVED** = leído en el desensamblado o en un fichero local. **INFERRED** = deducción. **UNKNOWN** = no resuelto.

Scripts (todos en el scratchpad, corren limpios; no están en el repo):
`C:\Users\migue\AppData\Local\Temp\claude\2c80c354be5b99aa\09cb1bdf-317d-4661-a18d-fe850b88a054\scratchpad\`
`fwdis.py` (desensamblador Thumb-2 con literales), `callers.py` (llamadores bl/b.w), `regseq.py` y `allregs.py`
(secuencias índice/valor hacia `0x08007878`/`0x080076C4`), `litref.py` (quién carga un literal de RAM),
`x64dis.py`, `x64callers.py`, `xrefstr.py`, `strs.py` (lado app).

## 0. Resumen del modelo del LCD (lo que explica todo lo demás)

- **INFERRED (alta confianza):** el controlador es un **RA8876** (o clon). Protocolo SPI `0x00`=índice, `0x80`=escritura
  de dato, `0xC0`=lectura de dato, `0x40`=estado (`0x0800899C`, `0x08008A1A`, `0x080089F6`, `0x0800788A`); y los
  registros coinciden uno a uno con el RA8876: `0x20..0x29` imagen principal, `0x50..0x5D` lienzo y ventana activa,
  `0x5F..0x62` posición de escritura, `0x04` puerto de datos (`0x08007D5A`), `0x68..0x6F` + `0x76` geometría,
  `0x90..0xB4` BTE, `0xB6..0xC9` DMA, `0xD2..0xD4` color. Bus: SPI del MCU en `0x40011000` (`0x08008930` envía un byte
  y lee la respuesta; `0x08008870` prepara DMA del MCU hacia ese SPI, solo la llama el escritor de filas `0x08007D6C`).
- **OBSERVED:** el firmware usa tres «páginas» de memoria de vídeo de 480×854×3 = `0x12C3C0` B:
  - página 0 = `0x000000` (**frontal**, la que se muestra en operación normal);
  - página 1 = `0x12C3C0` (**trasera/de trabajo**; `BAT` y `CLE` pintan aquí; también es donde se carga el logo);
  - página 2 = `0x258780` (la usa `DRA` estado 4). El `0x258780` que el informe anterior llamaba «longitud» es la
    **dirección de inicio del lienzo** (regs `0x50..0x53`), 2 fotogramas por encima del origen.
  - zona de paso del DMA de flash: `0xE12D00` (INFERRED: SDRAM ≥ 16 MB).
- **OBSERVED:** `STP` = `0x080163AC`: lienzo ← 0 y copia BTE de `0x12C3C0` a `0` (480×854). Es el «flip» trasera→frontal.
  Fin de cada `CLE` (`0x08007684..0x080076AA`): igual (lienzo ← 0 y BTE trasera→frontal).
- **OBSERVED:** al arrancar `main` (`0x08016400..0x0801651A`) pone MISA=0, lienzo=0, rellena toda la página 0 y luego
  llama `0x080076D0(0x20000)` (logo). Bucle principal: si el byte `0x2000010F`==1 vuelve a mostrar el logo
  (`0x08016506`).
- **OBSERVED (corrección al doc anterior):** el byte de clave de `LOG` es `[0x0C]` del paquete (la app lo pone en el
  offset 0x0D del informe, con el byte de report-id delante); `[8..0xB]` es la longitud BE32 (`0x08006720..0x0800672C`
  junto con `sendLogoSizeCommand` `0x18001EA40`). `side-panel-lcd.md` dice `[0xB]`.
- **OBSERVED:** el color del buffer se invierte al escribir: `0x08007D7A..0x08007DA0` copia cada píxel como
  `dst[0]=src[2], dst[1]=src[1], dst[2]=src[0]` (RGB de libjpeg → BGR en memoria de vídeo).

## 1. Cómo dibuja el firmware el logo completo (pregunta 1)

### 1.1 Rutina y origen de datos

- **OBSERVED:** el logo se pinta con `0x080076D0(dir_flash)`, llamada con `dir_flash = 0x20000` desde `main`
  (`0x080164B8`), desde el bucle principal (`0x08016508`) y desde el manejador `CLE` con `[0x0B]=0x43`
  (`0x0800764C → 0x08007650`; `CLE` con `[0x0A]='D'`, `[0x0B]='C'` es lo que manda la app al cerrar). **No pasa por
  JPEG ni por la RAM del MCU**: es DMA de flash a SDRAM del LCD más copia BTE.
- **OBSERVED:** la fuente es una **flash NAND SPI** (comandos `0x13` lectura de página `0x0800A498`, `0xD8` borrado de
  bloque con dirección de fila de 16 bit `0x0800A206`, `0x1F` set-feature `0x0800A44E`, páginas de 2 KB `0x0800A406`
  → `0x800`; bloques de 128 KB = 64 páginas). `0x0800A1E8` fija el feature `0xB0` (modo lectura continua sin buffer y
  ECC). Dirección `0x20000` → página 64 = bloque 1. **INFERRED:** la NAND cuelga del SPI del RA8876 (el DMA de
  `0x080079AC` la lee con los registros `0xB6..0xC9`).
- **OBSERVED:** el tamaño leído es `ancho × alto × 3` = 480·854·3 = `0x12C3C0` (cálculo en `0x080079D2..0x080079DC`).
  `LOG` con longitud exacta `0x12C3C0` (`0x0800673A`) marca «imagen fija» (estado 0); mayor → estado 2 (animación, varios
  fotogramas de `0x12C3C0`).

### 1.2 Secuencia de registros: logo vs. `BAT` (lado a lado)

Valores en orden de bytes escritos (little-endian por registro). `(var)` = argumento. Dirección = función que escribe.

| Paso | Logo `0x080076D0(0x20000)` | `BAT` `0x08006CD4` → `0x08005810` por fila `0x08007D28` |
|---|---|---|
| Inicio de imagen MISA `0x20..0x23` (`0x08007DE0`) | ← `0x0012C3C0` (**se muestra la página 1 directamente**) | **no se toca** |
| Ancho MIW `0x24,0x25` (`0x08007E0C`) | ← 480 | no se toca |
| Ventana principal `0x26..0x29` (`0x08007E24`) | ← 0,0,0,0 | no se toca |
| Lienzo `0x50..0x53` (`0x080052F0`) | ← `0x0012C3C0` | ← `0x0012C3C0` (`0x08006CDC`) |
| Ancho lienzo `0x54,0x55` (`0x0800531C`) | ← 480 | ← 480 (`0x08006CE6`) |
| Ventana activa pos `0x56..0x59` (`0x08004D1E`) | ← (0,0) | ← (x,y de la tabla + fila) (`0x08007D44`) |
| Ventana activa tam `0x5A..0x5D` (`0x08004CF4`) | ← 480, 854 | ← (w de la tabla, 1) (`0x08007D4C`) |
| Relleno | rectángulo (0,0)-(480,854) color 0: `0xD2..0xD4` color (`0x0800635E`), `0x68..0x6B`, `0x6C..0x6F` (`0x08009442`/`0x08009408`), `0x76` ← `0xE0` (`0x080094C6`), espera `0xB6` bit0 (`0x08005358`) | solo si `0x20000106`==1 (tras `CLE ff`): lo mismo con MISA←0 (`0x08006480`), 480×854 negro |
| Modo gráfico | — | reg `0x03` &= ~4 (`0x0800642E`), pos. escritura `0x5F..0x62` ← (x,y) (`0x08006404`), índice `0x04` (MRWDP), CS bajo, byte `0x80`, DMA MCU→SPI (`0x08008870`), `w*3` bytes BGR por fila |
| Lectura de flash | `0x080079AC`: `0xB7` \|= `0x80`/`0x40`, `0xBB`, `0x01` \|= 2, `0x5E` \|= 4; NAND «page read» fila 64; `0xC0..0xC3` ← `0x00E12D00`, `0xC6..0xC9` ← `len` (páginas·2048), `0xBC..0xBF` ← 0, `0xB6` \|= 1 (arranca), espera, `0x5E` &= ~4 | — |
| BTE paso→lienzo (`0x0800792C`) | regs `0x90..0xB4`: `0x92` prof. color, `0x93..0x96`/`0x97,0x98` origen=`0x00E12D00` ancho 480, `0x9D..0xA0`/`0xA1,0xA2` destino=`0x12C3C0` ancho 480, `0xA3..0xA6` X,Y=0, `0xA7..0xAA` 480×854, `0x91`, `0x90` \|= `0x10` (arranca) | — |
| Cierre | espera ocupado, **retardo 1000 ms** (`0x08007738`, `0x0800576C`) | — (el volcado a pantalla lo hace `STP` por BTE trasera→frontal) |

Asignación de roles de `0xBC..0xC9` (fuente/destino/cantidad) por analogía con el RA8876: INFERRED.

### 1.3 Por qué `DRA` estado 3/4 difiere (y qué NO está explicado)

| | `DRA` estado 3 (`0x0800550A`, `0x080098F8`) | `DRA` estado 4 (`0x08005564`, `0x08009946`) |
|---|---|---|
| Se activa | clave `[0x0C]` con nibble alto ≠ `0xB` | clave `0xBn` (`0x080067B0..0x080067BE`); `n` queda en `0x2000010C+0xA` |
| MISA `0x20..0x23` | ← **0** (muestra la página 0) | no se toca |
| MIW, ventana principal | 480, 0 | no se toca |
| Ventana activa | (0,0) 480×854 | no se toca (queda la de la última operación) |
| Lienzo `0x50..0x53` | ← **0** (página frontal, dibuja en directo) | ← **`0x00258780`** (página 2) |
| Ancho lienzo | 480 | 480 |
| Dibujo | `0x08005810(buf,x,y,w,h)` | igual |
| Tras dibujar | nada | si `n==4` **y** `[0x20000050]==0x31`: `0x080054B8(0x258780, 0)` (MISA←0, MIW, MWUL, lienzo←0, BTE página 2 → página 0, 480×854) y ACK `0x0800A018` |

- **OBSERVED:** `0x08005810` y el escritor de filas son los mismos que en `BAT`; el ancho máximo seguro es 480
  (búfer de pila `0x5A0` = 1440 B). La cabecera que manda `orcadeck dra` (`[9..0xB]`=len, `[0xC]`=clave,
  `[0xD..0x14]`=w,h,x,y BE16) coincide con lo que lee el firmware (`0x08006764..0x080067AC`).
- **OBSERVED:** el único cambio de estado persistente del estado 3 respecto a `BAT` es MISA←0 + lienzo←0; el del estado
  4 es lienzo←`0x258780`. **El código no da razón para negro ni cuelgue**: ambos usan la misma ruta de dibujo.
- **UNKNOWN (causa del negro en estado 3 y del cuelgue en estado 4).** Hipótesis, ninguna confirmada:
  1. (INFERRED) semántica real de MISA/páginas distinta de la supuesta (p. ej. la página mostrada no es la 0 en esta
     unidad); el logo muestra MISA=`0x12C3C0`, y `CLE`/`STP` copian trasera→frontal, lo que cuadra con frontal=0, pero
     no está probado en hardware.
  2. (INFERRED) bucles de espera sin tiempo límite (`0x08005334/0x08005340/0x0800534C`: `while(estado & 8)`,
     `0x08005358`, `0x0800536C`) que se quedan para siempre si el motor del RA8876 queda ocupado; el estado 4 deja
     el lienzo en una dirección que `BAT`/`CLE` no usan.
  3. (INFERRED) tras el estado 3 las siguientes escrituras `BAT` van a la página trasera y no son visibles sin `STP`;
     eso explicaría «se queda en lo último» pero no el negro inicial.
  Descartado: pacing/ACK (la app tampoco espera ACK en AKP153E, `hid-protocol.md` §3.2), tamaño de buffer (tiras de
  2–15 KB) y formato de cabecera.

## 2. ¿Algún comando muestra un búfer de RAM a pantalla completa o rectángulos fuera de la tabla `BAT`? (pregunta 2)

| Comando | Efecto (OBSERVED) | ¿Sirve? |
|---|---|---|
| `CLE` clave 1..18 (`0x080074C0`) | rectángulo `0x55×0x55` **en la tabla** (x=`0x31`/`0xC6`/`0x15B`.., y=`0x92`/`0x126`…), color 0 (negro) | No (tabla fija, solo negro) |
| `CLE` `0xFF` (`0x0800765A`) | limpia estado de teclas, activa `0x20000106`, rellena 480×854 negro (`0x08006480`) | No |
| `CLE` `[0x0B]=0x43` | muestra el **logo de la NAND** (`0x080076D0(0x20000)`, bloquea 1 s) | Solo logo guardado; no un búfer de RAM |
| `STP` | BTE trasera→frontal completa | Es el flip, no dibuja |
| `DIS`/`HAN`/`LIG` (`0x08007C34`) | encendido/apagado/brillo por regs `0xE0..0xE4`/`0x84..0x8F` | No dibujan |
| `MOD` (`[0x0A]`→`0x20000050`, `0x0800685E`) | fija el modo. Solo lo lee `DRA` estado 4 (`0x08009950`, cmp `0x31`) | **Habilita la única ruta diseñada de «fondo a pantalla completa»** |
| `DRA` estado 4, clave `0xB4`, modo `0x31` | dibuja JPEG (x,y,w≤480) en la **página 2 persistente** y la copia entera a la frontal | **Candidata** (ver §4); no probada |
| `DRA` estado 3 | dibuja en la frontal en directo | Candidata, pero «negro» observado |
| `LOG` | escribe NAND (§3) | Descartada (flash) |

Resumen: no hay otro comando que muestre RAM a pantalla completa. La única ruta hecha para ello es `MOD`=`0x31` +
`DRA` clave `0xB4`; la prueba del 2026-11-13 usó `0xB1` (nibble 1) y sin `MOD`, por lo que **no ejercitó la copia**.
El cuelgue observado con `0xB1` ocurre antes de ese paso, así que no queda explicado.

## 3. Qué manda la app oficial al AKP153E (pregunta 3)

- **OBSERVED (DLL):** `writeDataToHidDevice` reparte por tipo de `ImageStruct` en `0x180023A2x..0x180023EDC`:
  tipos `0x10/0x11/0x12/0x13` → `DRA` (`getSecondaryScreenPicInfo` `0x180019AE0`, llamada `0x180023A4E`);
  tipo 1 → `BAT` (`0x18001EE10`, llamada `0x180023AEF`); tipos 2, 3, 4, 5 → `LOG` (`sendLogoSizeCommand`
  `0x18001EA40`, llamada `0x180023ED7`); tipo `0x14` → `sendGIFToJPGInfo` (`0x18001E000`, llamada `0x180023EBD`). Los tipos
  2/3/5/`0x14` rechazan tamaños > `0xFFFFFF`, el 4 > `0xFFFFFFFF` (`0x180023E73..0x180023E91`).
- **Formato `LOG` (OBSERVED, `0x18001EA40`, offsets del informe con report-id en `[0]`):** `[1..3]="CRT"`, `[6..8]="LOG"`,
  `[9..0xC]` = tamaño BE32, `[0xD]` = clave; luego los datos en paquetes de `maxPacket`. En el firmware (sin report-id):
  longitud `[8..0xB]`, clave `[0xC]`.
- **Formato GIF/salvapantallas (OBSERVED, `0x18001E000`):** `[1..3]="CRT"`, `[4]`=argumento de 1 byte, `[5]=0xFF`,
  `[6..8]="LOG"`, `[9..0xC]` = tamaño BE32; sin byte de clave. Qué hace exactamente el firmware con `[4]`/`0xFF`:
  **UNKNOWN** (el parser de `LOG` solo ve clave 0 → sector `(0·3−2)<<17`, no verificado).
- **¿Escribe flash?** **Sí, siempre**: `LOG` borra `ceil(len/128K)` bloques de NAND desde el bloque `3·clave−2`
  (`0x08006554..0x0800657E`) y programa páginas de 2 KB con 5 ms de pausa (`0x0800997E..0x080099F6`); el logo de
  arranque es la clave 1 (`(3·1−2)<<17 = 0x20000`). Mensajes de la DLL: «Logo o salvapantallas o GIF…» (`0x182780`…).
- **Tamaño de pantalla completa:** **OBSERVED** en el log de la app (`%APPDATA%\HotSpot\StreamDock\logs\
  log-2024-02-20-12-12-35.txt` y otros, línea «Logo…:1 …:1229760») que la app envió un `LOG` de **1 229 760 B =
  480×854×3** (fotograma crudo de 24 bpp). Esos logs son de otra unidad (serie `33FF4DFF3054`, no la `355499441494`
  actual): modelo exacto **INFERRED**. El firmware coincide (`len == 0x12C3C0` = imagen fija).
- **Orden de canales del logo crudo:** **UNKNOWN** (la app lo construye en el EXE; el DMA+BTE copia los bytes tal cual;
  la ruta `BAT` sí invierte RGB→BGR).
- **`DRA` en AKP153E:** **OBSERVED** que la app no lo usa (celdas laterales = teclas `BAT` 16–18; `DRA` es para los
  modelos «SecondaryScreen»). No se encontró en la app ningún `MOD` con `0x31` para este modelo (`addModeChangedCommand`
  `0x180016930` no tiene llamadores directos; **no verificado** si el EXE lo invoca por vtable).
- **No hay** ningún envío de pantalla completa por RAM en la app para el AKP153E: lo único «grande» es `LOG` a flash.

## 4. Propuesta (pregunta 4)

**Con `BAT` solamente no existe forma de unir las celdas laterales** (OBSERVED, `side-panel-lcd.md` §5): el hueco entre
celdas está en la dimensión x del panel, recortada a 85/80 por la tabla, y `CLE` solo pinta negro en la tabla. **No existe
ninguna secuencia host-only demostrada.**

Única vía host-only con base en el firmware, **no probada, con riesgo de cuelgue sin explicar**, y que además cambia el
modo del dispositivo; solo para decidir con el usuario, no se ha ejecutado:

1. `CRT\0\0MOD` con `[0x0A]=0x31` (fija `0x20000050`).
2. `CRT\0\0DRA` clave `0xB4`, x,y,w≤480,h, JPEG baseline 4:4:4 pequeño (≤ 15 KB como en las pruebas): dibuja en la página 2
   (persistente entre llamadas) y, al terminar, la copia a la frontal (`0x080054B8`) y responde `ACK`.
3. Repetir (2) por cada franja que cubra el hueco; el contenido de cada `DRA` se acumula en la página 2.
4. Para volver al modo normal: `MOD` `[0x0A]=0x32` (valor de arranque, `0x080164CA`) y `CLE 0xFF`.

Riesgos y puntos a decidir antes de probar: la causa del cuelgue de `DRA` 4 está sin explicar (§1.3); la copia a
frontal 480×854 por cada `DRA` es una BTE completa; mientras el modo es `0x31`, `STP`/`BAT` siguen usando la página 1,
así que habría que reenviar las teclas por `DRA` o volver a `STP` y hacer que no pise la zona lateral (la BTE de `STP`
copia **toda** la página 1 y sobrescribiría lo pintado por `DRA` en la frontal). **Esto último es un bloqueo serio para
un panel lateral continuo mezclado con `BAT`.**

Alternativas descartadas: `LOG` (flash NAND: desgaste y sin recuperación comprobada), `DRA` estado 3 (negro observado),
`FirmwareUpgradeTool`/parche de firmware (fuera de alcance).

Siguiente paso de menor riesgo si se quiere avanzar: emular en `docs/research/firmware/work/emu` la ruta de `DRA`
estado 3/4 contra un modelo mínimo del RA8876 (registros MISA/lienzo/estado) para ver si algún bucle de espera
(`0x08005334`) puede quedarse bloqueado; no requiere hardware.

## 5. Qué sigue UNKNOWN

1. Causa del negro (estado 3) y del cuelgue (estado 4) de `DRA` (§1.3).
2. Si la página mostrada tras `CLE ff` es realmente la 0 (modelo de páginas INFERRED).
3. Roles exactos de `0xBC..0xC9` y valores de `0x91/0x92` de la BTE (se ven escribir, sin cotejar con la hoja de datos
   del RA8876 ni con un volcado real).
4. Orden de canales y construcción del logo de 1 229 760 B en el EXE.
5. Significado de `[4]` y `0xFF` en la cabecera GIF y comportamiento del firmware ante ella.
6. Si el EXE manda alguna vez `MOD` `0x31` (no hay llamadores directos de `addModeChangedCommand`).
7. Llamador `0x08009342` de `0x0800642E` y el uso de `0x2000010F` fuera del bucle principal (no analizados).
8. Tamaño real de la SDRAM del LCD (≥ 16 MB por la zona de paso `0xE12D00`, INFERRED) y si la unidad física es RA8876.
