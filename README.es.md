# A Better AJazz Controller (ABAC)

> Versión en español. English: [README.md](README.md).

Sustituto ligero de «Stream Dock AJAZZ» para el teclado **AKP153E**. Un único ejecutable Rust
(`abac.exe`) hace de driver HID, host de plugins del SDK v2 y, solo bajo demanda, servidor del
editor web. Sin Qt, CEF, Electron ni runtime async; sin admin; sin UI residente.

El objetivo es la misma funcionalidad útil que la app oficial —y las teclas de Orca— con una fracción
del consumo: 0 % de CPU en reposo, memoria mínima, tráfico HID en lotes y la UI web levantada solo
mientras se edita.

## Qué aporta frente a la app oficial

Todo gratis y de código abierto, con el hardware de serie:

- **Modo de pintado DRA** (`ABAC_DRA=1`, experimental): teclas a **98×98 px** en vez de 85×85, con calibración por tecla en el editor, redibujado parcial (solo se envían las teclas que cambian) y el **panel lateral como una barra continua** (reloj, CPU, GPU, RAM, temperatura de CPU/GPU) en lugar de tres celdas sueltas. Ver [Uso §4b](docs/guide/USAGE.md).
- **Parche de firmware pulsar/soltar**: el firmware de serie solo informa al soltar. El [parcheador](docs/guide/FIRMWARE-PATCH.md) hace que el AKP153E envíe un informe al **pulsar** y otro al **soltar**, y así el software tiene pulsación corta y larga reales. Lo flasheas tú desde tu propia copia del firmware; no se distribuye ningún binario de AJAZZ.

## Estado

| Pieza | Estado |
|---|---|
| `abac run` (driver + host + acciones integradas) | probado en vivo con el perfil Orca (2026-11-10): pintado, spinner, pulsaciones y cuota |
| Acciones integradas: agente, cuota, reloj, resumen | funcionando; mismas UUID que el plugin Node |
| Editor web `abac edit` | funcionando en loopback; escenas, celdas, plugins, brillo y Property Inspector; `edit` corre dentro de `run` si está vivo (con fallback) |
| Scene Shift (`profile.rotate`) | mergeado en `main` (`scenes.rs`); pendiente la prueba física en el dispositivo |
| Instalación de plugins (`.streamDeckPlugin`, `.sdPlugin`) | funcionando desde `edit`, con escaneo de Descargas bajo demanda; reconstruye manifests cifrados sin descifrarlos |
| Property Inspector en el editor | servidor y iframe integrados en `edit`; pendiente la prueba con plugins reales |
| Parche de firmware pulsar/soltar | **flasheado y probado en una unidad** (firmware `V2.293S.00.003`): llegan informes de pulsar y de soltar; ver [FIRMWARE-PATCH](docs/guide/FIRMWARE-PATCH.md) |
| Modo de pintado DRA (teclas 98×98, barra lateral continua) | experimental; en uso diario en una unidad |

Detalle y pendientes: [ROADMAP](docs/project/ROADMAP.md) y [HANDOFF](docs/project/HANDOFF.md).

## Requisitos

- Windows 10/11 x64.
- El AKP153E conectado (los monitores encendidos: cuelga de su hub USB).
- Para compilar: Rust gnullvm y llvm-mingw portable. Ver [DEVELOPMENT](docs/guide/DEVELOPMENT.md).
- Para ejecutar: `libunwind.dll` en `PATH` o junto al `.exe`.

## Inicio rápido

```powershell
# 1. Compilar (ver DEVELOPMENT para el PATH de llvm-mingw)
cd daemon
cargo build --release

# 2. Parar la app oficial: comparten HID y puerto 18618
schtasks /end /tn StreamDock

# 3. Comprobar el teclado (solo lectura)
.\target\release\abac.exe probe

# 4. Arrancar con tu perfil
.\target\release\abac.exe run "$env:APPDATA\HotSpot\StreamDock\profiles\<ID>.sdProfile"

# 5. Editar escenas en el navegador (si `run` está vivo, dentro de `run`)
.\target\release\abac.exe edit
```

Al terminar: Ctrl+C en `run` y `schtasks /run /tn StreamDock` para devolver el control a la app
oficial. No ejecutes las dos a la vez.

## Comandos

| Comando | Para qué |
|---|---|
| `render <perfil> <carpeta>` | renderiza la escena a JPEG sin abrir el HID |
| `probe` | enumera el AKP153E y muestra el descriptor (solo lectura) |
| `scene <perfil>` | pinta una escena completa y sale |
| `run <perfil>` | la app completa: driver + plugins + acciones integradas |
| `edit [--port N] [--no-open]` | editor web local bajo demanda |
| `bright <0-100>` | brillo |
| `watch [modo]` | entrada HID cruda (experimental) |
| `poll [input\|feature]` | `GET_REPORT` periódico (experimental) |
| `hidwatch` | enumera HID y marca el bootloader (solo lectura) |

Sintaxis, variables `ABAC_*` y flujos completos: [docs/guide/USAGE.md](docs/guide/USAGE.md).

## Qué pinta

- **Agente Orca**: un agente principal por tecla (agente de primer nivel en el worktree raíz de un
  repo, no un worker de orquestación). Nombre y color del repo; icono según estado: spinner naranja
  trabajando, burbuja azul esperando respuesta, aviso rojo bloqueado o terminal caído, check verde
  terminado, punto gris idle. Abajo: tiempo en el estado y ⚙N workers vivos. Parpadea 20 s al pasar a
  done y 60 s al pasar a waiting. Pulsar enfoca su panel en Orca.
