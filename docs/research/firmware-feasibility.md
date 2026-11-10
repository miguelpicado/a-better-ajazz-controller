# AKP153E v1: ¿es viable un firmware parcheado que envíe pulsar/soltar?

Fecha: 2026-11-10. Análisis estático, **solo lectura**: no se abrió el HID, no se envió nada al dispositivo, no se
flasheó nada y no se arrancó ni se paró Stream Dock. Dispositivo: VID `0x0300`, PID `0x1010`, firmware `V2.293S.00.003`.

Etiquetas: **OBSERVED** = leído en bytes o código, con dirección. **INFERRED** = deducción, sin prueba.
Direcciones `0x080…`/`0x200…` = firmware. `0x14…` = `Stream Dock AJAZZ.exe`. `0x18…` = `SDLibrary1.dll`.

## Veredicto

**Viable con condiciones; hoy está bloqueado.** El cambio en el código es pequeño y está localizado: 6 llamadas y
36 bytes en una zona libre, sin checksum que recalcular. Lo que falta es **la forma de cargarlo**. En este equipo no
existe ninguna herramienta que actualice el firmware del AKP153E, y el bootloader (`0x08000000–0x08003FFF`) no viene
en el `.bin`, así que no se conoce el protocolo con que recibe la imagen ni qué comprueba. Mientras no se consiga el
bootloader (volcado por SWD) o la herramienta oficial de actualización, **no hay forma segura de flashear**.

| Aspecto | Valoración |
|---|---|
| Parche del bucle de teclas | Fácil (OBSERVED: diseño completo en §4; imagen de trabajo generada, **NO PROBADA**) |
| Formato de la imagen | Sencillo: binario plano, sin cifrado, firma ni CRC dentro (OBSERVED) |
| Carga en el dispositivo | **Desconocida**: no hay herramienta local y el bootloader no está en el `.bin` |
| Riesgo de brick con la ruta oficial (`APP`+`NEW`) | **Alto**: el aviso de actualización queda guardado en flash (§3) |
| Recuperación | Depende de SWD/ROM boot de un MCU poco documentado (C*Core, INFERRED). Sin verificar en la placa |
| Esfuerzo | 1–2 días si se consigue el protocolo o el SWD; semanas o imposible si el MCU tiene el debug bloqueado |

## 0. Ficheros y respaldo

| Fichero | Tamaño | SHA-256 | Nota |
|---|---|---|---|
| `%APPDATA%\HotSpot\StreamDock\update\AKP153E.bin` (original, intacto) | 96 496 B | `15e03f5b5a593d665a382d500d8acf23b7e5aed87f64e4b7da450d1a36f54f6f` | Fecha 2024-02-20, el día de la instalación |
| `docs/research/firmware/backup/AKP153E.bin` (copia) | 96 496 B | `15e03f5b…54f6f` (idéntico) | Copiado con `cp -p`; el original no se tocó |
| `%APPDATA%\…\update\Stream-Dock-AJAZZ-Installer_Windows_global.exe` | 377 741 944 B | `61b104dc5f22a364800d75727b6008675f393c87e08528862180d8acd0bc1625` | Es el instalador de la app, no firmware. **No se copió** (377 MB en el repo); queda registrado el hash |
| `C:\Program Files (x86)\Stream Dock AJAZZ Global\FirmwareUpgradeTool.exe` | 1 809 384 B | `f02eb5b7e2b609c1d87583f200e57a6bd71871879d94656176979f2353eed606` | Herramienta para **otro** chip, ver §3 |
| `docs/research/firmware/work/AKP153E_pressrelease_NO_PROBADO.bin` | 96 496 B | `a7ea1549c454d1933d273fb16efddbc42048886df124b24ec5cde71182948fc4` | **NO PROBADO.** Solo sirve para diff y desensamblado. **No flashear** |
| `docs/research/firmware/work/make_patch.py` | — | — | Genera la imagen anterior y comprueba los bytes originales antes de parchear |

## 1. Formato del `.bin`

