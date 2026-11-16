# Handoff 2026-11-16 (léelo primero)

Rama `dra-experiment`. DRA en producción para el usuario: teclas 98×98 calibradas, barra lateral continua, tecla de resumen que
salta al agente en espera más antiguo. Todo compilado e instalado (`%LOCALAPPDATA%\OrcaDeck\bin`, tarea `OrcaDeck`) y confirmado
en el dispositivo. Guía de uso: `docs/guide/USAGE.md` §4b. Las secciones de más abajo (2026-11-11/12/13) son históricas: lo que dicen
sobre DRA sin dibujar o sobre el panel en 3 celdas está superado.

## Firmware pulsar/soltar: en curso (2026-11-16, tarde)
El usuario QUIERE detectar pulsar/soltar (y luego pulsación larga). Corrige la nota anterior de «no se parchea»: lo descartado es el parche de la tira (BAT), no el de pulsar/soltar.
- Host: `app.rs` ya entiende state 1/0 (`key_phase`, `Phase`; test `key_phase_handles_stock_and_press_release_firmware`). Con el firmware original (solo state 0) se comporta como antes.
- Flasheados hoy con éxito (herramienta «Updating successfully», reenchufe tras cada uno): v3, v1, parche identidad (`work/make_patch_identity.py`, 23 B, desvía al hueco y vuelve al original) y de nuevo el original. Lanzar la herramienta con su carpeta como directorio de trabajo y las rutas entre comillas; `bootwatch` desde la copia instalada (la de `target\release` no arranca sin `libunwind.dll`).
- **Síntoma actual, con CUALQUIER imagen, incluido el original (hash 15e03f5b…) y con OrcaDeck corriendo como tarea programada:** al pulsar una tecla la pantalla se apaga, no llega ningún informe de tecla al host (traza `ORCADECK_TRACE` y `watch` sin líneas) y no se ejecuta la acción; al pulsar otra vez se enciende. Esta mañana (antes de flashear) las teclas funcionaban.
- Por tanto NO está demostrado que v1/v3 fallen: la prueba está contaminada por una regresión del dispositivo/entorno posterior a los flasheos. Sospechas sin comprobar: palabras 5..9 del sector de configuración `0x0807E000` (el handler APP/NEW no las restaura; ver `bootloader-trigger.md`); estado de HID en Windows tras tantos reenumerados; el firmware pide `STP` (flag `0x2000010c`) para reportar teclas y el modo DRA no lo envía.
- Siguiente paso sugerido: recuperar primero las pulsaciones con el original (probar enviar `STP` al arrancar, `orcadeck watch` con `ORCADECK_CMD=STP:`, reiniciar Windows/otro puerto USB, o comparar con el exe del commit `7c5f888`). Solo cuando el original entregue pulsaciones, volver a probar v3.
- La unidad lleva ahora el firmware ORIGINAL.

## Compilar / instalar (esta máquina)
```powershell
$env:PATH='C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin;'+$env:PATH   # el toolchain gnullvm sale de daemon/rust-toolchain.toml
cd daemon; cargo build --release
Stop-ScheduledTask OrcaDeck; Get-Process orcadeck | Stop-Process -Force
..\tools\autostart\orcadeck-task.ps1 install; ..\tools\autostart\orcadeck-task.ps1 enable; Start-ScheduledTask OrcaDeck
```
El editor va embebido (`include_str!`): cambios en `daemon/crates/web/web/*` exigen recompilar + reinstalar.

## Pendiente
- Tests: `cargo test --release --workspace -- --test-threads=1` con `ORCADECK_CALIBRATION=C:\nonexistent\cal.json` pasa; en paralelo 5 tests de `app::tests::*` fallan/cuelgan (estado global compartido: `STRIP`, `MIRROR`, calibración). No comprobado si ya pasaba antes.
- Temp CPU de la barra necesita HWiNFO64 con «Shared Memory Support» (activo en esta máquina).
- `AGENTS.md` y los `__pycache__` de `docs/research/firmware/work` siguen sin trackear.
- Diagnósticos del IDE sobre `status` deprecated en `app.js`: preexistentes, ignorar.

---

# HANDOFF (2026-11-12)

## Estado actual (2026-11-12): OrcaDeck ya sustituye a Stream Dock

