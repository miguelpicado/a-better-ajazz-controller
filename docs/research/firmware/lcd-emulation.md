# AKP153E: emulación del LCD (DRA/BAT/STP/CLE/MOD) y parches de firmware (NO PROBADOS)

Fecha: 2026-11-13. **Solo ficheros + emulación.** No se abrió el HID, no se ejecutó `orcadeck`, la app ni
`FirmwareUpgradeTool`, no se flasheó nada. Firmware base: `backup/AKP153E.bin` (`V2.293S.00.003`, 96 496 B, base
`0x08004000`, SHA-256 `15e03f5b5a593d665a382d500d8acf23b7e5aed87f64e4b7da450d1a36f54f6f`).
**OBSERVED** = visto en bytes/emulación o en el dispositivo; **INFERRED** = deducción; **UNKNOWN** = sin resolver.
**Ningún parche de este documento está probado en hardware.**

Reproducir todo: venv con `unicorn` y `pillow`, y `python -I docs/research/firmware/work/emu2/run_all.py` (exit 0 = todo OK).

| Fichero | Qué es |
|---|---|
| `work/emu2/ra8876.py` | modelo mínimo RA8876 (regs, páginas 480×854×3, puerto de datos con ventana activa, relleno, BTE copia ROP S0) |
| `work/emu2/fw.py` | arnés Unicorn: ejecuta el firmware **real** (parser CRT, receptor `0x0800983C`, BAT/DRA/CLE/STP, libjpeg, escritor de filas) y sustituye solo las primitivas de bus LCD (`0x080076C4/C8/CC`, `0x0800788A`, `0x08006194`, `0x08008930`, `0x080087F4`) |
| `work/emu2/pr_suite.py` | validación press/release (v2, v3) |
| `work/emu2/scenarios_a.py` | A1/A2: DRA 3, 0xB1, MOD 0x31 + 0xB4 |
| `work/emu2/heap_probe.py` | ancho máximo de JPEG que el firmware decodifica |
| `work/emu2/make_lcd_patch.py`, `bat_evidence.py` | parche S (tira lateral) y su evidencia |
| `work/make_patch_v3.py` | parche press/release v3 (fichero nuevo junto a v1/v2) |

---

# PARTE 1 (prioridad 1): parche pulsar/soltar

## 1.1 Estado de v2 y por qué hay v3

v2 (`AKP153E_pressrelease_v2_NO_PROBADO.bin`, de `patch-emulation.md`) se revalidó con el emulador de `work/emu/` y
sigue siendo correcto. Se endurece con **v3** = v2 + 1 cambio (6 B): tras enviar el informe «pulsar» se deja `TX[10]=0`.
Motivo (OBSERVED en `patch-emulation.md` §5): la ISR USB (IRQ 24, `0x0800A018`) envía ACK con el **mismo búfer TX**; con
v2, un ACK durante una pulsación salía con `[10]=1` (pulsación duplicada). Con v3 sale con `[10]=0`, idéntico al original.
El informe «soltar» sigue idéntico al original byte a byte. Sitios y literales de v2 no cambian.

Código del hueco (`0x0801AB9C..0x0801ABF3`, cadenas muertas EMS/XMS de libjpeg), desensamblado de la imagen v3:

```
0801ab9c movs r2,#0 ; strb r2,[r0,#10] ; b 0x801abbe        stub_release (TX[10]=0, wait_send)
0801aba2 push {r4,lr} ; bl 0x8007188                        stub_press: línea(1,key) (r0/r1 del llamador)
0801aba8 ldr r0,=0x2000093c ; movs r1,#1 ; strb r1,[r0,#10] ; mov.w r1,#0x200 ; bl 0x801abbe   TX[10]=1, enviar
0801abb6 ldr r0,=0x2000093c ; movs r1,#0 ; strb r1,[r0,#10] ; pop {r4,pc}    <-- NUEVO en v3: TX[10]=0
0801abbe push {r4,lr} ; mov.w r4,#0x10000                   wait_send: espera acotada (~1 M ciclos) a EP2 libre
0801abc4 INDEX=2 ; ldrb [CSR+2] bit0 ; beq 0x801abda ; subs r4,#1 ; bne 0x801abc4
0801abda pop.w {r4,lr} ; b.w 0x8009b10                      send() original (con su propia comprobación de ocupado)
0801abe0 .word 0x2000093C, 0x200000E8, 0x200000EC
```

