# Estado del proyecto (2026-11-11)

Punto de entrada para retomar: [`HANDOFF.md`](HANDOFF.md). Qué falta, en orden:
[`ROADMAP.md`](ROADMAP.md). Arquitectura vigente: [`../design/ARCHITECTURE.md`](../design/ARCHITECTURE.md).

## Dónde estamos

Sustituto en Rust de «Stream Dock AJAZZ» para el AKP153E: un único binario (`orcadeck.exe`) hace de
driver HID, host de plugins del SDK v2 y, bajo demanda, servidor del editor web. Sin admin, sin UI
residente y sin runtime async. El workspace, `run`, `edit`, Scene Shift, la instalación de plugins y
el Property Inspector están mergeados en `main`; la prueba en dispositivo sigue pendiente.

## Mergeado en `main`

| Pieza | Estado |
|---|---|
| Scene Shift en `run` | `scenes.rs`; +27 136 B; tests unitarios; **sin probar en dispositivo** |
| Instalar/quitar plugins desde `edit` | `plugman` + `web`, ledger propio de instalados y escaneo de Descargas bajo demanda (sin vigilante) |
| Property Inspector | servidor en `host` + iframe en `edit`: sandbox sin `allow-same-origin`, token de un solo uso por `postMessage`, CSP sin red externa y `Origin: null` solo con sesión de PI abierta |
| `edit` dentro de `run` | canal de control `ctl.rs` (named pipe) con fallback standalone; recarga en caliente de una tecla; `RunInspector` como `InspectorBridge` |
| Tests | **141 verdes** con `cargo test --release --workspace` (medido 2026-11-11; 0 fallos) |
| Binario | `orcadeck.exe` = **3 725 824 B** (~3,55 MiB) recompilado el 2026-11-11 |

## Limpieza hecha

- Plugin Node antiguo, `deploy.ps1` e instalador de referencia archivados en `tools/legacy/`.
- Paquetes de terceros del usuario (zips, `.rar`, instalador, driver) movidos a `vendor/`
  (gitignorado).
- Borradas las extracciones regenerables `extract-2.10.185` y `current-3.10.191` (311 MB).

## Pendiente sin dispositivo

1. Verificar el primer run de `.github/workflows/ci.yml` en GitHub (usa el toolchain MSVC del
   runner con `RUSTUP_TOOLCHAIN=stable`; no se pudo probar en local).
2. Restos de [`REPO-AUDIT.md`](REPO-AUDIT.md): `base64`/`getrandom` duplicados y clippy
   (no está instalado en el toolchain `gnullvm-local`). Hechos: `cargo fmt` (commit aparte, 2026-11-11), licencia `MIT OR Apache-2.0`, rutas de usuario, código muerto,
   metadatos de Cargo, retirada de plugins.
3. Huecos de tests de [`REPO-AUDIT.md`](REPO-AUDIT.md) §4.2: desconexión HID en caliente, caída de
   la tubería de Orca, SQLite ocupado, traversal en iconos de perfil, tramas WebSocket enormes y
   carrera entre Scene Shift y pintado.

## Rendimiento (auditoría estática, [`PERF-AUDIT.md`](PERF-AUDIT.md))

No es todo lo ligera que podría. Sin medir aún `run` con dispositivo; solo `edit` en reposo (0,00 % CPU, 6,5 MB).

- **Mayor coste en reposo:** el polling RPC de `builtin` a Orca, ~3,2 % de un núcleo medido
  (19–27 ms por poll, `workerList` 160–278 ms cada 10 s). Subir intervalos lo deja en ~0,6–1,3 %.
- **Despertares:** el bucle del dispositivo se despierta 33,3 veces/s (lectura HID con timeout de
  30 ms) y `builtin` 2,5 veces/s; ambos sustituibles por espera bloqueante o por plazos.
- **Spinner:** 400 ms→800 ms reduce a la mitad el tráfico HID animado y el riesgo de pulsaciones perdidas.
- **Memoria:** el mapa `latest` cuesta 1,2–2,5 MB (hasta 10 MB con 288²); pre-escalar baja el lote
  de 15 teclas de 12–30 ms a 3–5 ms. Node: 56–64 MB por plugin (medido en dos).
- **Perfil release:** ya es bueno; no usar `opt-level="z"` (+40 % de pipeline), `lto="thin"`
  ni `panic="unwind"`.
- Plan ordenado en [`ROADMAP.md`](ROADMAP.md) (filas 14–16).

## Medido con dispositivo (2026-11-12, AKP153E, perfil Orca, `run`)

| Situación | CPU (% de un núcleo) | RSS | Hilos / handles |
|---|---|---|---|
| 4 agentes animando, antes de la caché de rasterizado | 8,7 | 49–58 MB | 4–5 / 106 estables |
| 2 teclas animando (1 y 13), con caché | 2,6 (ventana de 60 s) | 39,7 MB | 6 / 108 |
| Proceso recién lanzado | — | 13–40 MB | — |

- Caché `(svg,px)→imagen` en el hilo builtin: rasterizado 4–5 ms → ~20 µs por acierto.
- USB: ~5 ms por paquete de 512 B (~30 ms por tecla, JPEG ~3 KB); el JPEG cuesta <1 ms.
- Re-plug USB probado: `run` detecta la desconexión, reintenta cada 2 s y repinta las 17 teclas
  en ~585 ms al volver el dispositivo.
