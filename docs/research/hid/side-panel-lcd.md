# AKP153E: geometría del LCD, DRA y píxeles sin usar (REA + pruebas, 2026-11-13)

Firmware: `docs/research/firmware/backup/AKP153E.bin` (`V2.293S.00.003`, base `0x08004000`, Cortex-M4). Análisis
estático con capstone (REA no abre ARM raw en Windows). **OBSERVED** = leído en el desensamblado o visto en el
dispositivo. **INFERRED** = deducción.

## 1. El LCD es 480 × 854 en vertical (OBSERVED)

El firmware dibuja con `0x08005810(jpeg, x, y, w, h)`, y los números `0x1E0` (480) y `0x356` (854) aparecen en la
preparación (`0x0800550A`, `0x08006CD4`). Un fotograma completo son 480·854·3 = `0x12C3C0` bytes (el valor que escribe
`BAT` en los registros `0x50..0x53` del LCD).

Tabla de `BAT` (`0x08006CD4`, salto `tbb` en `0x08006D2A`; volcado con un mini-simulador). Coordenadas del panel,
x de 0 a 479 y y de 0 a 853. La orientación del dispositivo es el panel girado 90° (`dev(u,v) = panel(x=v, y=853-u)`,
igual que el giro 270° CW que ya aplica el host):

| Teclas | x | y | tamaño |
|---|---|---|---|
| 1–3, 4–6, 7–9, 10–12, 13–15 (columna del dispositivo 4 → 0) | 49 / 198 / 347 | 146 / 294 / 442 / 590 / 738 | 85×85 |
| 16, 17, 18 (celdas laterales) | 49 / 198 / 347 | 12 | 80×80 |

Paso entre teclas: 148–149 px. Hueco sin dibujar entre tiles: 63 px (teclas) y 69 px (celdas laterales). Ninguna
tecla ≥ 19 dibuja (`cmp ip,#0x13; bhs`).

## 2. `0x08005810` (dibujo JPEG): comportamiento útil (OBSERVED)

- Copia `len` bytes a `0x20000BA9`, decodifica con libjpeg 9a (baseline) y **dibuja fila a fila** con
  `0x080086A0(x, y+fila, w, 1, fila, w*3)`.
- **Ancho: se recorta a `w`** de la tabla (solo se leen `w` píxeles por fila). **Alto: lo manda el JPEG**, no `h`
  (el parámetro `h` no se usa; el bucle va hasta `output_height`).
- `0x08007D28` usa un búfer de pila de `0x5A0` = 1440 B → `w ≤ 480`. No hay recorte contra los bordes del panel.

## 3. Comandos del firmware que la app oficial no usa en el AKP153E (OBSERVED)

Analizador `0x08006618` (cabecera `CRT\0\0` + verbo en `[5..7]`), receptor USB `0x0800983C`:

| Verbo | Estructura | Efecto |
|---|---|---|
| `DRA` | `[9..0xB]` = longitud (24 bit), `[0xC]` = clave, `[0xD..E]` w, `[0xF..0x10]` h, `[0x11..12]` x, `[0x13..14]` y (BE16) | clave `0xBn` → estado 4; otra → estado 3. Tras recibir `ceil(len/512)` paquetes de 512 B llama `0x08005564` (estado 4) o `0x0800550A` (estado 3) y dibuja con `0x08005810(x,y,w)` |
| `LOG` | clave `[0xB]` | escribe el logo en **flash** en `(clave*3-2)<<17` (sector de 128 KB) |

Estado 3 reinicializa el LCD (`0x20..0x29`, `0x50..0x5D` a ventana completa y longitud 0); estado 4 solo pone la
longitud `0x258780` (= 2× fotograma) y el ancho 480. `BAT` usa `0x12C3C0`. INFERRED: esos registros son los que rompen
el dibujo.

**Límite de tamaño:** los datos van a `0x20014C38`, 0x14000 B (80 KB) antes de `0x20028C38` (tabla de teclas, datos de
`DRA`, API de flash). El bucle de recepción **no comprueba el tamaño** → una imagen > ~80 KB corrompe esa RAM
(INFERRED: explica «se rompía el firmware» con imágenes completas). `BAT` además solo admite 16 bit (64 KB).

## 4. Pruebas en el dispositivo (OBSERVED, usuario mirando)