Sitios de llamada (iguales que v1/v2): `bl 0x08007188` → `bl 0x0801ABA2` en `0x08006F92/0x08007022/0x080070A4` (pulsar) y
`bl 0x08009B10` → `bl 0x0801AB9C` en `0x08006FAA/0x0800703A/0x080070BC` (soltar).

### Tabla de diferencias (v3 frente al original)

97 bytes distintos, todos en `0x08006F92..0x0801ABEF`:

| Dirección | Original | v3 |
|---|---|---|
| `0x08006F92`, `0x08007022`, `0x080070A4` (4 B cada uno) | `bl 0x08007188` | `bl 0x0801ABA2` |
| `0x08006FAA`, `0x0800703A`, `0x080070BC` (4 B cada uno) | `bl 0x08009B10` | `bl 0x0801AB9C` |
| `0x0801AB9C..0x0801ABF3` (84 B de código + literales) | texto muerto `"Read from EMS failed…"` | stubs de arriba |

Generador: `python -I work/make_patch_v3.py backup/AKP153E.bin <out>` (comprueba SHA del original y el texto del hueco).

| Imagen | Tamaño | SHA-256 | CRC-16/ARC |
|---|---|---|---|
| `backup/AKP153E.bin` | 96 496 | `15e03f5b…f54f6f` | `0xC065` |
| `work/AKP153E_pressrelease_v2_NO_PROBADO.bin` | 96 496 | `4075f5e5c703f8b3d4df0a5aee3bb9afeec4401190fc8687ba99556b27e5131a` | `0x18A7` |
| **`work/AKP153E_pressrelease_v3_NO_PROBADO.bin`** | 96 496 | **`dd2fa325216ff8e09ab90fecc4ffbe543b4b752016ba6bc51fe396ed816a0df5`** | `0x1656` |

## 1.2 Emulación del bucle de teclas `0x08006EF4` (`pr_suite.py`, exit 0, 59 comprobaciones OK)

Modelo: matriz 3 filas × 5 columnas leída con el código real (`0x08009766`, `0x08007108`, `0x08006020`, `0x08008AD0`); host
que libera el EP tras *latencia* ms; r4–r11 y SP comparados antes/después de cada llamada.

| Escenario | Original | v3 |
|---|---|---|
| Teclas 1..15, pulsar 100 ms (15 casos) | 1 informe `[k,0]` | `[k,1]` y `[k,0]`, misma tecla, 512 B con cabecera `ACK\0\0OK\0\0` |
| ACK de ISR durante la pulsación (sonda `[9],[10]`) | `[10]=0` | `[10]=0` (v2 daba 1) |
| Rebote (on/off a 1 ms en pulsar y en soltar) | `[7,0]` | `[7,1]`,`[7,0]`: alterna y termina soltado. No hay antirrebote real (0x08007108 lee antes de esperar 10 ms): **el rebote se absorbe porque el bucle espera a que la lectura sea 0, no por filtrado** |
| 105 acordes (todas las parejas de las 15 teclas) | n/a | 0 errores, 0 «pulsar sin soltar» |
| Host lento 0/1/3/5/6/8 ms, tap 12 ms | ok | pulsar+soltar entregados |
| Host lento 20 ms (> espera ≈ 6,5 ms) | ok | **soltar descartado** (límite conocido de v2/v3) |
| Mantener 60 s | `[9,0]` | `[9,1]`, `[9,0]`; watchdog alimentado igual |

Las celdas laterales (teclas 16–18) **no tienen pulsador** (la matriz es 3×5 = 15; la tabla `0x080070DC` solo da 1..15).
No hay excepciones, accesos sin mapear ni fallos de registros en ningún escenario. BAT/STP/CLE/LED no se tocan:
`0x08007188` se llama igual (ON antes del informe, OFF igual que antes).

