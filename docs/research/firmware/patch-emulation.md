# AKP153E: revisión en seco y emulación del parche pulsar/soltar

Fecha: 2026-11-10. **Solo ficheros.** No se abrió el HID/USB, no se ejecutó `FirmwareUpgradeTool.exe` ni la app,
no se envió ningún comando `CRT` y no se flasheó nada. El contenido del `.bin` se trató como dato no confiable.

Etiquetas: **OBSERVED** = visto en bytes, desensamblado o emulación. **INFERRED** = deducción sin prueba.

## Veredicto

**El parche v1 hace lo que dice, pero tiene un fallo que se reproduce: puede perder el informe de soltar.**
Si el host no ha recogido todavía el informe de pulsar cuando sale el de soltar, `0x08009B1C` ve el EP ocupado y
**descarta el de soltar**. El host se queda con la tecla pulsada para siempre. En el original esto no pasa, porque
solo hay un informe por pulsación.

Se propone una **v2** (`AKP153E_pressrelease_v2_NO_PROBADO.bin`). Antes de cada envío espera, con un límite, a que el
EP quede libre. En la emulación pasa todas las pruebas y aguanta un host hasta unos 6 ms más lento.

**¿Es seguro flashearlo?** Como código, **v2 sí, con confianza media-alta (≈80 %)**: es correcto en Thumb-2, en
registros y pila, y su flujo coincide con el original salvo en el byte `[10]` y el informe extra. Como operación,
**no**: los riesgos de §6 (bootloader, recuperación, comportamiento real del USB) no se pueden validar en seco.
Sigue valiendo la condición de `firmware-feasibility.md`: flashear solo con una vía de recuperación probada (SWD o una
unidad de sacrificio). No hay que flashear v1: v2 lo sustituye.

| Imagen | Tamaño | SHA-256 | CRC-16/ARC (el de la herramienta) |
|---|---|---|---|
| `backup/AKP153E.bin` (original) | 96 496 B | `15e03f5b5a593d665a382d500d8acf23b7e5aed87f64e4b7da450d1a36f54f6f` | `0xC065` (coincide con `updatetool-origin.md`, lo que valida el algoritmo) |
| `work/AKP153E_pressrelease_NO_PROBADO.bin` (v1) | 96 496 B | `a7ea1549c454d1933d273fb16efddbc42048886df124b24ec5cde71182948fc4` | `0x9991` |
| `work/AKP153E_pressrelease_v2_NO_PROBADO.bin` (v2) | 96 496 B | `4075f5e5c703f8b3d4df0a5aee3bb9afeec4401190fc8687ba99556b27e5131a` | `0x18A7` |

## 1. Diff de v1 (OBSERVED)

`cmp` byte a byte: **50 bytes distintos**, todos en los 7 bloques declarados. El resto de la imagen es idéntico, con
la tabla de vectores, el dispatcher `CRT`, la tabla de scatter-load `0x0801B864` y los datos RW comprimidos.

| Sitio | Original | v1 |
|---|---|---|
| `0x08006F92` / `0x08007022` / `0x080070A4` | `bl 0x08007188` (línea ON) | `bl 0x0801ABA4` (`stub_press`) |
| `0x08006FAA` / `0x0800703A` / `0x080070BC` | `bl 0x08009B10` (envío) | `bl 0x0801AB9C` (`stub_release`) |
| `0x0801AB9C..0x0801ABBF` | `"Read from EMS failed\0\0\0\0Write to"` | stub de 36 B (+ el resto `" failed\0"` en `0x0801ABC0`) |

Desensamblado de v1 con capstone 5.0.7, Thumb, en `0x0801AB9C`:

