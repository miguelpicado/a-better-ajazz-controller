# Auditoría de rendimiento: `orcadeck run` y `orcadeck edit`

- **Fecha:** 2026-11-10
- **Repositorio:** `%USERPROFILE%\orca\workspaces\orca-stream-dock\perf-audit`
- **Rama:** `miguelpicado/perf-audit`
- **Alcance:** auditoría de solo lectura. No se modificó código, `Cargo.toml` ni configuración; el único fichero creado es este informe.
- **Herramienta / entorno:** Rust `1.99.0` (`gnullvm-local`, `x86_64-pc-windows-gnullvm`), `llvm-mingw-20261104-ucrt-x86_64`, Windows, PowerShell. Builds con `CARGO_TARGET_DIR` propio en `%TEMP%\opencode\ocdeck-perf\*` y perfil release alterado solo por variables `CARGO_PROFILE_RELEASE_*`.
- **Objetivo rector:** app extremadamente ligera y optimizada, todo Rust, 0 % CPU en reposo (hilos bloqueantes, sin polling), memoria y tráfico HID mínimos, sin dependencias nuevas salvo necesidad demostrada, UI web bajo demanda y nunca residente. **Medir antes de dar una optimización por buena.**
- **Restricciones respetadas:** no se ejecutó `run`, `probe`, `watch`, `poll` ni `hidwatch`; no se abrió ni escribió en el dispositivo; no se flasheó nada; no se instalaron dependencias; no se dejaron procesos en segundo plano.

## Cómo leer las cifras

- **MEDIDA**: tomada con un instrumento (tamaño de fichero, `Get-Process`, cronómetro del propio binario, sonda compilada contra el código real).
- **ESTIMADA**: calculada o modelada, no cronometrada directamente. Indico si es *cálculo* (aritmética exacta sobre constantes del código o medidas) o *modelo* (supone una hipótesis).
- Las cifras de CPU son **% de un núcleo lógico** salvo que digan "sistema". El equipo tiene 16 núcleos lógicos.
- El AKP153E pierde pulsaciones si se pinta casi sin pausa. Todo pintado pasa por el canal único `(key, image)` y se agrupa en lotes separados por `ORCADECK_GAP_MS` (400 ms por defecto) — `daemon/src/app.rs:36-40`, `app.rs:1098-1115`. Ninguna propuesta de este informe lo rompe.

---

## 1. Perfil release y tamaño del binario

### 1.1 Perfil actual leído

`daemon/Cargo.toml:28-33`:

```toml
[profile.release]
opt-level = "s"
lto = true          # fat
codegen-units = 1
panic = "abort"
strip = true
```

No hay clave `debug`: vale `false` (sin información de depuración solicitada) ni `incremental`. `rust-toolchain.toml` fija `stable-x86_64-pc-windows-gnullvm` (1.99.0); no se tocó.

### 1.2 Tamaños medidos

Comando base (desde `daemon/`, con el `PATH`/`RUSTUP_*` del encargo):

```powershell
$env:CARGO_TARGET_DIR="$env:TEMP\ocdeck-perf\base"
cargo build --release -p orcadeck --bin orcadeck
(Get-Item "$env:CARGO_TARGET_DIR\release\orcadeck.exe").Length
```

Cada variante usó un `CARGO_TARGET_DIR` distinto y una variable de entorno, sin editar `Cargo.toml`:

| Variante | Variable usada | `orcadeck.exe` | Δ vs actual | Tipo |
|---|---|---:|---:|---|
| **Actual** (`s`, fat LTO, cgu 1, abort, strip) | — | **3 681 792 B** (3.51 MiB) | — | MEDIDA |
| `strip = false` | `CARGO_PROFILE_RELEASE_STRIP=false` | 7 134 720 B (6.80 MiB) | **+3 452 928 B (+93.8 %)** | MEDIDA |
| `panic = "unwind"` | `CARGO_PROFILE_RELEASE_PANIC=unwind` | 3 900 416 B (3.72 MiB) | +218 624 B (+5.9 %) | MEDIDA |
| `codegen-units = 16` | `..._CODEGEN_UNITS=16` | 3 718 144 B (3.55 MiB) | +36 352 B (+1.0 %) | MEDIDA |
| `opt-level = "z"` | `..._OPT_LEVEL=z` | 3 215 360 B (3.07 MiB) | **−466 432 B (−12.7 %)** | MEDIDA |
| `opt-level = 3` | `..._OPT_LEVEL=3` | 4 417 536 B (4.21 MiB) | +735 744 B (+20.0 %) | MEDIDA |
| `lto = "thin"` | `..._LTO=thin` | 4 278 272 B (4.08 MiB) | +596 480 B (+16.2 %) | MEDIDA |
| `lto = false` | `..._LTO=false` | 4 472 320 B (4.27 MiB) | +790 528 B (+21.5 %) | MEDIDA |

El tamaño actual coincide **exactamente** con el que ya documentaba `docs/project/STATUS.md:22` (3 681 792 B): la cifra estaba bien y se confirma de forma independiente (MEDIDA).

Secciones del binario actual (`llvm-objdump -h`, MEDIDA): `.text` 3 020 166 B, `.rdata` 596 488 B, `.data` 3 072 B, `.pdata` 46 800 B, `.tls` 168 B, `.reloc` 10 760 B. Con `strip=false` aparecen `.debug_info` 676 392 B, `.debug_str` 1 228 853 B, `.debug_ranges` 218 416 B, `.debug_line` 232 433 B, etc.: los 3.45 MB extra son DWARF/símbolos, no código.

### 1.3 Coste en velocidad de las variantes de tamaño

`orcadeck render` sobre la escena real `R0KZ54Z0-…sdProfile` (17 celdas, sin dispositivo), 5 ejecuciones en caliente (MEDIDA):

| Binario | Mediana de `render` | Δ vs actual |
|---|---:|---:|
| Actual (`opt-level="s"`) | **41.4 ms** | — |
| `opt-level="z"` | 57.9 ms | **+40 %** |
| `opt-level=3` | 40.5 ms | −2 % (dentro del ruido) |