**Riesgo residual (UNKNOWN, no cerrable en seco):** semántica real del bit «ocupado» del EP2 (`[CSR+2]` bit0, supuesto
tipo MUSB), duración real de la espera acotada y timeout del watchdog, tamaño de pila, bootloader. Mitigación en host
(recomendada, cubre el descarte a ≥ 6,5 ms y los ACK): estado por tecla, ignorar `state=1` de una tecla ya pulsada, y un
timeout de «tecla pegada» (p. ej. 30 s) o resincronización al recibir cualquier otro informe.

## 1.3 Lista go/no-go para flashear press/release v3

Procedimiento conocido (coordinador): `orcadeck bootwatch`, luego `FirmwareUpgradeTool 768 4112 <bin> <icono>`; si la app
queda mal la unidad se queda en `EEEF:2929` y se puede reintentar (**no probado con imagen mala**); `backup/AKP153E.bin`
es la imagen de restauración.

- [ ] SHA-256 del `.bin` a flashear = `dd2fa325…a0df5` y el original de partida = `15e03f5b…f54f6f`.
- [ ] `backup/AKP153E.bin` íntegro y verificado a mano antes (restaurar con el mismo procedimiento).
- [ ] Probado que el modo `EEEF:2929` se alcanza y la herramienta reconoce la unidad **antes** de flashear v3.
- [ ] Unidad de sacrificio o vía SWD disponible; si no, aceptar el riesgo de soft-brick (UNKNOWN: bootloader no está en el `.bin`).
- [ ] USB estable (sin hub, sin suspender el PC); nada de la app oficial abierta (con el parche cada acción se dispara dos veces).
- [ ] Host listo con las mitigaciones de §1.2.
- [ ] Criterio de vuelta atrás: si tras flashear no hay informes de pulsar/soltar o falla el LCD → restaurar el original.
- NO-GO si: hash distinto, no se alcanza `EEEF:2929`, el firmware de la unidad no es byte a byte el del backup.

---

# PARTE 2: LCD

## A1. Qué hacen DRA 3, DRA 0xB1 y MOD 0x31 + DRA 0xB4 (emulación, `scenarios_a.py`)

**Hallazgo central (OBSERVED en emulación del código real):** el firmware **no decodifica JPEG de 480 px de ancho**.
libjpeg 9a asigna su memoria con un `malloc` de primer ajuste sobre un heap estático de `0x20028CC4..0x200301C0` =
29 948 B (`0x08016658`, inicialización perezosa en `0x0801669C`). Con un JPEG de 480 px el `alloc_small` falla
(`0x0800AD34`: `out_of_memory(cinfo,4)`), `error_exit` (`0x08016A10`) se llama con `msg_code=56` («Insufficient memory (case 4)»)
y `0x08005810` vuelve en silencio con 0 **sin escribir ninguna fila**. Todas las pruebas de hardware DRA usaban tiras de
480 px. Ancho máximo (`heap_probe.py`): **≈384 px en 4:4:4, ≈337 px en 4:2:0**; 480 falla en ambos. Tras un fallo no
hay fuga de heap (un BAT de 85 px posterior pinta sus 85 filas). Esto afecta también a BAT: un JPEG > ~384 px nunca se dibuja.

Registros y páginas que deja cada caso (modelo RA8876; páginas: 0 frontal, 1 = `0x12C3C0`, 2 = `0x258780`):