```
0801ab9c: 0022       movs r2, #0
0801ab9e: 8272       strb r2, [r0, #0xa]        ; TX[10]=0  (r0=TX lo pone el llamador)
0801aba0: eef7b6bf   b.w  #0x8009b10             ; tail-call send(TX,0x200)
0801aba4: 10b5       push {r4, lr}
0801aba6: ecf7effa   bl   #0x8007188             ; línea(1,key) con r0/r1 del llamador
0801abaa: 0448       ldr  r0, [pc, #0x10]        ; -> 0x0801ABBC = 0x2000093C (TX)
0801abac: 0121       movs r1, #1
0801abae: 8172       strb r1, [r0, #0xa]        ; TX[10]=1
0801abb0: 4ff40071   mov.w r1, #0x200
0801abb4: eef7acff   bl   #0x8009b10             ; informe "pulsar"
0801abb8: 10bd       pop  {r4, pc}
0801abbc: 3c090020   .word 0x2000093C
```

Bucle de teclas del original, fila 0 (las filas 1 y 2 son iguales, en `0x08007016` y `0x08007098`):

```
08006f86: add r0, fp ; ldrb r4,[r0,#0xa] ; strb.w r4,[sb,#9]   ; TX[9]=tabla[col+10]
08006f8e: mov r1, r4 ; movs r0,#1
08006f92: bl 0x8007188                    ; <- parcheado (pulsar)
08006f96: bl 0x8007108 ; cmp r0,#0 ; bne  ; espera activa mientras siga pulsada
08006f9e: mov r1, r4                      ; r0 = 0 (resultado del bucle)
08006fa0: bl 0x8007188                    ; línea OFF (sin cambios)
08006fa4: mov.w r1,#0x200 ; ldr r0,[pc,#0x150]   ; -> 0x080070FC = 0x2000093C
08006faa: bl 0x8009b10                    ; <- parcheado (soltar)
08006fae: b 0x8006f62                     ; movs r2,#1 ; movs r1,#0 ; ldr r0,... (no lee r0-r3 ni flags)
```

## 2. Comprobaciones a mano (OBSERVED salvo que se indique)

