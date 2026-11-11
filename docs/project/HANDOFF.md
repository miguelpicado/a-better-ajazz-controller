# HANDOFF (2026-11-11)

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

`main` incluye animación de teclas (spinner 250 ms, aro, temporizador), cadencias nuevas, doble clic en plugins, bucle `builtin` por plazos, hilo lector HID opcional, tests de huecos y el fix de traversal de iconos. Gate: **135 tests verdes**; `orcadeck.exe` = 3 721 728 B. Todo sin probar en dispositivo. Cuidado: `BATCH_GAP_MS` bajó a 250 ms; si se pierden pulsaciones, `ORCADECK_GAP_MS=400`.

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