### 1.4 Qué cambia de comportamiento

- **`strip=false`**: conserva nombres y DWARF; los backtraces y los volcados vuelven a ser legibles. No cambia la ejecución. Ahorro de `strip=true`: 3.45 MB (48 % del exe). Mantener `strip=true`; si algún día se quiere depurar en producción, `debug = 1` + `strip = "debuginfo"` es la vía, no `strip=false`.
- **`panic="abort"`** (ya activo): un `panic!` en **cualquier** hilo termina el proceso entero; no hay `unwind`, no corren los `Drop` y no se puede aislar el fallo de un hilo. Hoy no hay `catch_unwind` en el árbol, pero con `unwind` un pánico en el hilo lector de un plugin solo mataba ese hilo; con `abort` tumba el daemon (que no tiene supervisor propio). Las rutas con `unwrap()`/indexado alcanzables desde datos de plugin existen (`crates/builtin/src/orca.rs:37,91`, `crates/host/src/host.rs:641`) → es un cambio real de robustez, no solo de tamaño. En tests no afecta: Cargo ignora `panic` en el perfil `test` y los tests siguen compilando con unwinding. Ahorro: 218 624 B (5.9 %).
- **`codegen-units=1`** (ya activo): +1 % de tamaño frente a 16 a cambio de mejor código y compilación más lenta. Correcto para este proyecto.
- **`lto=true` (fat)** (ya activo): ahorra 16.2 % frente a `thin` y 21.5 % frente a `false`. Correcto.
- **`opt-level="s"` vs `3`**: ahorra 20 % de tamaño y el pipeline de imagen va igual (−2 %, ruido). Correcto.
- **`opt-level="z"`**: ahorra 466 KB (−12.7 %) pero **cuesta +40 % en el pipeline de imagen** (resize + JPEG + resvg). No compensa para una app que repinta teclas; se descarta.

**Conclusión §1:** el perfil actual está bien elegido. La única palanca grande que queda es quitar código (editor/plugman, §5), no cambiar flags.

---

## 2. Despertares periódicos (el punto crítico)

### 2.1 Inventario de esperas con timeout, `sleep` o `recv_timeout`

| Hilo | Fuente | Frecuencia | Por qué existe | Diseño propuesto |
|---|---|---|---:|---|
| **device (main)** | `app.rs:1053` `read_timeout(&mut buf, 30)` | **33.3 /s** siempre | Leer entrada HID y, de paso, ejecutar deadlines (restore 600 ms, retire 2 s, debounce settings 1 s, heartbeat 10 s) y vaciar el canal de pintado | Hilo lector que hace `read()` **bloqueante** y empuja `Event::Input` a un canal único; el hilo escritor/device espera con `recv_timeout(next_deadline)` o `recv()` si no hay plazo. Un solo escritor HID, un solo `STP` por lote, `ORCADECK_GAP_MS` calculado como `last_batch + gap`. |
| **builtin** | `app.rs:960` `recv_timeout(400)` | 2.5 /s | Refrescar reloj/spinner/agentes/cuota y recibir pulsaciones | `recv_timeout(min(next_deadline))` con plazos reales: fin de minuto si hay reloj, `SPIN_MS` solo si hay un agente `working` visible, `POLL_MS` si hay claves de agente/cuota, fin de blink; si no hay ninguno, `recv()` bloqueante. |
| **device (reconexión)** | `app.rs:1018` `sleep(2 s)` | 0.5 /s **solo desconectado** | Reintentar abrir el HID tras error | Dejarlo (solo corre sin dispositivo) o esperar `WM_DEVICECHANGE`/hotplug de hidapi. No afecta al reposo con dispositivo. |
| **ctl** | `ctl.rs:160-176` `ConnectNamedPipe` bloqueante | 0 /s | Aceptar `orcadeck edit` | Correcto: no tocar. |
| **host-accept / readers** | `host.rs:260-266`, `host.rs:521-545` sockets bloqueantes | 0 /s | Aceptar plugins y leer WebSocket | Correcto: no tocar. |
| **host supervisor** | `host.rs:368-408` `Child::wait` bloqueante; `sleep` solo en backoff de crash | 0 /s en reposo | Reaprovechar/rearrancar plugins | Correcto. El `sleep` de backoff solo ocurre tras un crash. |
| **web (solo `edit`)** | `web/lib.rs:85-93` `listener.incoming()` bloqueante; `126-134` timeouts de 10 s **por petición** | 0 /s en reposo | Servir el editor bajo demanda | Correcto: el servidor solo existe mientras el cliente `edit` está conectado (`ctl` cierra el pipe). |
| **`edit` CLI** | `edit.rs:137-143` `thread::park()` | 0 /s | Esperar Ctrl+C sin polling | Correcto. |
| **plugman watcher** | `plugman/lib.rs:761-771` `sleep(500 ms)` + `poll()` | 500 ms | Instalar paquetes del inbox | **No lo usa `run` ni `edit`**: solo el CLI `orcadeck-plugman watch`. Si algún día se conecta al daemon, usar `ReadDirectoryChangesW`, no polling. |
| **RPC a Orca** | `builtin/orca.rs:57-79` hilo por llamada + `recv_timeout(5 s)` | 0.83 polls/s × 2 llamadas + 0.1/s workerList | Refrescar agentes/cuota | Un hilo RPC persistente con canal de peticiones (o pipe solapado); y bajar la cadencia (§8). |

**Total en reposo con la escena Orca:** 33.3 + 2.5 wakes/s periódicos ≈ **35.8 /s**, más ~1.8 hilos nuevos/s por las llamadas RPC (no son wakes de timer, pero sí trabajo de kernel).

### 2.2 ¿Cuánto cuesta un despertar?