Subcomandos nuevos de investigación: `orcadeck dra <jpeg> <x> <y> <w> <h> [key] [stp]` y `orcadeck bat <jpeg> <key>`.
JPEG baseline 4:4:4, tiras de 480×107, 2–15 KB (muy por debajo del límite).

| Prueba | Resultado |
|---|---|
| `DRA` estado 3 (key 0) | pantalla en negro; se recupera relanzando OrcaDeck (el `CLE ff` reinicia) |
| `DRA` estado 3 + `STP` | igual, negro |
| `DRA` estado 4 (key 0xB1) desde negro | sigue en negro |
| `DRA` estado 4 desde pantalla sana | **cuelga el firmware** (no responde); recupera al relanzar |
| `DRA` estado 3, 1 s después `BAT` a la tecla 8 | **cuelga** (se queda en lo último mostrado); recupera al relanzar |
| `BAT` tecla 8 con JPEG de 85 de ancho × 148 de alto (en panel) | **funciona**: las 63 filas extra pintan fuera del tile, hacia la izquierda del dispositivo, sobre el hueco y parte de la tecla vecina |
| `BAT` tecla 17 con JPEG de 148 de ancho × 80 de alto (en panel) | **recortado a 80** de ancho: no se pinta nada en vertical fuera de la celda |

**Conclusión:** `DRA` no es utilizable en esta unidad (los dos estados la dejan negra o colgada). Queda descartado salvo
que se descifre la secuencia del controlador LCD. `LOG` sigue descartado (flash, sin recuperación).

## 5. Píxeles sin usar en las teclas (OBSERVED + INFERRED)

- La pantalla es una sola detrás de las 15 teclas (el brillo cambia en todas a la vez; el lateral tiene brillo
  distinto: pantalla o controlador propio, INFERRED).
- Detrás de cada tecla hay mucha más pantalla visible que 85×85 (el usuario la ve moviendo la cabeza; el verde del
  hueco se ve «un poquito» en la tecla vecina).
- **Aprovechable hoy por `BAT`:** cualquier alto en panel = horizontal en el dispositivo, **hacia la izquierda** desde
  el borde derecho del tile. Un JPEG de 85 × (85 + N) en panel pinta N columnas extra de dispositivo (hueco máximo 63
  entre teclas; más allá pisaría la tecla vecina).
- **No aprovechable:** la dimensión vertical del dispositivo (ancho en panel) está recortada a 85 / 80. Por eso las
  celdas laterales (apiladas en vertical) no se pueden unir con `BAT`: el hueco entre ellas está en la dimensión
  recortada.

### 5.1 Medida por teclas 7, 8, 9 (OBSERVED, usuario)

JPEG de 85 × 385 en panel (tile + regla de 300 px a la izquierda del dispositivo, línea blanca cada 100). Resultado
(estimación a ojo del usuario): la anchura **útil** en horizontal es de unos **95–98 px** por tecla (≈ la distancia entre
dos líneas blancas), que coincide con el límite visible por detrás de la tecla. Hoy se usan 85: se ganan ~10–13 px.
Vertical sin medir (recortado a 85 / 80 por `w`). Sin fallos ni cuelgues con 385 filas.

### 5.2 Vertical en teclas 7, 8, 9 (OBSERVED, usuario)

JPEG de 95 (horizontal) × 120 (vertical) en dispositivo: **recortado a 85 en vertical**, igual que en las celdas
laterales. Confirma que el recorte vertical es de `w` de la tabla de `BAT` y vale para las 18 teclas.

### 5.3 `MOD` 0x31 + `DRA` 0xB4 (OBSERVED, usuario, 2026-11-13)

`orcadeck mod 49` + `orcadeck dra <tira> 0 <y> 480 <h> 180`. **No cuelga** (a diferencia de 0xB1 y del estado 3), pero la
primera tira dejó **ruido blanco y de colores** en pantalla y, con las 8 tiras de la rejilla completa, el **ruido
sigue igual**: lo dibujado no llega a la página frontal, o la copia lee otra zona. Al pulsar una tecla se enciende en
amarillo y el hueco queda «fantasma» (BAT sigue su flujo normal). Recuperación: `mod 50` (0x32) y relanzar OrcaDeck.
Lectura (INFERRED): la copia página 2 → 0 se ejecuta (el ruido es página 2 sin inicializar), pero el dibujo no cae en
página 2 (revisar el significado real de los regs `0x50..0x53` y de la posición de escritura `0x5F..0x62`).