| Campo | Valor | Evidencia |
|---|---|---|
| Tipo | **Binario plano** de la aplicación Cortex-M. Empieza directamente por la tabla de vectores | OBSERVED: `[0]`=SP `0x200321C0`, `[4]`=Reset `0x08004159` |
| Dirección base | `0x08004000` | OBSERVED: los vectores apuntan a `0x0800xxxx`, `VTOR = 0x08004000` en `0x08009626..0x08009630`, y la tabla de regiones (`0x0801B864`) solo cuadra con esa base |
| Cabecera propia | Ninguna | OBSERVED |
| Cifrado | **No** | OBSERVED: el código desensambla limpio de principio a fin y hay cadenas en claro. La entropía global es 7,13 bits/B, sin bloques ≥ 7,2 (REA `inspect_byte_entropy`) |
| Compresión | Solo los **datos RW iniciales** (0x130 B → 0x6C B), con el compresor LZ de `armlink` (ARM Compiler). El código no está comprimido | OBSERVED: tabla de scatter-load en `0x0801B864` = `{src 0x0801B884, dst 0x20000000, len 0x130, fn 0x08004BF4 (__decompress)}`, `{src 0x0801B8F0, dst 0x20000130, len 0x32090, fn 0x0800A538 (zero-init)}` |
| Descompresión | Hecha. Token: bits 0–2 = nº de literales+1, bit 3 = back-ref, bits 4–7 = longitud (0 → byte extra). Se consume exactamente hasta `0x178F0` = fin del fichero | OBSERVED: rutina `0x08004BF4`. Reimplementada en `…\scratchpad\decomp.py`, salida `rwdata.bin` |
| Versión embebida | **`V2.293S.00.003`** en RW `0x200000BA` (antes se veía truncada porque está comprimida) | OBSERVED. Coincide con lo que reporta el dispositivo |
| Checksum / CRC / firma | **Ninguno dentro de la imagen.** No hay cola tras los datos RW, ni tablas CRC32 o MD5/SHA, y los slots reservados de vectores `0x1C–0x2B` son 0 | OBSERVED. Que el bootloader calcule algo por su cuenta: desconocido |
| Alcance | **Solo la aplicación** (`0x08004000–0x0801B8EF`). Fuera quedan el bootloader `0x08000000–0x08003FFF` y el sector de configuración `0x0807E000` | OBSERVED (VTOR, lectura de `0x08003800`, escritura de `0x0807E000`) |
| ¿Es el firmware que lleva la unidad? | Sí, la misma versión y build (INFERRED, con alta confianza): versión idéntica y número de serie USB `355499441494` codificado en la imagen (`0x080196C1`), el mismo que da Windows (`USB\VID_0300&PID_1010\355499441494`) | OBSERVED en ambos lados |

## 2. MCU

| Dato | Valor | Evidencia |
|---|---|---|
| Núcleo | **Cortex-M4F** (Thumb-2 con FPU; 191 instrucciones VFP; `CPACR \|= 0xF00000`) | OBSERVED `0x0800961C` |
| Fabricante / familia | **C*Core (国芯, Suzhou)**, MCU de seguridad. INFERRED por: drivers `Source\drv\src\eport_drv.c` (EPORT es la nomenclatura de C*Core), detectores de manipulación "PCI VD_L/VD_H/TD_L/TD_H touch!" y "SDIO static/dynamic-N touch!", "nvram clean!", y API de flash en **ROM de máscara `0x04005xxx`** | OBSERVED: cadenas en `0x08005F14`, `0x080080C7…0x08008238`; punteros ROM `0x04005C51`/`0x04005AFB`/`0x04005AF5`/`0x04005ACD` (`0x0800595C`) |
| Modelo exacto | Sin identificar. **No es STM32/GD32**: los registros de reset, reloj y flash no coinciden | — |
| Flash | ≥ 512 KB en `0x08000000` (INFERRED: el sector de configuración está en `0x0807E000`) | OBSERVED: la dirección |
| RAM | ≥ ~201 KB en `0x20000000`. `.data` + `.bss` llegan justo a `0x200321C0`, que es el SP inicial | OBSERVED |
| Mapa de flash | `0x08000000–0x08003FFF` bootloader (16 KB, con una palabra leída en `0x08003800`) · `0x08004000–0x0801B8EF` aplicación (96 496 B) · `0x0807E000` configuración y aviso de actualización | OBSERVED (`0x08005390`, `0x08004C64`) |
| Reset por software | Bit 31 de `0x40002000` (registro del fabricante; no usa `AIRCR`) | OBSERVED `0x080093F8` |
| USB | **USB 2.0 High-Speed**: EP `0x82` IN y `0x01` OUT, interrupt, **512 B**, intervalo 1 | OBSERVED: descriptores en `0x0801967A…0x08019711` |
| Otros periféricos | SPI2 `0x40003800` (LCD), JPEG por software (libjpeg 9a, 2014), I2C, PWM, RTC, tamper | OBSERVED: cadenas y literales |

