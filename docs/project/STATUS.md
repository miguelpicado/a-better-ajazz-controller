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
| Tests | **135 verdes** con `cargo test --release --workspace` (medido 2026-11-11; 0 fallos) |
| Binario | `orcadeck.exe` = **3 721 728 B** (~3,55 MiB) recompilado el 2026-11-11 |

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