| Comprobación | Resultado |
|---|---|
| Codificación BL/B.W | Los 6 BL de los sitios y los 3 del stub se decodificaron con capstone y con un decodificador propio de BL T1 (`bls.py`): destinos `0x0801ABA4`/`0x0801AB9C`/`0x08007188`/`0x08009B10`. Distancia máxima ≈ 0x13C00, lejos del límite de ±16 MB |
| Bit Thumb / estado | `BL imm` y `B.W` nunca cambian de estado; no hay `BLX` ni `BX` a registros. Todo destino es par y empieza en una instrucción válida |
| Alineación | Las instrucciones están alineadas a 2. `LDR literal`: `Align(0x0801ABAA+4,4)+0x10 = 0x0801ABBC`, alineado a 4, dentro del hueco, valor `0x2000093C` |
| Pila / AAPCS | `stub_press`: `push {r4,lr}` / `pop {r4,pc}` (8 B, mantiene la alineación a 8). `stub_release` no usa pila: hace tail-call con el `lr` del llamador. Profundidad extra: 8 B (v1) o 16 B (v2) |
| Callee-saved r4–r11 | Ningún stub escribe r4–r11. `0x08007188` hace `push.w {r0-r12,lr}`/`pop.w {r0-r12,pc}` y `0x08009B1C` hace `push {r4-r6,lr}`. Emulado: r4–r11 y sp se comparan antes y después de cada sitio, en 34 escenarios y con `0x08007188` real o con un stub que ensucia r0–r3, r12 y los flags. **0 discrepancias** |
| Registros y flags tras la llamada | Tras el sitio de pulsar viene `bl 0x8007108`, que no lee r0–r3 ni flags. Después solo se usa `r4` (`mov r1,r4`), y se conserva. Tras el sitio de soltar se salta a `0x08006F62`/`0x08006FF2`/`0x08007082`, que escriben r0–r2 antes de leerlos y no tienen condicionales antes de un `movs`/`cmp`. El `send` original ya ensuciaba r0–r3, r12 y los flags, así que el llamador lo tolera |
| `r0`/`r1` en `stub_release` | Los tres llamadores cargan `r0` desde `0x080070FC` (= `0x2000093C`) y `r1=0x200`. `stub_release` solo toca `r2` y `[r0+10]` |
| Literal pools | Uno solo en v1 (`0x0801ABBC`) y tres en v2 (`0x0801ABDC/E0/E4`). Todos dentro del hueco y al alcance del `LDR T1` |
| Hueco libre: xrefs | Búsqueda de palabras de 32 bits (alineadas o no) con valor en `0x0801AB80..0x0801AC00`. Solo aparecen las entradas de la tabla de mensajes de libjpeg `0x08019E50` (idx 34, "Read from EMS failed"), `0x08019E54` (35), `0x08019EEC` (73, XMS), `0x08019EF0` (74) y `0x08019E90` (50, que apunta a `0x0801ABF4`, fuera del hueco, y no se toca). No hay `ADR`/`LDR literal`/`MOVW+MOVT` hacia el hueco. Hay un falso positivo en `0x0801AB54`, que son cadenas |
| ¿Se usan esos mensajes? | Base de la tabla `0x08019DC8` ("Bogus message code"). No hay ningún patrón `movs rX,#34/35/73/74` seguido de `str rX,[..,#0x14]` (`err->msg_code`) entre 294 stores `#0x14`. Son `JERR_EMS_*`/`JERR_XMS_*` de `jmemdos.c`. Si se llegaran a formatear, se imprimirían bytes basura hasta el primer `\0`, sin ejecutarlos (INFERRED) |
| Autoverificación de flash | No hay literales de longitud ni de fin de app usados para checksum, ni polinomios (`0xA001`, `0x1021`, CRC32). `0x08004000` solo aparece en VTOR (`0x08009638`) y `0x0801B8F0` solo en la tabla de scatter |
| Flujo de suelta intacto | Solo se añaden `TX[10]=0` y el salto. Los 512 B enviados al soltar son **idénticos al original byte a byte** en todos los escenarios (ver §4) |
| Envío síncrono | `0x08009B1C`: `INDEX([usb+0x0E])=2` → si `[[0x200000EC]+2]` bit0 está a 1 (ocupado) **vuelve sin enviar** → `0x0800A044` (longitud) → `0x080194EC` (copia byte a byte al FIFO `0x4004C028`) → `0x0800931C` (pone el bit0 = "listo"). El buffer se puede reescribir en cuanto `send` vuelve |

## 3. Cómo se hizo la emulación

Unicorn 2.1.4 (en un venv del scratchpad; no se instaló nada global), CPU Cortex-M4, modo Thumb|MCLASS. Los scripts
están en `work/emu/` (`emu.py`, `suite.py`, `realline.py`, salida completa en `suite_all.txt`). Usan `work/rwdata.bin`,
los 0x130 B de `.data` ya descomprimidos.

- **Memoria**: flash `0x08000000` (512 KB, imagen en `0x08004000`), RAM `0x20000000` (256 KB, `.data` cargada, resto
  a 0) y periféricos `0x40000000–0x400FFFFF` como memoria genérica. Cada acceso queda registrado. Un hook marca
  como error cualquier acceso sin mapear y cualquier excepción de CPU.
- **Estado inicial**: `STP=1` (`0x2000010C`), pantalla encendida (`0x20000112=0`), `TX="ACK\0\0OK\0\0"+0`.
  Punteros USB que en el dispositivo pone la inicialización: `[0x200000E8]=0x4004C000`, `[0x200000EC]=0x4004C010`,
  `[[0x200000D8]+0x14]=0x4004C010` (INFERRED, por el uso en `0x08009B1C`/`0x0800931C`).
- **Matriz modelada en los registros reales** (OBSERVED en el código): las filas son salidas activas a nivel bajo:
  `0x40014008` bit0 (fila 0), `0x40014008` bit1 (fila 1) y `0x4001D008` bit1 (fila 2). Las columnas son entradas:
  `0x4001200E` bits 0..2 (columnas 2..4) y el EPORT `0x40019004` bit4/bit0 (columnas 1 y 5). Así se ejecutan de
  verdad `0x08009766`, `0x08007108`, `0x08006020` y `0x08008AD0`. Tabla tecla→(fila,col) sacada de `0x080070DC`.