### Descriptores USB (OBSERVED, `0x0801967A`)

| Descriptor | Contenido |
|---|---|
| Dispositivo | `12 01 0002 00 00 00 40 0300 1010 0003 01 02 03 01` → bcdUSB 2.00, EP0 64, VID `0x0300`, PID `0x1010`, bcdDevice 3.00 |
| Configuración | 1 interfaz HID, bus-powered + remote wakeup, 100 mA. HID bcd 2.00, report descriptor de 54 B (usage page `0xFFA0`, 512 B de entrada y 512 B de salida) |
| Cadenas (handler `0x08009C60`) | idx 0 = LANGID 0x0409. idx 3 = `"355499441494"` (fijo en la imagen). **idx 1 e idx 2 reutilizan los primeros bytes del número de serie**: fabricante `"355"` (8 B) y producto `"35549"` (12 B) |

**Corrección a `keystate-modes.md`/`hid-protocol.md`:** nuestra unidad **no** se anuncia como `"AJAZZ AKP153E"`. El
fabricante USB es `"355"` y el producto `"35549"`. Es OBSERVED en el firmware y en 308 líneas del log de la app
(`hid_get_product_string … Product String: 35549`). La app llega al UUID `AKP153E` por **VID/PID** (fábrica AjazzE),
no por la cadena de producto. La variante con espacio final (`"AJAZZ AKP153E "` → `APK153E_293SV3`) es otro firmware y
probablemente otro hardware (REV2, PID `0x3010`, `V3.AKP153E.02.009`, según `community-ideas.md`). No hay que
imitarla: el host propio (orcadeck) ya lee el byte `[10]`.

## 3. Proceso de actualización

### 3.1 Lo que hace el firmware (OBSERVED)

`CRT\0\0APP` con `[8..10]="NEW"` (dispatcher `0x08006870..0x080068A8`):

1. `0x08007C34(0,0,200,100,…)`: apaga o atenúa la pantalla (la misma llamada que el modo reposo).
2. `0x08004C4C`: inicia la flash mediante la ROM (`0x0800595C`). Lee 10 palabras de `0x0807E000`, **borra el sector**
   y reescribe `[+0]`, `[+4]=~[+4]`, `[+8]`, `[+0xC]=~[+0xC]` (palabras de config con complemento) y
   **`[0x0807E010] = 0x21436587`** (aviso de actualización). Después imprime `APP_Updata_Flag = %2x`.
3. `0x080093F8`: reset por software.

**El código de la aplicación no recibe la imagen.** No hay manejador de bloques, de dirección ni de CRC. Todo eso lo
hace el bootloader de `0x08000000`, que **no está en el `.bin`** (INFERRED: arranca, ve el aviso y se queda esperando
la imagen).

### 3.2 Lo que hace la app (OBSERVED)