### 5.4 `DRA` estado 3 FUNCIONA con JPEG ≤ 384 px de ancho (OBSERVED, usuario, 2026-11-13)

Causa de los negros/ruidos anteriores (worker, `docs/research/firmware/lcd-emulation.md`, INFERRED, consistente con las
pruebas): el heap estático de libjpeg del firmware (29 948 B) no decodifica JPEG de **480 px** de ancho («Insufficient
memory»; máximo ~384 px en 4:4:4). Con anchos menores `DRA` estado 3 (key 0, `[0xC]=0`) dibuja **en vivo en la página
frontal**, sin `BAT` ni `STP`:

| Prueba | Resultado |
|---|---|
| `dra g0a.jpg 0 0 240 107 0` (solo la primera mitad) | media rejilla en el lateral; el aparato sigue vivo (LED local) |
| 2.ª mitad `dra g0b.jpg 240 0 240 107 0` | **rejilla completa**: franja lateral continua entre celdas |
| 2 mitades seguidas con 0.6 s de pausa | la 2.ª no se dibujó (INFERRED: decodificación aún en curso; usar ≥1 s o esperar) |
| `dra v8.jpg 198 442 120 95 0` (tecla 8, 120 vertical × 95 horizontal) | **se ve el cian fuera de los 85 px verticales**: no hay recorte con `DRA` |

Consecuencia: **no hace falta parchear el firmware para dibujar** (panel lateral continuo, teclas más grandes en las dos
dimensiones). `BAT`+`STP` siguen existiendo, pero `STP` copia toda la página trasera y borra lo pintado en vivo; el camino
útil es dibujar todo con `DRA` estado 3 (teclas incluidas). Pendiente: ritmo/pausa fiable, coste por fotograma, que `CLE`
y el resto de comandos convivan, y el parche de pulsar/soltar (prioridad 1).

### 5.5 Ritmo y tamaño de tile con `DRA` 3 (OBSERVED, usuario, 2026-11-13)

- **Ritmo:** 3 `DRA` (98×98) con 0,2 s de pausa entre procesos: los 3 se dibujan. 15 `DRA` (98×98, 4:4:4, ~1–2 KB) con ~50 ms
  entre procesos `orcadeck dra`: los 15 se dibujan, sin fallos ni cuelgues. (La pérdida de la 2.ª mitad con 0,6 s de §5.4 no
  se repitió: queda INFERRED que dependía del tamaño del JPEG o de que el proceso se reabría; hay que seguir midiendo.)
- **Centrado:** el rectángulo que se dibuja con `x0=x_BAT−(S−85)/2`, `y0=y_BAT−(S−85)/2` (S=98 → −6,5) queda **algo
  desplazado**; con y0=y_BAT (sin restar) se vio desplazado hacia la izquierda del dispositivo.
- **Tamaño 98×98:** con borde blanco de 1 px y diagonales se ve **recortado por los cuatro lados**, y depende de la tecla
  (perimetrales distinto de centrales). Con anillos cada 2 px la lectura es «depende de la tecla y demasiado pequeño».
  El tamaño y el offset útiles son **por tecla**, no globales.
- **Decisión del usuario:** calibrar tecla a tecla en el editor: pulsar físicamente la tecla, mover el cuadro en tiempo real
  con las flechas (teclado o en pantalla) y guardar el desplazamiento y el tamaño.

## 6. Qué queda abierto

1. Recorte en la **dirección horizontal del dispositivo**: probar hasta qué fila extra dibuja de forma fiable y si el
   orden de envío (tecla vecina después) permite solapar de forma controlada (el último en llegar gana).
2. Panel lateral continuo en horizontal: se pueden ensanchar las 3 celdas hacia la izquierda (a más de 80 px), pero no
   unirlas en vertical. Sin `DRA` ni `LOG` no hay panel continuo.
3. Para arreglar `DRA`: reconstruir la inicialización del LCD (registros `0x20..0x5D` vía `0x08007878`, índice/dato por
   `0x080076C4`/`0x080076CC`) y compararla con lo que hace `CLE ff` (`0x08006480`). Es trabajo de emulación (ver
   `docs/research/firmware/work/emu`), no de pruebas ciegas: cada fallo cuelga el firmware.