- **Tiempo virtual**: `0x0800576C` (retardo en ms) se salta y suma `r0` ms. Cada llamada a `0x08007108` suma 0,02 ms.
  En v2, cada vuelta de espera suma 0,1 µs (≈20 ciclos, INFERRED).
- **Host**: el bit0 de `0x4004C012` se pone a 1 en `0x0800931C`. En ese momento se captura el informe (bytes
  escritos al FIFO). El host lo borra pasados *latencia* ms. Un intento de envío con el bit a 1 cuenta como
  **descartado**.
- **Funciones externas**: `0x08007C34` (pantalla) es un stub que registra. `0x08007188` (línea amarilla) va en
  modo stub (registra `(on,key)` y ensucia r0–r3, r12 y los flags) **o real** (`realline.py`). En los dos modos
  vuelve y conserva los registros. El watchdog `0x40005006` (`0x5555`/`0xAAAA`, `0x0800A474`) se cuenta.
- Se llama a `0x08006EF4` en bucle, como hace `main` (`0x080164FC`), con *canarios* en r4–r11 y un sp fijo. Tras
  cada retorno se comprueba que se restauran.

## 4. Resultados (OBSERVED en emulación)

Notación: `[tecla,estado@ms]`. Los 512 B de cada informe salieron siempre con la cabecera `41434b00004f4b0000` y
`[11..511]=0`. **Ningún escenario dio excepciones, accesos sin mapear, bucles sin salida ni fallos de registros**.
Los periféricos tocados son los mismos 11 en las tres imágenes; la diferencia es que v1/v2 escriben el doble en el
FIFO y el CSR.

| Escenario | Original | v1 | v2 |
|---|---|---|---|
| Teclas 1..15, 100 ms cada una | `[k,0@114]` | `[k,1@22] [k,0@114]` | igual que v1 |
| Pulsación de 3 ms / 12 ms | `[7,0@24]` | `[7,1@22] [7,0@24]` | igual |
| Ráfaga 5× (30 ms on/off) | 5 × `[1,0]` | 5 × (`[1,1]` `[1,0]`), alternos | igual |
| Mantener 60 s | `[9,0@60014]` | `[9,1@22] [9,0@60014]` | igual. Watchdog alimentado igual (cada iteración de espera) |
| 2 teclas a la vez, misma fila | nada | nada | nada |
| 2 teclas a la vez, filas distintas | solo la de la fila que se lee antes | pulsar y soltar de esa tecla | igual |
| Solapadas misma fila (2 y luego 5) | `[2,0@74] [2,0@209]` (suelta falsa de 2 al entrar 5; 5 no se ve) | `[2,1] [2,0@74] [2,1@167] [2,0]`: pares coherentes | igual |
| Pantalla apagada / sin `STP` | sin informe, solo despierta la pantalla | idéntico | idéntico |
| **Host tarda 5 ms, pulsación de 12 ms** | `[7,0@24]` | **`[7,1@22]` y el de soltar se descarta** | `[7,1@22] [7,0@27]` (espera unas 30 000 vueltas) |
| Host tarda 1 ms | ok | ok (la línea OFF tarda 2 ms en el modelo) | ok |
| Host tarda 20 ms (más que la espera) | ok | soltar descartado | soltar descartado tras unos 6,5 ms de espera |
| Host que no lee nunca | 1.º informe ok, 2.º descartado | `[7,1]`, todo lo demás descartado | igual que v1, más ≤6,5 ms de espera por envío |

Lo que muestra:

1. **Función principal: OK** en v1 y v2. Al pulsar sale un informe con `state=1` y al soltar otro con `state=0`, con
   la misma tecla. Al soltar salen los mismos bytes que en el original. La línea amarilla se enciende y se apaga
   igual: la llamada ON va antes del informe de pulsar.