| Hallazgo | Evidencia |
|---|---|
| Ni el EXE ni la DLL construyen `APP` ni `NEW` (búsqueda de secuencias `mov dl,'X'` + `QByteRef::operator=`, el mismo patrón que genera `CRT`, `CLE` y `STP`, que sí aparecen: 35, 6 y 2 veces en la DLL). Tampoco hay literales `APP\0`/`NEW\0` | Script en scratchpad, búsqueda sobre `.text` |
| `SDSetting::updateFirmware` (`0x140211E50`): lee la versión, descarga `https://cdn1.key123.vip/StreamDock/firmware/firmware-version-libcef.json`, compara versiones y lanza **`UpDateToolV2\UpDateTool11-11.exe StreamDockUpDate`** (`0x1402215B0`, `0x1402220EB`) | Ese ejecutable **no está instalado** |
| Las dos JSON del CDN (descargadas en solo lectura) solo listan `MBox-N1` y `VSD_M18` (imágenes `.img` `d13x_…`). **El AKP153E no aparece** → la app mostraría `ServerNotFindDev` | `…\scratchpad\cdn\*.json` |
| `FirmwareUpgradeTool.exe` es para el SoC de Actions (cadenas `usbupg`, `fwc`, `ramboot`, `uboot`, `partition`, libusb, CBW/CSW). Busca `VID 0xEEEF / PID 0x2929` (`0x140013148`). No habla HID ni CRT | OBSERVED |
| La pantalla de ajustes muestra "Update mode" si `SDDevice::findDevice(0xEEEF, 0x2929)` (`0x14021421B`). Es el modo de actualización de los modelos Actions, no el del AKP153E | OBSERVED |

**Conclusión:** con lo instalado **no se puede reconstruir el protocolo HID con que el bootloader del AKP153E recibe la
imagen**: ni tamaño de paquete, ni dirección, ni verificación. Se desconoce el VID/PID en modo bootloader, si comprueba
un CRC o una firma, y si rechaza versiones. Tampoco se sabe si el bootloader está en una región protegida (estos MCU de
seguridad suelen poder bloquearla, INFERRED). Con este `.bin` todo eso queda **sin determinar**.

### 3.3 ¿Qué pasa con una imagen inválida o si no llega la imagen?

INFERRED, sin bootloader no se puede comprobar:

- El aviso `0x21436587` está **en flash**, así que sobrevive a desconectar la alimentación. Si se envía `APP`+`NEW` sin
  una herramienta que complete la carga, lo más probable es que el dispositivo **arranque siempre en el bootloader**
  y deje de funcionar como teclado hasta recibir una imagen. Es un brick blando, recuperable solo si se conoce el
  protocolo. Es otra razón para **NO enviar nunca `APP`+`NEW`**.
- Si el bootloader solo borra el aviso cuando la carga termina bien, una imagen corrupta debería dejarlo accesible
  y permitir reintentar, porque el bootloader no se toca. Con una imagen bien formada pero que falle en ejecución
  (HardFault en el arranque), sin aviso el bootloader saltaría a ella en cada arranque. **Volver al bootloader exigiría
  que la aplicación atienda `APP`+`NEW`, o SWD.** Por eso el parche no toca ni el dispatcher ni el arranque.

## 4. Parche (diseño; imagen de trabajo NO PROBADA)

### 4.1 Bucle actual (`0x08006EF4`, OBSERVED)

Barrido de una matriz de 3 filas: `0x08009766` activa una fila y `0x08007108` lee las columnas. Hay un bloque
idéntico por fila (fila 0 en `0x08006F86`, fila 1 en `0x08007016`, fila 2 en `0x08007098`):

```
TX[9] = tabla[col + 10/5/0]       ; TX = 0x2000093C (informe IN de 512 B)
bl 0x08007188 (1, key)            ; línea amarilla ON (efecto local)
while (0x08007108()) ;            ; espera activa a que se suelte
bl 0x08007188 (0, key)            ; línea amarilla OFF
bl 0x08009B10 (TX, 0x200)         ; ÚNICO informe, al soltar; TX[10] nunca se escribe → 0
```