- **MEDIDA (suelo del SO):** un programa C mínimo que hace 300 × `WaitForSingleObject(evento, 30 ms)` consume **< 0.001 ms de CPU en total** (por debajo de la resolución de `GetProcessTimes`; < 3 µs/despertar). El temporizador de 30 ms en sí no gasta CPU medible.
- **MEDIDA (trabajo por vuelta del bucle real, sonda contra el código):** `registry.tick` con 13 claves y un `Orca` simulado: **p50 7–15 µs**, p95 7–16 µs, máx. 60 µs. Por tanto el coste del tick de builtin es despreciable.
- **ESTIMADA (cálculo):** el cuerpo del bucle de `app.rs:1082-1119` (2 `Instant::now`, `rt.tick` con 2 locks, `retire_due` con 1 lock, 2 `try_recv`, comparación de heartbeat) está en el mismo orden, ~2–10 µs por vuelta → 33.3 × 10 µs ≈ **0.03 % de un núcleo como máximo**.

**Conclusión honesta:** el bucle de 30 ms **no es un problema de CPU medible** (0.00–0.03 %); es un problema de *diseño* (impide dormir de verdad, añade hasta 30 ms de latencia a cada pulsación y mantiene 33 despertares/s de batería). El verdadero coste de CPU en reposo está en el polling a Orca (§2.3), no en el `read_timeout`.

### 2.3 El coste real: polling a Orca desde builtin

`Registry::tick` se ejecuta en cada vuelta de 400 ms (`app.rs:954`), pero `poll_agents` solo refresca cada `POLL_MS = 1000 ms` (`actions.rs:9,105-114`). Con ticks a 0/400/800/1200 ms, el refresh efectivo es **cada 1200 ms (0.83/s)**. Cada refresh hace, como mínimo:

- `worktree.ps` — **MEDIDA: p50 15.2–22.3 ms** (dos tandas; máx. 260 ms bajo carga).
- `terminal.list` — **MEDIDA: p50 3.7–4.8 ms**.
- cada 10 s: `orchestration.workerList` — **MEDIDA: p50 160–278 ms** (el comentario del código decía ~450 ms; hoy es menos, pero sigue siendo el peor).
- cada 10 s: `orchestration.runList` — **MEDIDA: p50 3.1 ms**.
- cada 30 s: `repo.list` — no medido (estimado < 5 ms).

**ESTIMADA (cálculo con las medidas):** (15.2+3.7) ms / 1.2 s ≈ **15.8 ms/s ≈ 1.6 % de un núcleo** (rango 1.6–2.3 % con la otra tanda) en el proceso de Orca, más (160+3.1)/10 s ≈ **16.3 ms/s ≈ 1.6 %**, total ≈ **3.2 % de un núcleo de forma continua** mientras haya una clave `agent`/`summary` visible. En la escena `Orca` del usuario hay 5 claves de agente + summary → siempre. Además el daemon crea ~1.8 hilos/s (`orca.rs:60`) y abre el pipe 2 veces/s.

Esto sí incumple el objetivo de “0 % CPU en reposo”, aunque el consumo esté en Orca y no en `orcadeck`. Es el hallazgo P0 de la auditoría.

### 2.4 Riesgos del rediseño propuesto

- **Reconexión HID:** el hilo lector debe terminar y cerrarse con la sesión; si el dispositivo se desenchufa, `read()` falla y hay que enviar `Disconnected` al escritor. Nunca dejar el lector vivo con un handle cerrado.
- **Dos handles vs uno:** leer y escribir desde dos hilos sobre el **mismo** `HidDevice` no está documentado como thread-safe por hidapi; lo conservador es abrir un segundo handle solo-lectura (mismo VID/PID) y mantener el escritor actual. Validar en dispositivo.
- **Pulsaciones perdidas:** el lector bloqueante **mejora** la latencia (elimina el muestreo de 30 ms) y sigue vaciando el buffer HID mientras el escritor pinta; el canal debe ser ilimitado para que el lector nunca se bloquee. Mantener el lote único y el `STP` único.
- **Handshake:** hoy el `HANLDER` se responde desde el bucle principal; con lector separado, el lector debe reenviar el evento al escritor (nunca escribir él).
- **`ORCADECK_GAP_MS`:** el plazo del próximo lote es `last_batch + gap`; si no hay nada pendiente, no debe haber despertar. No reducir el gap.

---

## 3. Memoria y copias: de la imagen del plugin al HID

### 3.1 Recorrido y copias por tecla

Entrada típica: `setImage` con `data:image/png;base64,…` o un SVG data-URI (`app.rs:68-81`). Por cada tecla, en el caso 144×144 RGBA:

| Etapa | Qué ocurre | Bytes/copia | Copias |
|---|---|---:|---|
| 1. WebSocket → `Value` | tungstenite entrega texto; `serde_json` lo parsea (`host.rs:601-611`) | ~4 KB (base64) | 1 texto + 1 `String` en el `Value` |
| 2. `Output::Image` | se clona el `String` de la imagen (`host.rs:107,659-740`) | ~4 KB | +1 |
| 3. `decode_uri` | base64 decode a `Vec<u8>` (`app.rs:69-75`) | ~3 KB | +1 |
| 4. `image::load_from_memory` | PNG/JPEG → `DynamicImage` 144×144 | 82 944 B (RGBA) / 62 208 B (RGB) | 1 buffer persistente |
| 5. `view.base.insert` | se guarda el original (`app.rs:347`) | 82 944 B | persistente |
| 6. `effective()` | **clona** la base para componer (`app.rs:174-181`) | 82 944 B | +1 |
| 7. `blend` (si hay título) | `to_rgba8` + `overlay` (`app.rs:128-136`) | 82 944 B | +1 (y la base clonada sigue viva) |
| 8. canal `Paint` | `send` mueve la imagen (`app.rs:261-263`) | 0 | move |
| 9. `latest.insert(p.img.clone())` | **clona otra vez** para el repintado tras reconexión (`app.rs:1095`) | 82 944 B | +1 persistente **duplicada** |
| 10. `key_jpeg` | `resize_exact(85)` RGBA + `to_rgb8` + `rotate270` (`render.rs:5-8`) | 28 900 + 21 675 + 21 675 B | 3 buffers temporales |
| 11. JPEG | q75, baja de 4 si > 9 000 B | 1 685–9 000 B | 1 |
| 12. `image_packets` + `wire` | cada chunk se copia a un `Vec` de N bytes y luego a otro de N+1 (`proto.rs:53-69`) | 2.5–9 KB por paquete | 2 copias por paquete |