- **Cuota IA**: una cuenta por tecla desde `quota-status.json`: barras de 5 h, semanal y mensual. Es
  de solo lectura; pulsar fuerza un refresco.
- **Reloj** y **resumen**: celdas laterales; el resumen cuenta agentes working / waiting / done.
- **Scene Shift**: salta a otra escena o a la siguiente/anterior según `Settings.ProfileUUID`.

## Estructura

```text
daemon/                 workspace Rust y binario abac
  src/                  CLI, HID, render, perfiles, escenas, run, edit
  crates/host/          host de plugins (WebSocket 18618)
  crates/builtin/       acciones Orca integradas
  crates/plugman/       instalador de plugins y overlays
  crates/web/           servidor y SPA del editor
docs/                   guías, diseño, estado e investigación (índice en docs/README.md)
  guide/                uso, desarrollo y diagnóstico
  design/               arquitectura vigente y diseños
  project/              estado, roadmap, handoff y auditoría
  research/             evidencias por tema (hid, firmware, plugins, perfiles, comunidad)
tools/                  autoarranque y legacy/ (plugin Node e instalador de referencia)
vendor/                 paquetes de terceros del usuario (gitignorado)
```

## Documentación

- [docs/README.md](docs/README.md) — índice completo.
- [docs/guide/USAGE.md](docs/guide/USAGE.md) — uso, subcomandos, variables, Scene Shift, plugins, autoarranque. English: [USAGE.en.md](docs/guide/USAGE.en.md).
- [docs/guide/TROUBLESHOOTING.md](docs/guide/TROUBLESHOOTING.md) — errores y diagnóstico. English: [TROUBLESHOOTING.en.md](docs/guide/TROUBLESHOOTING.en.md).
- [SECURITY.md](SECURITY.md) — política de seguridad y reporte privado.
- [CONTRIBUTING.md](CONTRIBUTING.md) — cómo contribuir.
- [docs/project/ROADMAP.md](docs/project/ROADMAP.md) — hoja de ruta.
- [docs/design/ARCHITECTURE.md](docs/design/ARCHITECTURE.md) — hilos, canales, lotes, formato de escena, estado.
- [docs/guide/DEVELOPMENT.md](docs/guide/DEVELOPMENT.md) — compilar, tests, probar sin dispositivo.

## Reglas

- **Flashear el firmware es bajo tu responsabilidad**: `FirmwareUpgradeTool.exe` con 4 argumentos flashea al instante y `CRT\0\0APP NEW` pone la unidad en el bootloader. Sigue la guía [FIRMWARE-PATCH](docs/guide/FIRMWARE-PATCH.md) y conserva tu `AKP153E.bin` original.
- **No ejecutar Stream Dock y ABAC a la vez**: mismo HID y puerto 18618.
- Los paquetes de terceros del usuario (zips, `.rar`, instalador y carpeta del driver) están en
  `vendor/`, fuera de git: no se tocan ni se commitean.

## Legado y créditos

`tools/legacy/com.miguelpicado.orcadock.sdPlugin/` es el plugin Node original para la app oficial
(agente, cuota, reloj y resumen). Sus acciones se migraron a `abac-builtin`; el plugin sigue en
el repo como referencia y para usar con Stream Dock. `tools/legacy/deploy.ps1` lo copia a la app
oficial y `node tools/legacy/com.miguelpicado.orcadock.sdPlugin/test.js` ejecuta sus tests.
`tools/streamdock/` contiene el alta puntual del perfil y los manifiestos de ejemplo;
`tools/legacy/plugin-install/` es el instalador Node de referencia.

Los iconos del plugin Node (`tools/legacy/com.miguelpicado.orcadock.sdPlugin/imgs/`) vienen de
[1am/orca-agent-deck](https://github.com/1am/orca-agent-deck) (MIT).

## Aviso legal y firmware

Proyecto independiente y no oficial, sin relación con AJAZZ, HotSpot, Elgato ni Orca. Software y herramientas de
firmware se ofrecen **«tal cual», sin garantía alguna y bajo tu responsabilidad**: los autores **no se hacen
responsables de daños** al dispositivo, al equipo, a los datos o a las cuentas. El firmware es **muy
experimental**: el parche de pulsar/soltar se ha probado en **una** unidad, pero flashear puede dejar el dispositivo inservible.
No se distribuye firmware del fabricante: el parcheador trabaja sobre tu propio volcado.
El software está diseñado exclusivamente para el AKP153E; el protocolo debería servir para otros AJAZZ, pero no
está verificado. Detalle: [README.md](README.md) (English) y [docs/research/firmware/README.md](docs/research/firmware/README.md).

## Licencia

`MIT OR Apache-2.0` a elección de quien lo use ([`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)).
Todas las dependencias son permisivas; sus avisos están en [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

Procedencia: el código es propio. De los repositorios de la comunidad analizados
([`docs/research/community/`](docs/research/community/)) solo se usaron datos fácticos del protocolo
(VID/PID, mapa de teclas, offsets); no se copió código de los licenciados con GPL-3.0, MPL-2.0 ni sin licencia.

