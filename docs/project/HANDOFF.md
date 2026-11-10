# HANDOFF (2026-11-10)

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

## Siguiente, en orden

1. Sin dispositivo: P0-P2 de [`REPO-AUDIT.md`](REPO-AUDIT.md), los huecos de tests de §4.2 y la
   retirada opcional de plugins sin uso al cambiar de escena (2 s de gracia).
2. Con dispositivo y OK del usuario: parar Stream Dock (`schtasks /end /tn StreamDock`), probar
   `run`, `edit`, Scene Shift, PI e instalación de plugins en vivo; medir exe/RSS/CPU; autoarranque.
3. Pulsación larga: solo investigación/emulación de firmware. **No flashear** sin plan de
   recuperación y orden explícita; alternativa segura: AKP153E rev. 2 (PID `0x3010`).

## Rutas clave

- Perfil Orca: `%APPDATA%\HotSpot\StreamDock\profiles\R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile`.
- Firmware original: [`../research/firmware/backup/AKP153E.bin`](../research/firmware/backup/AKP153E.bin).
- Evidencia del actualizador: [`../research/firmware/updatetool-origin.md`](../research/firmware/updatetool-origin.md).
- Compilar: [`../guide/DEVELOPMENT.md`](../guide/DEVELOPMENT.md).