**Pico transitorio por tecla (144², con título):** ~4+4+3+83+83+83+83+29+22+22+9+9 ≈ **440 KB**.
**Pico con imágenes 288² (retina, habituales en plugins):** las copias de imagen se ×4 → **~1.4 MB por repintado**.
**Memoria persistente por tecla:** `view.base` + `latest` = 2 × 83 KB (144²) o 2 × 332 KB (288²). Para 15 teclas: **2.5 MB (144²) / 10 MB (288²)**, más la caché de títulos (15 × 28.9 KB = 0.43 MB).

### 3.2 Medidas del pipeline

Sonda compilada contra el código real de `key_jpeg` (misma secuencia `resize_exact(85, Lanczos3) → to_rgb8 → rotate270 → JPEG q75`), 30 repeticiones (MEDIDA):

| Imagen de entrada | Tiempo por tecla | Bytes JPEG |
|---|---:|---:|
| UI plana 144×144 | **822 µs** | 1 685 B |
| Ruido 144×144 | 818 µs | 5 518 B |
| UI plana 288×288 | **1 624 µs** | 1 685 B |
| Ruido 288×288 | 1 974 µs | 5 855 B |

Rasterizado resvg de un SVG de agente a 85 px: **82.7–128.8 µs** (MEDIDA, dos tandas de la sonda).

`orcadeck render` de la escena real (17 celdas, incluye decodificar PNG de disco): **41.4 ms** en caliente, **6.8 MB de pico de working set** (MEDIDA). Salida: 34 962 B de JPEG para 17 celdas.

### 3.3 Propuestas concretas

1. **Quitar el mapa `latest`** y repintar tras reconexión desde `view.effective(key)` (la vista ya tiene la imagen base y el título). Ahorro: **1.2–2.5 MB (144²) / 5–10 MB (288²)** de RSS, más un clon de 83–332 KB por repintado. Ficheros: `app.rs` (`session`, `switch_scene`, `paint`). Riesgo: el repintado tras re-plug debe componer el título igual que antes; validar con dispositivo.
2. **Pre-escalar al ingerir** a `image_px(key)` (85/80): guardar ya en 85×85 y saltar el `resize_exact` de `key_jpeg` cuando ya está a esa resolución. Ahorro: **0.7–1.8 ms por tecla** (≈70 % de `key_jpeg`), un lote de 15 teclas pasa de **12–30 ms a 3–5 ms** de CPU; y con (1), la memoria persistente baja a **~0.9 MB** para 15 teclas. La composición de título/feedback ya trabaja a 85 (`blend` redimensiona la base a `px`), así que la salida es la misma. Ficheros: `app.rs`, `render.rs`. Riesgo: verificación visual; el filtro debe ser el mismo Lanczos3 para no cambiar el aspecto.
3. **Un solo mapa de imagen**: si (1) y (2) se hacen, `view.base` es la única copia; `paint` deja de clonar (`app.rs:396`) y el canal lleva `(key, gen)` en vez de la imagen, componiendo en el hilo de device. Ahorro adicional: un clon de 83–332 KB por paint de builtin (2.5/s por tecla animada).
4. **Cadenas**: `Output::Image` clona el `String` base64; se puede mover con `std::mem::take` si `command` recibe `&mut Value`. Ahorro ~4 KB y una alloc por imagen. Menor.
5. **No tocar el JPEG q75**: 1.7–5.5 KB medidos, muy por debajo de `MAX_JPEG=9000`; solo las fotos ruidosas reintentan con q−4. Correcto tal cual.

---

## 4. Procesos de plugins (Node)

### 4.1 Coste medido por proceso

| Proceso | Working set | Privado | CPU en reposo | Hilos | Tipo |
|---|---:|---:|---:|---:|---|
| `node` vacío (`setTimeout`) | 57.6 MB | 47.7 MB | 0.00 % | 17 | MEDIDA |
| Plugin legacy `com.miguelpicado.orcadock` (`plugin.js`, 4 acciones) | **56.3 MB** | 47.0 MB | **0.00 %** | 17 | MEDIDA |
| Plugin `com.insta360.webcam` (`bin/plugin.js`) | **63.8 MB** | 21.6 MB | no válido* | 12 | MEDIDA (solo RSS) |

\* El plugin de insta360 se midió con el puerto WebSocket cerrado y gasta 2.8 % de CPU reintentando; ese número **no** representa el reposo con host. Su RSS sí es válido.

Por plugin hay además **2 hilos del daemon** (lector de socket + waiter de proceso, `host.rs:260-266,355-363`), con stack reservado por defecto (2 MB virtuales, no residentes).

### 4.2 Política actual (ya implementada)

- Solo se lanzan los plugins que la escena usa: `launch_plugins` recibe `placed.used` (`app.rs:1000`, `684-707`).
- Un plugin que la escena nueva deja de usar se retira **2 s** después (`RETIRE_MS=2000`, `app.rs:86-88`), con cancelación si otra escena lo vuelve a pedir (`retire_unused`/`cancel_retire`, `app.rs:298-315`).
- Los paints tardíos del contexto retirado se descartan (`retired`, `app.rs:284-296,1092`).
- El proceso se mata con `taskkill /PID /F` (`host.rs:776-786`), un proceso extra por retirada.
- El crash se reintenta con backoff (`host.rs:368-408`).

**Veredicto:** la carga ya es perezosa por escena y la retirada a 2 s es razonable; no conviene añadir un timeout de inactividad (un plugin debe seguir vivo para responder a pulsaciones). El coste es el precio de Node: **~56–64 MB por plugin**. En la escena `Grabación` (zoom + insta360) son **~120 MB**; en la escena `Orca` no hay plugins Node (todo builtin). Documentarlo es más útil que optimizarlo.