2. **Fallo de v1 (F1)**: entre el envío de pulsar y el de soltar solo pasa lo que tarde `línea(0)` más una lectura
   de la matriz, en una pulsación corta. Si el host no ha leído todavía (bit0 a 1), se pierde el informe de soltar.
   En la práctica el host HS con intervalo 1 sondea cada 125 µs y el driver HID de Windows mantiene lecturas
   pendientes, así que será raro (INFERRED). Pero el resultado es grave: la tecla queda "pegada" en el host.
3. **Comportamientos que ya tenía el original** y se mantienen: (a) con dos teclas de la misma fila, `0x08007108`
   devuelve 0, lo que se lee como "soltada"; (b) **no hay antirrebote real**, porque `0x08007108` lee la matriz
   **antes** de esperar 10 ms y devuelve esa lectura, así que una pulsación de 3 ms ya cuenta; (c) de dos filas
   pulsadas solo se ve la primera.

## 5. Contención con el ISR USB (OBSERVED en estático; no emulado)

Con el grafo de llamadas BL se ve que `send_ack` (`0x0800A018`) se llama desde el **IRQ 24** (vector 40 =
`0x0800977F`): ruta `0x0800977E → 0x08009F8C → 0x0800983C → 0x0800A018` (`0x08009968` en la ruta DRA con `MOD='1'`,
`0x08009A20` al terminar una imagen). Usa **el mismo buffer TX** y el mismo EP2, y solo reescribe `[0..6]`.

- Mientras se mantiene una tecla, un ACK del ISR saldría con `[9]=tecla`. En el original lleva `[10]=0`, así que el
  host lo ve como un clic falso. Con v1/v2 lleva `[10]=1`, es decir, una **pulsación duplicada**: inocua si el host
  guarda el estado por tecla, peor si cuenta flancos. Lo comprobó la sonda `ISR-ACK veria` en todos los escenarios.
  Recomendación para el host: ignorar `state=1` si esa tecla ya está pulsada, y no tratar como tecla un ACK que
  llega justo tras subir una imagen.
- **Carrera de FIFO** (ya estaba en el original): si el ISR entra entre la comprobación de ocupado y el bit "listo"
  de un `send` del bucle principal, mezcla dos paquetes en el FIFO. v1/v2 hacen el doble de envíos desde `main`, así
  que la exposición se duplica. La ventana es de unos 512 accesos al FIFO por envío.
- v2 escribe `INDEX=2` en cada vuelta de la espera, igual que `0x08009B1C`. Si el ISR cambia `INDEX` entre medias,
  la vuelta siguiente lo corrige. En el peor caso la espera termina antes o agota el límite: siempre está acotada.

## 6. Riesgos que la emulación no puede validar

| Riesgo | Por qué queda abierto | Gravedad |
|---|---|---|
| Semántica real del bit "ocupado" (`[CSR+2]` bit0) | Se supuso que el hardware lo borra al transmitir (tipo MUSB TxPktRdy). Si no lo borra, o hay doble buffer, la espera de v2 agota el límite y se comporta como v1 | Media |
| Duración de `línea(0)` y del bucle de espera | Fijan el margen de F1 y el límite de v2 (`0x10000` vueltas ≈ 1 M ciclos; con un reloj de 100–200 MHz son 5–10 ms, INFERRED). `SystemCoreClock` (`0x20000058`) se fija en tiempo de ejecución | Baja |
| Watchdog | `0x0800A474` lo alimenta en cada `0x08007108`. v2 añade como mucho unos 10 ms sin alimentarlo por envío, del mismo orden que el retardo de 10 ms del original. El timeout del watchdog es desconocido | Baja |
| Pila | +8 B (v1) o +16 B (v2) en `main`. El tamaño real de la pila no se conoce. La pila está en lo alto de `.bss`, con SP `0x200321C0` | Muy baja |
| Interrupciones, DMA, LCD real | No se emularon (el ISR USB, el SPI LCD, el temporizador). `línea()` se ejecutó real, pero sin hardware detrás | Baja |
| Bootloader y carga | El bootloader no está en el `.bin`. El protocolo D2/D3 con CRC-16 es OBSERVED desde la herramienta (`updatetool-origin.md`), pero cómo se recupera de una carga interrumpida, el aviso `0x21436587` en flash y si comprueba algo más son INFERRED | **Alta** (riesgo de brick) |
| Protección de flash / borrado | Granularidad de borrado, si `0x08004000` está protegida y la alineación de escritura del MCU (C*Core, sin identificar). La imagen mide 24 124 palabras exactas (múltiplo de 4) y tiene el mismo tamaño que el original, así que no cambia el borrado | Media |
| App oficial | El grupo `293S` del EXE ignora `state`: con el parche **cada acción se dispara dos veces** | Conocido: usar solo `orcadeck` |
| Unidad distinta | Si el firmware de la unidad no es byte a byte `15e03f5b…`, los `assert` del script fallan (bien), pero un parche aplicado a ciegas sería peligroso | Media |