- Tarea programada `OrcaDeck` (al iniciar sesión, sin ventana, `QUOTA_DIR` fijado, log
  `%LOCALAPPDATA%\OrcaDeck\orcadeck.log`) habilitada. `StreamDock` y su entrada Run, desactivadas.
- La tarea ejecuta una COPIA en `%LOCALAPPDATA%\OrcaDeck\bin`. Tras recompilar, redesplegar:
  `Stop-ScheduledTask OrcaDeck`; matar `orcadeck.exe`; `tools\autostart\orcadeck-task.ps1 install`;
  `... enable`; `Start-ScheduledTask OrcaDeck` (`install` la deja deshabilitada y mata el proceso).
- Editor: acceso «OrcaDeck Editor» en el menú Inicio (`orcadeck-task.ps1 editor`, sin ventana); se
  cierra solo ~7 s tras cerrar la pestaña. Muestra en vivo el panel (JPEG de `run`, girado 90° con CSS).
- Gate: todo verde con `cargo test --release --workspace` (`orca_pipe` es intermitente).
- Hecho 2026-11-12 (tarde): `register` ejecutado (HKCU; `orcadeck unregister` lo deshace). Imágenes del pipeline a 2×
  (`WORK_SCALE` en `proto.rs`, 170 px; `key_jpeg` reduce una vez al tamaño del dispositivo) y espejo del editor a esa
  resolución (JPEG q88, sin giro CSS). JPEG del dispositivo q85 (`ORCADECK_JPEG_Q`; q92 perdía fotogramas, q75 era el
  valor original). `real_folders.rs` eliminado. `edit` escribe `%LOCALAPPDATA%\OrcaDeck\edit-timing.log`.
- Editor «lento al abrir» (40-50 s desde PowerToys Run): NO es de OrcaDeck (`edit` tarda 14 ms hasta lanzar el
  navegador). `ShellExecute` (hasta de `cmd.exe`) tarda ~21 s de forma intermitente; `CreateProcess` directo, <250 ms.
  Causa sin identificar. El usuario va a probar una exclusión de Defender:
  `Add-MpPreference -ExclusionPath "$env:LOCALAPPDATA\OrcaDeckin"` (deshacer: `Remove-MpPreference`). Resultado
  pendiente: tras ~10 min sin usar, abrir el editor y comparar la hora de `edit start` del log con la de Enter.
  Si no mejora: quitarla y capturar con Process Monitor/WPR.
- Pendiente: CPU de `run` (52 s en ~8 min, ~11 % de un núcleo, tras el 2×) con y sin `WORK_SCALE`; reposo real y con
  4 agentes; fps/pulsaciones con q85; tecla (4,2) de Orca se deja como está (decisión del usuario); un `edit` matado
  con la pestaña abierta bloquea el siguiente hasta ~30 s (`serve_edit`, `app.rs`; el error no se ve con conhost
  headless).
- Hecho 2026-11-13: acciones builtin de sistema (`crates/builtin/src/sys.rs`) con los UUID de HotSpot
  (`com.hotspot.stream.{cpu,memory,PhysicalDisk,NetWork,cpu.temperature,Cpu_Clock,Cpu_Package_Power,Gpu_*}`) y de
  `com.krabs.windowsgizmos.{Lock,Reboot,Shutdown,TaskManager}` + nuevo `com.miguelpicado.orcadock.logoff` (añadido al
  manifest del plugin Node para el editor). CPU/RAM/disco C:/red vía Win32; GPU vía `nvml.dll`; temp/reloj/potencia de CPU
  solo con HWiNFO64 y «Shared Memory Support» (sin él: «N/D»). Apagar/reiniciar/cerrar sesión piden 2.ª pulsación (4 s).
  Pendiente: probar en dispositivo; la página 2 de Monitorización (hora/tiempo HotSpot) no está implementada; sin
  clippy/rustfmt instalados en el toolchain gnullvm (CI sí corre clippy).