---

## 5. Dependencias y peso en el binario

### 5.1 Árbol

`cargo tree -p orcadeck`: **122 paquetes** (MEDIDA). Duplicados (MEDIDA):

| Crate | Versiones | Quién lo arrastra |
|---|---|---|
| `base64` | 0.22.1 / 0.23.1 | `orcadeck` / `usvg` |
| `getrandom` | 0.2.17 / 0.4.3 | `orcadeck-web` / `orcadeck-host`, `tungstenite`, `zip` |
| `miniz_oxide` | 0.8.9 / 0.9.1 | `png` / `flate2` |
| `syn` | 2.0.119 / 3.0.10 | proc-macros (`zeroize_derive` / `serde_derive`, `thiserror`) |

No hay `cargo-bloat` instalado y el encargo prohíbe instalarlo. Se usó un build **sin LTO** con `-Wl,-Map=…` (mismo código, distinto perfil) y se agregaron los tamaños de las secciones de entrada por crate.

### 5.2 Peso por crate (atribución del mapa)

**ESTIMADA (cálculo sobre el mapa del enlazador, build `lto=false`; 4 429 780 B de secciones de entrada atribuidas):**

| Crate | Bytes | % |
|---|---:|---:|
| `usvg` | 518 143 | 11.7 |
| `skrifa` | 459 817 | 10.4 |
| `harfrust` | 457 459 | 10.3 |
| `tiny_skia` | 385 347 | 8.7 |
| `std` | 291 683 | 6.6 |
| `image` | 241 498 | 5.5 |
| `orcadeck_web` | 223 478 | 5.0 |
| `read_fonts` | 167 295 | 3.8 |
| `orcadeck_host` | 127 065 | 2.9 |
| `orcadeck_builtin` | 119 325 | 2.7 |
| `orcadeck_plugman` | 118 525 | 2.7 |
| `core` | 99 566 | 2.2 |
| `pxfm` (vía `moxcms` de `image`) | 90 200 | 2.0 |
| `resvg` | 62 341 | 1.4 |
| `png` | 53 031 | 1.2 |
| `svgtypes` | 51 644 | 1.2 |
| `roxmltree` | 49 621 | 1.1 |
| `data_encoding` | 48 587 | 1.1 |
| `serde_json` | 45 632 | 1.0 |
| `zune_jpeg` | 34 639 | 0.8 |
| `fontdb` | 34 055 | 0.8 |
| `zip` | 33 841 | 0.8 |
| `tungstenite` | 33 559 | 0.8 |
| `unicode_bidi` / `unicode_script` | 31 851 / 28 461 | 0.7 / 0.6 |
| `imagesize`, `http`, `rustc_demangle` | 28.9 / 28.5 / 27.3 K | ~0.6 cada uno |

**La pila SVG+fuentes** (`usvg`+`skrifa`+`harfrust`+`tiny_skia`+`read_fonts`+`pxfm`+`resvg`+`svgtypes`+`roxmltree`+`fontdb`+`data_encoding`+`imagesize`+`unicode_*`) suma **≈2.47 MB, el 56 %** de las secciones atribuidas. Es el precio de rasterizar texto SVG (títulos de tecla y las 4 acciones builtin) con resvg + fuentes del sistema; no hay recorte sin renunciar al texto o cambiar de renderer.

- `image` (241 KB) incluye `png` (53), `zune-jpeg` (35) y `moxcms/pxfm` (90) para gestión de color; en `image 0.25.10` `moxcms` no es opcional (no hay feature `color-management`), así que no se puede recortar sin cambiar de crate. `default-features=false, features=["png","jpeg"]` ya está bien.
- `orcadeck_web` + `orcadeck_plugman` + `zip` ≈ **375 KB (8.5 %)** solo existen para el editor. `orcadeck-web` embebe además la SPA: 53 058 B en `.rdata` (MEDIDA: `index.html` 3 398 + `app.js` 34 769 + `app.css` 14 453 + `icon.svg` 438).
- `base64` 0.22 se usa solo para decodificar data-URIs (`app.rs:71-72`); `usvg` ya trae 0.23. Unificar a 0.23 elimina un crate (ahorro pequeño, ESTIMADA < 20 KB).
- `getrandom` 0.2 (web) vs 0.4 (host) se puede unificar a 0.4; ahorro pequeño (ESTIMADA < 10 KB).
- `miniz_oxide` y `syn` duplicados vienen de dependencias externas; no se arreglan sin subir versiones ajenas.

**Conclusión §5:** no hay una dependencia gorda evitable sin renunciar a una función (texto SVG, JPEG/PNG, WebSocket, editor). La única poda grande es sacar el editor del binario residente (~8.5 %), que choca con el inspector en vivo.

---

## 6. Tráfico HID por repintado

### 6.1 Constantes y protocolo

- `MAX_JPEG = 9000`, `KEY_PX = 85`, `SIDE_PX = 80` (`proto.rs:7-9`).
- Cada tecla = 1 paquete `BAT` (cabecera con longitud y key) + `ceil(jpeg/N)` paquetes de datos de **N bytes exactos**, rellenados con ceros; cada paquete se envía con 1 byte de report ID (`wire`, `proto.rs:64-69`).
- `N` = longitud del output report, leída del descriptor; si no, 512 por defecto (`main.rs:25-30`). El documento de protocolo dice “probablemente 512 o 1024” (`docs/research/hid/README…`, `hid-protocol.md:34`). **No se midió N real** (habría requerido `probe`, prohibido en este encargo).
- **Un solo `STP` por lote**: `dock.flush()` se llama una vez tras pintar todas las teclas pendientes (`app.rs:1098-1110`). Correcto.
- No hay ACK de salida: `set_key` solo escribe (`main.rs:47-56`).

