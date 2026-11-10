# ROADMAP (2026-11-10)

Objetivo rector en [`HANDOFF.md`](HANDOFF.md). Estado verificado en [`STATUS.md`](STATUS.md).

## A. Terminar sin dispositivo

| # | Pieza | Estado |
|---|---|---|
| 1 | `run`: driver + host + builtin, lotes, reconexión | hecho, probado en vivo |
| 2 | Títulos, estados, ok/alert, OpenUrl, reinicio de plugins | hecho; sin prueba física tras los últimos merges |
| 3 | Límites zip + overlay obsoleto en `plugman` | hecho |
| 4 | `orcadeck edit` (editor web bajo demanda) | hecho; 0 s CPU y ~7 MB RSS en reposo, medido |
| 5 | Scene Shift | mergeado en `main` (`scenes.rs`, +27 136 B); pendiente prueba física |
| 6 | Property Inspector | servidor e iframe integrados en `edit`; pendiente prueba con plugins reales |
| 7 | Instalar/quitar plugins desde `edit` | hecho, con escaneo de Descargas bajo demanda; falta prueba en vivo |
| 8 | `edit` dentro de `run` (`ctl.rs`) + recarga en caliente de una tecla | hecho, con fallback standalone; sin prueba física |
| 9 | Retirada de plugins sin uso al cambiar de escena | hecho (2 s de gracia, +7 168 B); sin prueba física |
| 10 | Autoarranque (`tools/autostart/orcadeck-task.ps1`) | tarea `OrcaDeck` instalada DESACTIVADA; activar exige consola elevada |
| 11 | P0–P2 de [`REPO-AUDIT.md`](REPO-AUDIT.md) | hecho: rutas de usuario, código muerto, metadatos Cargo, `.github/workflows/ci.yml` (sin verificar en GitHub). Pendiente: `base64`/`getrandom` duplicados, `cargo fmt` (28 ficheros), licencia (decide el usuario), clippy no instalado en el toolchain local |
| 12 | Huecos de tests de [`REPO-AUDIT.md`](REPO-AUDIT.md) §4.2 | pendiente |
| 13 | Medidas de ligereza: exe, RSS y CPU en reposo con el dispositivo | pendiente (necesita dispositivo); procedimientos en [`PERF-AUDIT.md`](PERF-AUDIT.md) §7 |
| 14 | Rendimiento P0 de [`PERF-AUDIT.md`](PERF-AUDIT.md): cadencia RPC a Orca (`POLL_MS` 1000→3000–5000, `WORKERS_MS` 10 s→30 s) y `SPIN_MS` 400→800 | pendiente, sin dispositivo (constantes en `builtin`) |
| 15 | Rendimiento P1: lectura HID bloqueante en hilo lector + `builtin` por plazos (33→0 y 2,5→0 despertares/s), quitar mapa `latest`, pre-escalar imágenes | pendiente; toca `app.rs`, validar con dispositivo (pulsaciones) |
| 16 | Rendimiento P2: unificar `base64`/`getrandom`, `TerminateProcess` directo, hilo RPC persistente, celdas laterales 16–18 | pendiente; sacar el editor a otro binario (−375 KB) es decisión de producto |

## B. Pruebas con dispositivo y OK del usuario

1. Parar Stream Dock (`schtasks /end /tn StreamDock`); `orcadeck run` con el perfil Orca y comprobar
   pintado, títulos, estados, feedback y pulsaciones.
2. `orcadeck edit` con `run` vivo: comprobar `ctl.rs`, recarga en caliente, Scene Shift, Property
   Inspector e instalación de un plugin desde Descargas.
3. Medir exe, RSS y CPU en reposo; anotarlos en `STATUS.md`.
4. Activar el autoarranque (consola elevada) y reiniciar sesión.

## C. Pulsación larga / pulsar-soltar por firmware (NO flashear)

Contexto y veredicto: [`../research/firmware/patch-emulation.md`](../research/firmware/patch-emulation.md)
y [`../research/firmware/updatetool-origin.md`](../research/firmware/updatetool-origin.md). Orden
seguro, solo con orden explícita y plan de recuperación:

1. Ensayo de reflasheo idéntico del original con `orcadeck hidwatch` corriendo para ver
   `EEEF:2929`; demuestra que el bootloader entra en modo actualización y vuelve al original.
2. Identificar el chip y buscar SWD/pin BOOT (abrir la unidad; fotos).
3. Solo entonces, valorar el parche v2 en una unidad de sacrificio; nunca con `APP`+`NEW`.
4. Alternativa sin riesgo: unidad AKP153E rev. 2 (PID `0x3010`), que sí emite down/up.