## 7. Parche v2 (propuesto, NO PROBADO)

`work/make_patch_v2.py` comprueba el SHA-256 del original y los 88 B de cadenas EMS/XMS (`0x0801AB9C..0x0801ABF3`) y
reescribe los mismos 6 sitios. Bytes cambiados: 88 (24 en los sitios y el resto en el hueco).

```
0801ab9c: movs r2,#0 ; strb r2,[r0,#10] ; b 0x801abb8         ; stub_release -> wait_send
0801aba2: push {r4,lr}                                        ; stub_press (sitios: bl 0x801aba2)
0801aba4: bl 0x8007188                                        ; línea(1,key)
0801aba8: ldr r0,[pc,#0x30]  (=0x2000093C) ; movs r1,#1 ; strb r1,[r0,#10]
0801abae: mov.w r1,#0x200 ; bl 0x801abb8 ; pop {r4,pc}
0801abb8: push {r4,lr} ; mov.w r4,#0x10000                    ; wait_send(r0=buf,r1=len)
0801abbe: ldr r2,=0x200000E8 ; ldr r2,[r2] ; movs r3,#2 ; strb r3,[r2,#14]   ; INDEX=2
0801abc6: ldr r2,=0x200000EC ; ldr r2,[r2] ; ldrb r2,[r2,#2] ; lsls r2,r2,#31
0801abce: beq 0x801abd4 ; subs r4,#1 ; bne 0x801abbe          ; espera acotada
0801abd4: pop.w {r4,lr} ; b.w 0x8009b10                       ; send() con su propia comprobación
0801abdc: .word 0x2000093C, 0x200000E8, 0x200000EC
```

Repetida la validación (§2 y §4): codificaciones correctas según capstone, literales `0x0801ABDC/E0/E4` dentro del
hueco y al alcance, sitios de llamada `bl 0x0801ABA2`/`bl 0x0801AB9C`, r4–r11 y sp intactos, ninguna excepción ni
acceso fuera de mapa. Los mismos informes que v1 en todos los escenarios, más el caso del host lento a 5 ms
corregido. `línea()` ejecutada real: `[5,1@20] [5,0@110]`.

## Ficheros

- `work/make_patch_v2.py`: genera v2 y comprueba el original y el hueco.
- `work/AKP153E_pressrelease_v2_NO_PROBADO.bin`: **NO PROBADO, no flashear sin la red de seguridad de
  `firmware-feasibility.md` §6**.
- `work/emu/emu.py`, `suite.py`, `realline.py`, `suite_all.txt`, y `work/rwdata.bin`. Uso:
  `python -I work/emu/suite.py <orig.bin> <parche.bin> [...]`, en un venv con `unicorn` y `capstone`.
- Las herramientas auxiliares de análisis estático (`fwdis.py`, `xref.py`, `bls.py`, `cg.py`) quedaron en el
  scratchpad de la sesión. No se modificaron ni el original ni v1.