### 6.2 Bytes por repintado de 15 teclas (ESTIMADA, cálculo exacto)

| JPEG/tecla | Paquetes/tecla (N=512) | Bytes en bucle (N=512) | Bytes en bucle (N=1024) |
|---:|---:|---:|---:|
| 1 685 B (UI plana, medida) | 5 | 38 988 B | 47 150 B |
| 2 152 B (icono real, medida) | 6 | 46 683 B | 62 525 B |
| 5 518 B (ruido, medida) | 12 | 92 853 B | 108 650 B |
| 7 000 B (típico del comentario de `render.rs`) | 15 | 115 938 B | 124 025 B |
| 9 000 B (`MAX_JPEG`) | 19 | 146 718 B | 154 775 B |

Incluye el `STP` (un paquete de N+1). **Escena real `Orca`** (13 teclas de 2 152 B + 2 de ~1 459 B): **30 894 B de JPEG** → **44 631 B** en el cable con N=512, **60 475 B** con N=1024 (MEDIDA la parte JPEG, ESTIMADA la del cable). El `init` añade 3 paquetes (DIS, LIG, CLE) una vez; el heartbeat 1 paquete cada 10 s.

**Desperdicio de relleno:** cada tecla rellena su último chunk a N. Con 2 152 B: 408 B perdidos por tecla a N=512 (13 % del total por tecla) y 920 B a N=1024 (22 %). Es inherente al protocolo; solo se reduce bajando el JPEG (ya está en q75) o eligiendo N menor si el descriptor lo permite.

### 6.3 Coste temporal

- **MEDIDA:** 822 µs/tecla (144²) y 1 624 µs/tecla (288²) de CPU para producir el JPEG.
- **ESTIMADA (modelo):** si cada transferencia OUT interrupt tarda ~1 ms (bInterval típico 1 ms), un lote de 15 teclas a 7 KB son 226 paquetes ≈ **226 ms de dispositivo ocupado**; a 2 KB son 87 paquetes ≈ **87 ms**. Con `ORCADECK_GAP_MS=400`, eso deja el dispositivo ocupado el **22–56 %** del tiempo si se repinta sin parar (coincide con el ~40 % que cita `render.rs:8-9`).
- **Riesgo firmware:** con un spinner a `SPIN_MS=400` (`actions.rs:11`) y 5 agentes `working`, hay un lote de 5 teclas **cada 400 ms indefinidamente** (~19 % de duty a 6 paquetes/tecla). Es exactamente el patrón “pintar casi sin pausa” que pierde pulsaciones. Ver P0-2.

---

## 7. Procedimientos reproducibles sin dispositivo

> Los tres primeros no necesitan dispositivo. Los marcados **[D]** requieren el AKP153E y deben ejecutarse solo cuando el coordinador lo autorice.

### 7.1 CPU y RSS en reposo con dispositivo **[D]**

```powershell
# 1) arrancar el daemon con la escena real (consola aparte)
$env:ORCADECK_GAP_MS = "400"
.\orcadeck.exe run "$env:APPDATA\HotSpot\StreamDock\profiles\R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile"

# 2) medir 60 s sin tocar nada
$p = Get-Process orcadeck
$c1 = $p.TotalProcessorTime.TotalMilliseconds
$w1 = $p.WorkingSet64
$t0 = Get-Date
Start-Sleep -Seconds 60
$p.Refresh()
$secs = ((Get-Date) - $t0).TotalSeconds
$cores = (Get-CimInstance Win32_Processor).NumberOfLogicalProcessors
"CPU % de un nucleo : {0:N3}" -f (($p.TotalProcessorTime.TotalMilliseconds - $c1) / ($secs * 1000) * 100)
"CPU % del sistema  : {0:N3}" -f (($p.TotalProcessorTime.TotalMilliseconds - $c1) / ($secs * 1000) * 100 / $cores)
"RSS MB             : {0:N1}" -f ($p.WorkingSet64 / 1MB)
"Privado MB         : {0:N1}" -f ($p.PrivateMemorySize64 / 1MB)
"Delta RSS MB       : {0:N1}" -f (($p.WorkingSet64 - $w1) / 1MB)

# 3) contadores (1 muestra/s durante 60 s)
Get-Counter '\Process(orcadeck)\% Processor Time' -SampleInterval 1 -MaxSamples 60 |
  ForEach-Object { $_.CounterSamples[0].CookedValue } |
  Measure-Object -Average -Maximum
Get-Counter '\Process(orcadeck)\Working Set - Private' -SampleInterval 1 -MaxSamples 60 |
  ForEach-Object { $_.CounterSamples[0].CookedValue } |
  Measure-Object -Average -Maximum
```

Repetir con una escena sin claves builtin (p. ej. `Grabación`) y con una escena con 5 agentes `working`; la diferencia aísla el coste del polling (§2.3) y del spinner.

### 7.2 Tráfico HID real por lote **[D]**

El propio binario ya traza cada lote (`app.rs:1111-1112`):

```powershell
$env:ORCADECK_TRACE = "1"
# ... run ...
# salida esperada por lote:
#   1234ms painted 15 keys 30904 B in 87 ms
```

`painted N keys B in X ms` da claves, bytes JPEG y tiempo de escritura por lote (no incluye el relleno a N; para eso, multiplicar por paquetes). Para bytes reales de bus, `Wireshark + USBPcap` sobre el dispositivo; no hace falta ningún crate.

### 7.3 Test con HID simulado que cuente despertares/s

Hoy `session()` está acoplado a `hidapi::HidDevice`, así que el test exige un refactor mínimo: extraer un trait `Reader { fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> }` y otro `Writer`. Con eso:

```rust
// tests/wake_budget.rs (propuesto; falla con el diseño actual)
struct FakeHid { reads: Arc<AtomicU64> }
impl Reader for FakeHid {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, String> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(30)); // imita read_timeout(30)
        Ok(0)
    }
}
#[test]
fn idle_device_loop_wakes_at_most_three_times_per_second() {
    let reads = Arc::new(AtomicU64::new(0));
    let h = FakeHid { reads: reads.clone() };
    run_device_loop_for(h, Duration::from_secs(10)); // sin pintados ni plazos
    let per_s = reads.load(Ordering::Relaxed) as f64 / 10.0;
    assert!(per_s <= 3.0, "wakes/s = {per_s}"); // hoy da 33.3
}
```

Objetivo tras el rediseño: **≤ 3 wakes/s en reposo** (idealmente 0) y **1 wake por pulsación**. El test se puede escribir antes del refactor y dejarlo `#[ignore]` como especificación.

### 7.4 RSS de un plugin Node (sin dispositivo)

```powershell
$plug = "$env:APPDATA\HotSpot\StreamDock\plugins\com.miguelpicado.orcadock.sdPlugin"
$p = Start-Process node -ArgumentList 'plugin.js -port 9 -pluginUUID TEST -registerEvent registerPlugin -info {}' `
     -WorkingDirectory $plug -PassThru -WindowStyle Hidden