`0x08009B10` → `0x08009B1C` **copia el buffer de forma síncrona** al EP (`0x080194EC`) y **descarta el informe si el EP
sigue ocupado** (`ldrb [..,#2]; lsls #31; bne → return`). Por eso, modificar `TX[10]` después del envío es seguro. El
riesgo es que se pierda un informe si el host no ha leído el anterior (OBSERVED).

### 4.2 Cambio mínimo (OBSERVED en el desensamblado de la imagen de trabajo)

Zona libre (code cave): `0x0801AB9C–0x0801ABC7` (44 B). Son las cadenas de libjpeg "Read from EMS failed" y "Write to
EMS failed". Solo las referencia la tabla de mensajes (`0x08019E50`, `0x08019E54`), y el gestor EMS/XMS no existe en
un MCU (INFERRED: nunca se imprimen). Así la imagen **no cambia de tamaño** y no se mueve la tabla de scatter-load.

| Dirección | Original | Parcheado |
|---|---|---|
| `0x0801AB9C` `stub_release` | texto | `movs r2,#0` · `strb r2,[r0,#10]` · `b.w 0x08009B10` (r0=TX y r1=0x200 los pone el llamador) |
| `0x0801ABA4` `stub_press` | texto | `push {r4,lr}` · `bl 0x08007188` · `ldr r0,=0x2000093C` · `movs r1,#1` · `strb r1,[r0,#10]` · `mov.w r1,#0x200` · `bl 0x08009B10` · `pop {r4,pc}` · literal en `0x0801ABBC` |
| `0x08006F92`, `0x08007022`, `0x080070A4` | `bl 0x08007188` | `bl 0x0801ABA4` → la línea amarilla se enciende igual y luego sale el informe **pulsar**: `[9]=key`, `[10]=1` |
| `0x08006FAA`, `0x0800703A`, `0x080070BC` | `bl 0x08009B10` | `bl 0x0801AB9C` → `[10]=0` y luego el informe **soltar**: `[9]=key`, `[10]=0` |

Total: 50 bytes cambiados (`cmp -l`). Se mantienen el orden de la línea amarilla local, el despertar de pantalla
(rama `r6==1`), el requisito de `STP` (`flag 0x2000010C`) y `TX[9]=0` al salir.

| Pregunta | Respuesta |
|---|---|
| ¿Espacio libre? | No hay huecos de 0x00/0xFF ≥ 24 B dentro de la imagen. Sí hay 44 B de cadenas muertas (arriba). Otra opción: añadir al final, tras `0x0801B8F0`; cambia el tamaño, y no se sabe si el bootloader lo acepta |
| ¿Recalcular checksums? | **No hay ninguno en la imagen** (OBSERVED). El bootloader o la herramienta podrían usar uno propio: desconocido |
| ¿Comprimir de nuevo? | No: el parche no toca los datos RW. Para cambiar la versión visible (p. ej. `V2.293S.00.103`) habría que reescribir el bloque RW. El formato admite tokens de solo literales, así que basta un compresor trivial de ~20 líneas, pero el bloque crece y la tabla `0x0801B864` habría que revisarla. No se ha hecho |
| Herramientas | Python + capstone (para verificar), `make_patch.py` (codifica BL/B.W a mano y comprueba los bytes originales). No hace falta toolchain ARM |
| Efecto en la app oficial | El grupo `293S` del EXE ignora `state` y dispara la acción **con cada informe** (`0x1400A8D69`). Con el parche, la app oficial ejecutaría cada acción **dos veces**. Solo tiene sentido con el host propio |

## 5. Reversibilidad y recuperación