- Hecho 2026-11-13 (después): teclas rediseñadas (barra grande con texto dentro, naranja en atención, tecla roja en
  peligro; umbrales en las llamadas a `level(...)` de `sys.rs`). Plugin HotSpot «System Performance Monitor» movido a
  `%APPDATA%\HotSpot\StreamDock\plugins-removed-20261109\`; las teclas son de `com.miguelpicado.orcadock.*`
  (`cpu`, `cpu_temp`, `cpu_clock`, `cpu_power`, `ram`, `disk`, `net`, `gpu_*`, `lock`, `logoff`, `restart`, `shutdown`,
  `taskmgr`; los UUID HotSpot/krabs siguen como alias). Escenas Monitorización/Tablero/Orca migradas (copias en
  `%LOCALAPPDATA%\OrcaDeck\*.bak.json`). Panel lateral: `panel_time|panel_cpu|panel_gpu`, dos mini-teclas apiladas por
  celda (5,0..5,2 de Monitorización). Panel lateral movido a la escena Orca (5,0..5,2); Monitorización sin celdas laterales (por decidir).
  Probado 2026-11-13: las 3 celdas laterales dejan hueco físico; BAT con imagen alta (80x400) y DRA no dibujan nada en el
  AKP153E (pantalla en negro; se recupera relanzando el daemon). Un panel continuo solo sería posible con el logo `LOG`
  (escribe en flash): descartado sin plan de recuperación. Pendiente: probar en dispositivo.
- Defender: con la carpeta excluida el cambio de escena y la carga de imágenes van mucho más rápido (confirmado por el usuario).
- Lo de abajo es histórico: «sin probar en dispositivo» ya no aplica.

---

# (2026-11-11)

Resumen vigente para retomar. Detalle: [`STATUS.md`](STATUS.md); pendientes ordenados:
[`ROADMAP.md`](ROADMAP.md); arquitectura: [`../design/ARCHITECTURE.md`](../design/ARCHITECTURE.md).

## Objetivo

Sustituto extremadamente ligero de «Stream Dock AJAZZ» para el AKP153E: un binario Rust, sin
runtime async ni UI residente, 0 % de CPU en reposo, memoria y tráfico HID mínimos. La UI web
(`orcadeck edit`) se levanta bajo demanda.

## Estado

- `main` tiene el workspace integrado: `run`, `edit`, Scene Shift, instalar/quitar plugins desde
  `edit` (escaneo de Descargas bajo demanda y ledger propio), Property Inspector y `edit` dentro de
  `run` por named pipe (`ctl.rs`), con fallback standalone y recarga en caliente de una tecla.
- Gate: `cargo test --release --workspace` → **92 tests verdes** (medido 2026-11-10).
- `orcadeck.exe` = 3 681 792 B. **Sin probar en dispositivo** todo lo mergeado después de `run`.
- Limpieza hecha: plugin Node, `deploy.ps1` e instalador de referencia en `tools/legacy/`;
  paquetes del usuario en `vendor/` (gitignorado); extracciones de 311 MB borradas.

## Reglas duras

1. **No flashear.** No lanzar `FirmwareUpgradeTool.exe` con 4 argumentos
   (`768 4112 <bin> <icono>` flashea al instante), ni enviar `CRT\0\0APP NEW`.
2. **No ejecutar Stream Dock y OrcaDeck a la vez**: comparten HID y puerto 18618.
3. No borrar ni mover nada que no se haya nombrado por ruta exacta.
4. No commitear ni tocar `vendor/`; son paquetes del usuario.

## Rendimiento

[`PERF-AUDIT.md`](PERF-AUDIT.md) (auditoría estática, cifras marcadas MEDIDA/ESTIMADA). Quick wins sin
dispositivo: P0-1 (cadencias de RPC a Orca) y P0-2 (`SPIN_MS` 800). Lo que toca `app.rs` (hilo lector HID
bloqueante, `builtin` por plazos, quitar `latest`) esperar a medir `run` con el dispositivo
(procedimiento en §7 del informe) para validar que no se pierden pulsaciones.

## Hecho el 2026-11-11 (workers OpenCode y agy)

`main` incluye: animación de teclas (spinner 250 ms con cabeza visible, aro de alerta, temporizador solo en pregunta/done), cadencias nuevas, doble clic en plugins, bucle `builtin` por plazos, hilo lector HID opcional, pre-escalado de imágenes y sin mapa `latest`, tubería `ctl` blindada, límite WebSocket de 4 MiB, fix de traversal de iconos, `base64`/`getrandom` unificados, tabla `DeviceSpec`, clippy bloqueante, humo del binario y E2E en CI, workflow de release, script de espejo público y docs públicas EN/ES. Gate: **141 tests verdes**; `orcadeck.exe` = 3 725 824 B.

Todo lo anterior está **sin probar en dispositivo**. Espejo público: https://github.com/miguelpicado/a-better-ajazz-controller (se actualiza con `node tools/publish-public.mjs`, nunca con `git push` directo; los binarios de firmware de AJAZZ son solo locales).

## Probado en dispositivo el 2026-11-12 (AKP153E, perfil Orca)

`run` pinta, anima y no pierde pulsaciones con 4 teclas animando. Cambios derivados de la prueba:
aviso (aro) de 5 min en done/pregunta; spinner con fase continua (50 ms de resolución) y
`BuiltinMsg::Painted` tras cada lote (cada fotograma se renderiza justo antes de enviarse);
`BATCH_GAP_MS` 125; marcador de cuota redondeado a píxel (antes repintaba 5 teclas por tick).
El USB escribe ~5 ms por paquete de 512 B (~30 ms por tecla): ~4 fps reales con 4 teclas.
El AKP153E no acepta GIF (`M_V` se descarta); no se flashea. Pendiente: reordenar teclas con la UI;
`orca_pipe` se colgó una vez 12 min (intermitente, no reproducido); resto de la lista de pruebas.

Después (misma sesión): Scene Shift salta escenas sin tecla `profile.rotate`; pinturas tardías de
builtin se descartan por generación (`Runtime::paint_at`); spinner a 100 ms de resolución (20 ángulos);
caché de rasterizado (160 entradas, 4–5 ms → ~20 µs); `ORCADECK_TRACE=1` imprime teclas, bytes, ms y
tiempos de JPEG/raster por lote; `ORCADECK_GAP_MS` cambia el hueco. Medidas en [`STATUS.md`](STATUS.md).
Pendiente de decisión del usuario: la tecla (4,2) de la escena Orca apunta directo a NetWork por id.

## Lista de pruebas con el dispositivo (siguiente sesión)

Con Stream Dock parado (`schtasks /end /tn StreamDock`) y `QUOTA_DIR` definido:

1. `orcadeck run <perfil>`: pintado, spinner a 4 fps, aro y temporizador en una tecla de agente; **pulsar mientras animan** (hueco `BATCH_GAP_MS` 250; si se pierden pulsaciones: `ORCADECK_GAP_MS=400`).
2. Re-plug USB: todas las teclas se repintan con título/feedback.
3. Scene Shift y cambio de escena; `edit` dentro de `run` (tubería blindada) y recarga de una tecla.
4. `orcadeck register` y doble clic en un `.streamDeckPlugin` real (deshacer con `unregister`).
5. Opcional: `ORCADECK_HID_READER=thread` (pulsaciones y re-plug).
6. Medir exe, RSS y CPU en reposo; anotarlos en `STATUS.md`.

## Siguiente, en orden

1. Sin dispositivo: ver el primer run de CI en GitHub (`.github/workflows/ci.yml`, sin probar),
   `base64`/
   `getrandom` duplicados . Hecho: retirada de plugins, rutas de
   usuario, código muerto, metadatos Cargo.
   **Cuidado:** sin `QUOTA_DIR` las teclas de cuota leen `%APPDATA%\OrcaDeck\quota` y quedan
   vacías; definir `QUOTA_DIR=<quota-monitor>` (entorno o tarea de autoarranque).
2. Con dispositivo y OK del usuario: parar Stream Dock (`schtasks /end /tn StreamDock`), probar
   `run`, `edit`, Scene Shift, PI e instalación de plugins en vivo; medir exe/RSS/CPU; autoarranque.
3. Pulsación larga: solo investigación/emulación de firmware. **No flashear** sin plan de
   recuperación y orden explícita; alternativa segura: AKP153E rev. 2 (PID `0x3010`).

## Rutas clave

- Perfil Orca: `%APPDATA%\HotSpot\StreamDock\profiles\R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile`.
- Firmware original: [`../research/firmware/backup/AKP153E.bin`](../research/firmware/backup/AKP153E.bin).
- Evidencia del actualizador: [`../research/firmware/updatetool-origin.md`](../research/firmware/updatetool-origin.md).
- Compilar: [`../guide/DEVELOPMENT.md`](../guide/DEVELOPMENT.md).