| Caso | Registros al entrar (OBSERVED, código) | Resultado en el modelo con tiras de 480 px |
|---|---|---|
| DRA estado 3 (`0x0800550A`) | MISA `0x20..0x23`←0, MIW←480, ventana principal←(0,0), ventana activa ←(0,0)+480×854, lienzo `0x50..0x53`←0, ancho←480; luego `0x08005810` | decodificación falla → 0 filas; ninguna página cambia; MISA=0 (pantalla = página 0 tal cual estaba: **negra tras `CLE ff`**) |
| DRA 0xB1 (estado 4, `0x08005564`) | solo lienzo←`0x258780`, ancho←480; sin MISA ni ventana | idem, sin copia (solo 0xB4+MOD 0x31 copia); lienzo queda en `0x258780` |
| MOD 0x31 + DRA 0xB4 (`0x08009950..0x08009964`) | como 0xB1 y después `0x080054B8(0x258780,0)`: MISA←0, lienzo←0 y BTE (S0=S1=`0x258780`, destino 0, ROP 0xC, 480×854) | decodificación falla, **pero la BTE se ejecuta**: la página 0 pasa a ser copia íntegra de la 2 (diff 0,0–479,853). En el modelo la página 2 son patrones; en hardware es SDRAM sin escribir = **ruido** |

Cómo explica lo observado en hardware (consistente con todos los datos, un solo mecanismo):

1. **Negro (DRA 3, DRA 3+STP, DRA 4 desde negro):** no se dibuja nada; la pantalla conserva la página 0 que dejó `CLE ff` (negro). `STP` copia la página 1 (negra) → negro. OBSERVED en emulación; coherente con «negro y se recupera relanzando» (INFERRED que lo observado en el dispositivo es esto).
2. **Ruido que no cambia (MOD 0x31 + 0xB4, 8 tiras):** cada tira falla igual (480 px) y la página 2 nunca se escribe; cada `DRA 0xB4` copia la página 2 virgen sobre la frontal → mismo ruido siempre. OBSERVED en emulación; la teoría anterior («el dibujo no cae en la página 2», regs `0x50..0x53`/`0x5F..0x62`) **queda descartada**: con tiras de 240 px el mismo flujo escribe en la página 2 correctamente (diff 0,200–239,306) y la copia la lleva a la frontal. La LED de pulsación sigue funcionando porque `0x08007188` dibuja en la frontal directamente.
3. **Registros 0x5F..0x62, 0x50..0x53, 0x56..0x5D:** el modelo (cursor en 0x5F..0x62, lienzo + (y·ancho+x)·3, ventana activa que envuelve el cursor) reproduce el BAT que funciona en hardware (`BAT` de 85 px: 85 transferencias de 255 B en `(x,y+fila)`, ventana 85×1) sin ningún ajuste.

**UNKNOWN (no se reproduce en emulación):**
- **Cuelgue** de DRA 0xB1 desde pantalla sana y de «DRA 3 + BAT 1 s después». En emulación ninguno cuelga (los bucles de espera `0x08005334/40/4C` solo esperan el bit3 de estado, `0x08005358` el bit0 de `0xB6` y `0x0800536C` el bit2; con el modelo terminan siempre). Hipótesis no verificables en seco: (a) la ruta de error de libjpeg imprime por la UART (`0x0800A500` → `0x0800DAF0` espera TXE sin límite; en emulación se cuelga si TXE no sube, no es hecho en hardware); (b) el estado del LCD tras `lienzo=0x258780`/BTE concreto en el chip real; (c) pila (`error_exit` hace `longjmp` dentro de la ISR USB). Ninguna se puede decidir sin hardware.
- Si el chip es realmente un RA8876 (roles de `0xBC..0xC9`, valores de `0x91/0x92`), tamaño real de SDRAM y de la memoria de la zona de paso `0xE12D00`.
- Si los bucles de espera pueden quedarse bloqueados con una BTE real (en el modelo nunca).

## A2. ¿Hay secuencia solo-host (sin cambiar firmware) para un panel lateral continuo?

**Sí, en emulación; sin probar en hardware.** Con **JPEG ≤ ~384 px de ancho (≤ 337 si 4:2:0)** el `DRA` estado 3 sí dibuja,
**directamente en la página frontal**, en cualquier `(x,y)`: `DRA 3` 240×107 en `(0,200)` pinta exactamente `(0,200)-(239,306)`;
85×85 en `(40,10)` pinta `(40,10)-(124,94)`. Secuencia: teclas con `BAT` + `STP` como hoy, **después** `DRA` clave `0x00` en 2–3 tiras
(p. ej. 3 × 160 px) que cubran `x=0..479` en la franja lateral (y de ~0 a ~140). Límites (OBSERVED en emulación):
1. `STP` (y `CLE`) copia **toda** la página 1 sobre la 0 y **borra** lo pintado por `DRA`: hay que repintar la franja tras cada `STP`
   (parpadeo; dibuja en directo, sin doble búfer).