| Punto | Estado |
|---|---|
| Original para restaurar | **Sí**: copia idéntica en `backup/`, comprobada por SHA-256. Pero el "mismo mecanismo de la app" **no existe** para el AKP153E (§3.2), así que restaurar exige el mismo cargador desconocido que flashear |
| Bootloader inmutable | Desconocido. Lo que se ve: está en flash (`0x08000000`), separado de la app, y la app no lo borra. La ROM de máscara `0x0400xxxx` del MCU sí es inmutable. Si tiene modo ISP por pin BOOT, sería la recuperación de último recurso (INFERRED, falta la hoja de datos del modelo exacto) |
| SWD / pines | Sin inspeccionar. Hay que abrir la carcasa y buscar pads SWDIO/SWCLK/BOOT. En MCU de seguridad C*Core el debug suele poder bloquearse (INFERRED) |
| Riesgo de brick | **Medio-alto** sin SWD. **Bajo** con SWD funcional y la flash completa volcada (incluidos el bootloader y `0x0807E000`) |
| Seguro de hardware | Un programador SWD (J-Link, o DAPLink/ST-Link con OpenOCD) **y el algoritmo de flash del MCU concreto**. Para C*Core puede no haber soporte en OpenOCD; a cambio, la ROM expone la API de flash (`0x04005xxx`), con la que se puede escribir un flash-loader en RAM. Se recomienda una segunda unidad AKP153E de sacrificio |

## 6. Plan recomendado (con puntos de parada)

1. **Sin abrir nada**: conseguir `UpDateToolV2\UpDateTool11-11.exe` (soporte de AJAZZ/Mirabox, o de dentro del
   instalador de 377 MB, que va comprimido) y analizarlo **en estático**: comandos HID, VID/PID del bootloader, tamaño
   de bloque, CRC o firma. **PARADA 1**: si el bootloader exige una firma criptográfica, el proyecto es **no viable**.
2. **Abrir la unidad** (solo mirar): identificar el MCU por la serigrafía, buscar los pads SWD/BOOT y conseguir la hoja
   de datos. **PARADA 2**: si no hay SWD o está bloqueado y no hay protocolo de bootloader documentado, **no seguir**.
3. **SWD solo lectura**: volcar toda la flash `0x08000000–0x0807FFFF`, guardarla con su SHA-256 y comprobar que
   `0x08004000…` coincide byte a byte con `backup/AKP153E.bin`. Desensamblar el bootloader (vector en `0x08000000`).
   **PARADA 3**: si el volcado no coincide, la unidad lleva otro firmware; empezar de nuevo con ese.
4. **Validar sin flashear**: revisar `make_patch.py` contra el volcado real y, si se puede, emular con Unicorn o QEMU
   el bucle `0x08006EF4` con `0x08007108` simulado (pulsar → mantener → soltar), comprobando que hay dos llamadas a
   `0x08009B10` con `[10]=1` y luego `[10]=0`.
5. **Flashear solo con red de seguridad**: con SWD verificado (lectura y escritura probadas en una zona inocua o en
   una unidad de sacrificio), escribir **solo** la región de la app. **No usar `APP`+`NEW`.** Verificar leyendo de
   vuelta. Si falla, restaurar `backup/AKP153E.bin` por SWD.
6. **Host**: `orcadeck` ya ignora la app oficial; tratar `state=1` como pulsar y `state=0` como soltar, y la pulsación
   larga con un temporizador (p. ej. 550 ms). No usar la app oficial con el firmware parcheado (doble disparo).

**Estimación:** pasos 1–2, medio día. Paso 3, de 1 a 3 días según el soporte del MCU. Pasos 4–6, 1 día.
**Sin SWD ni protocolo del bootloader: no hacerlo.**

## Límites de este análisis

- REA no puede abrir binarios ARM raw en Windows: Ghidra no está configurado y el proveedor P0 solo admite PE/6502.
  Por eso se usaron REA `inspect_byte_entropy` y capstone 5.0.9 (scripts en
  `C:\Users\migue\AppData\Local\Temp\claude\2c80c354be5b99aa\281ad4cd-058d-4455-8739-62ab64a348c6\scratchpad\`:
  `decomp.py`, `lits.py`, `str.py`, `estr.py`, `fx.py`, `xr.py`, `upd*.txt`).
- La identificación del MCU (C*Core) es INFERRED. Falta ver el chip.
- Los dos JSON del CDN se descargaron con una petición GET anónima, sin datos del usuario.