Start-Sleep 6
$p.Refresh()
"WS MB={0:N1} Private MB={1:N1} Threads={2}" -f ($p.WorkingSet64/1MB), ($p.PrivateMemorySize64/1MB), $p.Threads.Count
Stop-Process -Id $p.Id -Force
```

Con el puerto cerrado el plugin reintenta y la CPU no es representativa; el RSS sí. Medido: 56–64 MB por plugin (§4).

### 7.5 Tabla de tamaños del exe (sin dispositivo)

```powershell
$env:PATH="C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:RUSTUP_HOME="%USERPROFILE%\.rustup"; $env:CARGO_HOME="%USERPROFILE%\.cargo"
$env:RUSTUP_TOOLCHAIN="gnullvm-local"
foreach ($v in @(
  @{n='actual'; e=@{}},
  @{n='strip-off'; e=@{CARGO_PROFILE_RELEASE_STRIP='false'}},
  @{n='unwind'; e=@{CARGO_PROFILE_RELEASE_PANIC='unwind'}},
  @{n='cgu16'; e=@{CARGO_PROFILE_RELEASE_CODEGEN_UNITS='16'}},
  @{n='optz'; e=@{CARGO_PROFILE_RELEASE_OPT_LEVEL='z'}},
  @{n='opt3'; e=@{CARGO_PROFILE_RELEASE_OPT_LEVEL='3'}},
  @{n='ltothin'; e=@{CARGO_PROFILE_RELEASE_LTO='thin'}},
  @{n='nolto'; e=@{CARGO_PROFILE_RELEASE_LTO='false'}}
)) {
  $env:CARGO_TARGET_DIR = "$env:TEMP\ocdeck-perf\$($v.n)"
  foreach ($k in $v.e.Keys) { Set-Item "env:$k" $v.e[$k] }
  cargo build --release -p orcadeck --bin orcadeck *> $null
  "{0,-9} {1,9:N0} B" -f $v.n, (Get-Item "$env:CARGO_TARGET_DIR\release\orcadeck.exe").Length
  foreach ($k in $v.e.Keys) { Remove-Item "env:$k" }
}
```

### 7.6 Microbench del pipeline de imagen (sin dispositivo)

La sonda usada está fuera del repo; el núcleo es este (30 repeticiones tras un warm-up):

```rust
let rgb = imageops::rotate270(&img.resize_exact(85, 85, FilterType::Lanczos3).to_rgb8());
let mut q = 75;
loop {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, q).write_image(rgb.as_raw(), 85, 85, Rgb8)?;
    if out.len() <= 9000 || q <= 1 { break out.len(); }
    q -= 4;
}
```

Medido: 822 µs/tecla (144²), 1 624 µs (288²); JPEG 1 685 B (UI) / 5 518 B (ruido). Repetir con el binario del daemon (`opt-level="s"`) daría el número exacto de producción; la sonda se compiló con el perfil release por defecto (`opt-level=3`), así que el coste real será **algo mayor** (ESTIMADA).

---

## 8. Lista final priorizada

> **Ganancia** = efecto esperado si se aplica solo esa medida. **Dispositivo** = hace falta el AKP153E para validar el resultado (no para medir CPU/RSS).

### P0 — incumplen el objetivo rector o el hallazgo de firmware

| # | Mejora | Ganancia esperada | Riesgo | Ficheros | ¿Dispositivo? |
|---|---|---|---|---|---|
| **P0-1** | **Bajar la cadencia de RPC a Orca**: `POLL_MS 1000→3000–5000`, `WORKERS_MS 10 000→30 000`, `runList` a 30 s. Mantener el spinner local a 400 ms. | **−60…−80 % de ~3.2 % de un núcleo** (medido: 19–27 ms por poll a 0.83/s + 160–278 ms de `workerList` cada 10 s) → **≈0.6–1.3 %**. Además −1.2 hilos/s en el daemon. | Estado de agentes hasta 3–5 s tarde (¿aceptable para un dock?). `workerList` a 30 s retrasa contadores de workers. | `crates/builtin/src/actions.rs` (`POLL_MS`, `WORKERS_MS`, `fetch_agents`) | No para CPU; sí para validar UX |
| **P0-2** | **`SPIN_MS 400→800`** (o congelar la animación tras 60 s). | **−50 % del tráfico HID animado** y del duty del dispositivo: 5 agentes pasan de un lote cada 400 ms a uno cada 800 ms (≈19 %→≈10 % de duty a 6 paq/tecla); reduce el riesgo de pulsaciones perdidas del AKP153E. | Spinner menos vivo. Si se congela, un agente `working` parece parado. | `crates/builtin/src/actions.rs` (`SPIN_MS`) | **Sí** (pulsaciones bajo pintado) |

### P1 — memoria y arquitectura de espera

| # | Mejora | Ganancia esperada | Riesgo | Ficheros | ¿Dispositivo? |
|---|---|---|---|---|---|
| **P1-1** | **Eliminar el mapa `latest`**; repintar tras re-plug desde `view.effective(key)`. | **−1.2…−2.5 MB RSS** con imágenes 144²; **−5…−10 MB** con 288². Un clon de 83–332 KB menos por repintado. | El repintado de reconexión debe componer título/feedback; probar desenchufar/enchufar. | `src/app.rs` (`session`, `switch_scene`, `paint`, `Runtime`) | Sí (reconexión); no para RSS |
| **P1-2** | **Pre-escalar a 85/80 al ingerir** y saltar el resize en `key_jpeg` si ya está a `px`. | **−0.7…−1.8 ms por tecla** (≈70 % de `key_jpeg`): lote de 15 teclas **12–30 ms → 3–5 ms**; con P1-1, memoria persistente **2.5 MB → ~0.9 MB** (y 10 MB → 0.9 MB en 288²). | Cambio de aspecto si el filtro no es idéntico; la composición de título/feedback ya asume 85. | `src/app.rs`, `src/render.rs` | Sí (visual) |
| **P1-3** | **Lectura HID bloqueante en hilo lector** + bucle de device por `recv_timeout(next_deadline)`/`recv()`, canal único de eventos. | **33.3 → 0 wakes/s en reposo**; **−0…30 ms de latencia por pulsación**; mantiene un solo escritor, un solo `STP` y `ORCADECK_GAP_MS`. | Segundo handle/thread-safe de hidapi, cierre en reconexión, handshake reenviado; validar que no se pierden pulsaciones. | `src/app.rs`, `src/main.rs` (`Dock`), `src/proto.rs` | **Sí** |
| **P1-4** | **Builtin por deadline** (parte de P1-3): `recv_timeout(min(próximo plazo))` o `recv()`; plazos: minuto si hay reloj, `SPIN_MS` solo si hay `working`, `POLL_MS` si hay claves, fin de blink. | **2.5 → 0 wakes/s** y 37.5 → 0 construcciones de SVG/s en reposo. CPU del tick ya es 7–15 µs (despreciable); el valor es no despertar. | Baja; cuidado con el orden tick-antes-de-esperar para no perder el repintado tras una pulsación. | `src/app.rs`, `crates/builtin/src/actions.rs` | No |

### P2 — menores, o solo si el tamaño/energía importan

| # | Mejora | Ganancia esperada | Riesgo | Ficheros | ¿Dispositivo? |
|---|---|---|---|---|---|
| P2-1 | Un **hilo RPC persistente** con canal de peticiones en vez de un hilo por llamada (`orca.rs:57-79`). | −1.8 hilos/s y −2 aperturas de pipe/s; evita el hilo colgado 5 s si el pipe se bloquea. | Manejo de timeout/reconexión del pipe. | `crates/builtin/src/orca.rs` | No |
| P2-2 | **Unificar `base64` a 0.23** y `getrandom` a 0.4. | Elimina 2 crates duplicados; ESTIMADA < 20 KB de exe y árbol más simple. | Muy bajo (API compatible en lo usado). | `Cargo.toml`, `crates/web/Cargo.toml` | No |
| P2-3 | **`TerminateProcess` directo** en vez de `taskkill /PID /F` al retirar un plugin. | −1 proceso por retirada; ESTIMADA 3–10 ms menos y menos ruido en el sistema. | Abrir proceso por PID (permisos); ya se usan llamadas raw de kernel32 en `ctl.rs`. | `crates/host/src/host.rs` | No |
| P2-4 | **Sacar el editor** (`orcadeck-web` + `plugman` + `zip`) a otro binario. | **−375 KB ≈ −8.5 %** de las secciones y código no residente en `run`. | Pierde el inspector en vivo y la reutilización del host de plugins. **Descartado el 2026-11-11 por decisión del usuario.** | `Cargo.toml`, `src/main.rs`, `crates/web` | No |
| P2-5 | **Relleno negro de celdas laterales 16–18**: `scene_jpegs` solo rellena 1..=15 (`main.rs:89-94`) y `run` solo precarga 1..=15 (`app.rs:1007`), así que la tecla 17 nunca se pinta y queda el splash de arranque. | Cosmético; coherencia entre `render`/`scene`/`run`. | Ninguno. | `src/main.rs`, `src/app.rs` | Sí (visual) |
| P2-6 | **No hacer**: `opt-level="z"` (−466 KB pero **+40 %** de pipeline), `panic="unwind"` (+219 KB), `lto="thin"` (+596 KB), `cgu=16` (+36 KB y peor código). | — | — | `Cargo.toml` | No |

---

## Dudas y límites de la auditoría

1. **N real del reporte HID**: no se midió (requiere `probe`, prohibido). Los cálculos de bytes se dan para 512 y 1024; el código usa 512 si el descriptor no responde.
2. **CPU/RSS de `run` con dispositivo**: no medidos, por la prohibición de ejecutar `run`. Se midieron los componentes (suelo del `WaitForSingleObject`, tick de builtin, RPC de Orca, pipeline de imagen, `edit` en reposo = 6.5 MB WS / 0.00 % CPU), pero no el proceso completo con el bucle de 30 ms y el dispositivo conectado.
3. **Tiempo de bus HID**: el modelo de ~1 ms por transferencia OUT es una hipótesis (bInterval típico); hay que confirmarlo con `ORCADECK_TRACE` o USBPcap.
4. **`workerList`**: 160–278 ms medidos hoy con Orca ocupado; el comentario del código dice ~450 ms. El número depende de la carga de Orca.
5. **Coste exacto con `opt-level="s"`**: las sondas de `key_jpeg` y resvg se compilaron con el perfil release por defecto (`opt-level=3`); el daemon real será algo más lento (ESTIMADA la diferencia).
6. **`panic="abort"`**: no se ejecutó la suite con él; Cargo ignora `panic` en el perfil `test`, así que los tests no cambian, pero no se verificó aquí.
7. **Node**: el RSS varía con el plugin y su `node_modules`; medidos 56–64 MB en dos plugins, no es una ley.