2. `MOD 0x31` + `DRA 0xB4` **no sirve** para mezclar con teclas: copia 480×854 de la página 2 sobre toda la pantalla.
3. Nunca enviar JPEG de 480 px (silencio y pantalla sin cambios, o ruido con 0xB4).
4. Riesgo hardware: el cuelgue de DRA 3 + BAT es UNKNOWN; probar primero un solo `DRA 3` de 160 px en la franja lateral.
Con BAT solo, no: el ancho está recortado a la `w` de la tabla (85/80), como ya se midió.

## B1. Parche mínimo S (tira lateral continua) — recomendado para el firmware

Dado el límite de heap, **w=480 en una tecla no sirve** (el JPEG de 480 px no se decodifica y leer `w` px de un JPEG más
estrecho lee basura de la fila). Diseño: las 3 celdas laterales pasan a ser **3 segmentos contiguos** que cubren todo el ancho:

| Tecla | x (sin cambio) | w | y | Cobertura |
|---|---|---|---|---|
| 16 | 49 → **0** | 0x50 → **198** (`0xC6`) | 12 → **0** | x 0..197 |
| 17 | 198 (`0xC6`) | 0x50 → **149** (`0x95`) | 12 → **0** | x 198..346 |
| 18 | 347 (r1 por defecto `0x15B`, `movw` en `0x08006D0A`, compartido) | 0x50 → **133** (`0x85`) | 12 → **0** | x 347..479 |

Alto: lo manda el JPEG (hasta la fila 853). Todas las `w` caben en `movs rN,#imm8` (≤ 255), nada de `movw`; la `w` máxima
es 480 − 347 = 133 para x=347. El host parte una imagen de 480 px en 3 JPEG de 198, 149 y 133 px de ancho (exactos: si el JPEG
es más estrecho que `w`, se leen bytes de más).

**Diferencias exactas (7 bytes, `make_lcd_patch.py`):**

| Dirección | Original | Parche | Instrucción |
|---|---|---|---|
| `0x08006E26` | `50 23` | `C6 23` | `movs r3,#0x50` → `#198` (tecla 16, w) |
| `0x08006E28` | `0C 22` | `00 22` | `movs r2,#0xc` → `#0` (tecla 16, y) |
| `0x08006E2A` | `31 21` | `00 21` | `movs r1,#0x31` → `#0` (tecla 16, x) |
| `0x08006E36` | `50 23` | `95 23` | `movs r3,#0x50` → `#149` (tecla 17, w) |
| `0x08006E38` | `0C 22` | `00 22` | y tecla 17 |
| `0x08006E46` | `50 23` | `85 23` | `movs r3,#0x50` → `#133` (tecla 18, w) |
| `0x08006E48` | `0C 22` | `00 22` | y tecla 18 |

Efectos secundarios evaluados: **STP** copia toda la página 1 → sin cambio; **CLE** de teclas 16–18 (`0x08007604..0x0800763E`)
sigue limpiando sus 85×85 originales (`CLE ff` limpia todo; el host usa `CLE ff`); **LED de pulsación** (`0x08007188`, tabla RAM
`0x20000014`) solo cubre las teclas 1..15 y no usa los datos de las celdas laterales; el parámetro `h` (`str r3,[sp]`) no se lee en
`0x08005810`; `0x08007D28` usa un búfer de 1440 B: w ≤ 480 seguro. Compatibilidad: BAT/STP/CLE y teclas 1..15 sin cambios
(`bat_evidence.py`: página 1 idéntica byte a byte tras 15 BAT de 85×85). Un host antiguo que mande 80×80 a las teclas 16–18 verá
celdas deformadas (basura en las columnas que sobran): solo usar con orcadeck.