- `edit` dentro de `run` probado: guardar una tecla (mover/cambiar) la repinta en caliente; el slider de
  brillo se aplica al instante, al arrancar (`state.json`) y tras cada reconexión. Antes ninguno de los
  dos estaba cableado (`Runtime::reload` solo lo llamaba un test): ahora `FsStore::put_scene` y
  `set_device` avisan por `InspectorBridge::saved`/`brightness`.
- El `Title`/`ShowTitle` del manifiesto no se dibuja en `run` (solo el `setTitle` de plugins);
  decisión del usuario: no hace falta.
- `crates/web/tests/real_folders.rs` (comparaba con el perfil real del usuario, frágil) eliminado.
- Editor permanente (2026-11-12): acceso «OrcaDeck Editor» en el menú Inicio (`orcadeck-task.ps1 editor`,
  `conhost --headless`, sin ventana; PowerToys Run lo encuentra). El editor arranca en ~80 ms y termina solo
  ~5 s después de cerrar la pestaña: la página hace long-poll a `/api/live` y el servidor se cierra cuando
  deja de recibirlo (`Server::wait_idle`; 30 s si la pestaña nunca llega). Ese mismo `/api/live` entrega al
  editor los JPEG que `run` envía al dispositivo (`InspectorBridge::mirror`), así que la rejilla muestra en
  tiempo real lo mismo que el panel; la UI los gira 90° (el panel recibe las imágenes giradas 270°).
- Calidad (2026-11-12): imágenes del pipeline a 2× (`WORK_SCALE`, 170 px) y JPEG del dispositivo q85 (probado q92: perdía fotogramas; antes q75; `ORCADECK_JPEG_Q` lo cambia sin recompilar). El espejo del editor usa la imagen 2× (q88, sin giro). Vigilar pulsaciones perdidas y fps con 4 teclas animando: los fotogramas pesan más por USB.
- Editor lento al abrir (2026-11-12): NO es de OrcaDeck. `edit` tarda 14 ms hasta lanzar el navegador (`%LOCALAPPDATA%\OrcaDeck\edit-timing.log`). `ShellExecute` (acceso directo, `orcadeck.exe` o `cmd.exe` firmado) tarda ~21 s a veces (no siempre; no reproducido tras 330 s de inactividad en la última prueba) y justo después ~50 ms; `CreateProcess` directo (conhost/orcadeck) siempre <250 ms. Causa sin identificar (no AppXSvc, no Defender, no el .lnk). Siguiente: capturar una apertura lenta con Process Monitor/WPR (requiere admin) o probar una exclusión de Defender para `%LOCALAPPDATA%\OrcaDeckin`.
- Sin medir: reposo real (ningún agente trabajando) y 4 agentes tras la caché; depende de
  que los agentes de Orca estén parados, no se puede forzar desde aquí.

## Decisiones de la sesión 2026-11-12

- Scene Shift siguiente/anterior salta escenas sin tecla `profile.rotate` (NetWork no la tiene y
  sus plugins `.html` no se ejecutan).
- Las pinturas tardías de builtin tras cambiar de escena se descartan por generación
  (`Runtime::paint_at`).
- Spinner: fase continua a 100 ms (20 ángulos); `BuiltinMsg::Painted` dispara el siguiente
  fotograma tras cada lote escrito; `SPIN_MS` = 1000 es solo espera de respaldo.
- `BATCH_GAP_MS` = 125; aro de aviso de done/pregunta = 5 min.
- El AKP153E rev. 1 no acepta GIF: el firmware solo recibe JPEG por tecla.

## Pendiente con dispositivo y OK del usuario

1. Parar Stream Dock con `schtasks /end /tn StreamDock`.
2. Probar en vivo `run`, `edit` (incluido dentro de `run`), Scene Shift, Property Inspector e
   instalación de plugins desde Descargas.
3. Medir exe, RSS y CPU en reposo; probar el autoarranque.
4. Investigar la pulsación larga por firmware (solo análisis/emulación; **no flashear**).

## Límites conocidos

- El firmware v1 entrega un solo evento por pulsación y **al soltar**; `state` siempre es 0. No hay
  pulsación larga ni down/up sin cambiar de firmware o de hardware.
- AKP153E v1 sin dial y con panel lateral no táctil. Plugins `.html` no soportados.
- El fallback standalone de `edit` no recarga en caliente; con `edit` dentro de `run`, sí la tecla
  guardada. El cambio de escena relee el manifiesto.
- Los plugins de escenas anteriores siguen vivos para conservar sus ajustes (consumen RAM).
- El parche de firmware está diseñado y emulado, **nunca flasheado**; regla dura: no flashear.

## Entorno

- Windows 10/11 x64, Rust `gnullvm-local` (1.99.0) y llvm-mingw portable en
  `C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin`.
- Gate: `cargo test --release --workspace` desde `daemon/`; CI en `.github/workflows/ci.yml`; releases en `release.yml`.
- `libunwind.dll` debe estar en `PATH` o junto al `.exe`.