**Ensanchar teclas 1..15 por encima de 85 (no necesario ahora, solo diseño):** cada una de las 15 `movs r3,#0x55`
(`0x08006D44, 56, 66, 74, 82, 90, 9C, AC, BC, CA, DA, EA, F8, 0x08006E08, 18`; byte de inmediato = dirección+0) puede
subirse hasta `#0xFF`; `w` real limitada por el heap (≈384) y por `x+w ≤ 480` (en x=347 como mucho 133); w mayor que el JPEG lee
basura. Efecto lateral: el marco amarillo `0x08007188` (`x+0x55`, 15 `add.w rX,r0,#0x55`) quedaría dentro del tile y su OFF
(negro) dejaría una cicatriz. Aprovechamiento vertical (relleno de huecos de 63 px) posible hasta `w=148` salvo x=347.

## B2. Imagen y evidencia

| Imagen | Tamaño | SHA-256 | CRC-16/ARC |
|---|---|---|---|
| **`work/AKP153E_sidestrip_NO_PROBADO.bin`** (parche S) | 96 496 | **`411ed0a1174f425f0c17558bea8842cccaa6b541d364845752e51eeba28a90d3`** | `0x4EF9` |

Evidencia (`bat_evidence.py`, exit 0): BAT de JPEG 198×100 / 149×100 / 133×100 (rojo/verde/azul) a las teclas 16/17/18:
- parche: la página 1 cambia exactamente en `(0,0)-(479,99)` y, tras `STP`, la pantalla muestra los 3 segmentos contiguos (0 píxeles fuera
  de color en 3 filas × 480 px; la fila 100 no cambia);
- original, mismos JPEG: bbox `(49,12)-(426,111)`, hueco `x=130..197` sin pintar (no continuo);
- teclas 1..15 con BAT 85×85: página 1 idéntica byte a byte entre original y parche;
- 7 bytes distintos, `0x08006E26..0x08006E48`, 96 496 B. Sin accesos fuera de memoria de vídeo.

Los parches S y press/release v3 son **independientes** (zonas disjuntas: `0x08006E2x` y `0x08006F92..0x0801ABEF`).

## B3. Riesgos de flasheo y go/no-go (parche S)

Riesgos: bootloader fuera del `.bin` (UNKNOWN), sin SWD verificado, soft-brick por `APP/NEW` interrumpido (la unidad queda en
`EEEF:2929` y se reintenta; **no probado con imagen mala**), comportamiento LCD real sin validar (el modelo RA8876 es INFERRED).
El parche S solo toca inmediatos de la tabla BAT: el riesgo funcional es bajo (peor caso: celdas laterales mal colocadas; teclas y
USB no se tocan), pero el riesgo de la operación de flasheo es el de siempre.
- [ ] SHA-256 = `411ed0a1…8a90d3`, origen = `15e03f5b…f54f6f`; restauración `backup/AKP153E.bin` a mano.
- [ ] `EEEF:2929` alcanzado y reconocido antes; no flashear v3 y S a la vez la primera vez (probar S solo, o v3 solo).
- [ ] Host (orcadeck) capaz de enviar JPEG de 198/149/133 px a las teclas 16/17/18 y de **nunca** enviar un JPEG > 384 px (ni 480).
- [ ] Antes de flashear, probar en hardware sin firmware nuevo: `BAT` de 300 px y de 480 px a una tecla (la emulación predice que 480 px no dibuja).
- NO-GO si hash distinto o si falla cualquier comprobación de `run_all.py`.

## Preguntas abiertas

1. Cuelgue de DRA 0xB1 / DRA 3 + BAT (§A1): sin causa; se necesita una prueba de hardware con DRA 3 de 160 px.
2. Heap real del dispositivo = 29 948 B (OBSERVED en el código; el valor concreto de ancho máximo ≈384/337 sale de emulación): confirmar en hardware con BAT de 300/380/400 px.
3. El resto de INFERRED/UNKNOWN de `hid/lcd-init-rea.md` §5.
