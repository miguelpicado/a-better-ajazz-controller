# OrcaDeck

Sustituto ligero de «Stream Dock AJAZZ» para el teclado **AKP153E**. Un único ejecutable Rust
(`orcadeck.exe`) hace de driver HID, host de plugins del SDK v2 y, solo bajo demanda, servidor del
editor web. Sin Qt, CEF, Electron ni runtime async; sin admin; sin UI residente.

El objetivo es la misma funcionalidad útil que la app oficial —y las teclas de Orca— con una fracción
del consumo: 0 % de CPU en reposo, memoria mínima, tráfico HID en lotes y la UI web levantada solo
mientras se edita.

## Estado

| Pieza | Estado |
|---|---|
| `orcadeck run` (driver + host + acciones integradas) | probado en vivo con el perfil Orca (2026-11-10): pintado, spinner, pulsaciones y cuota |
| Acciones integradas: agente, cuota, reloj, resumen | funcionando; mismas UUID que el plugin Node |
| Editor web `orcadeck edit` | funcionando en loopback; escenas, celdas, plugins y brillo (persistido; el real va por `bright`) |
| Scene Shift (`profile.rotate`) | implementado y con tests; pendiente la prueba física |
| Instalación de plugins (`.streamDeckPlugin`, `.sdPlugin`) | funcionando; reconstruye manifests cifrados sin descifrarlos |
| Property Inspector en el editor | en desarrollo |
| Parche de firmware pulsar/soltar | diseñado y emulado; **nunca flasheado** |

Detalle y pendientes: [ROADMAP](docs/ROADMAP.md) y [HANDOFF](docs/HANDOFF.md).

## Requisitos

- Windows 10/11 x64.
- El AKP153E conectado (los monitores encendidos: cuelga de su hub USB).
- Para compilar: Rust gnullvm y llvm-mingw portable. Ver [DEVELOPMENT](docs/DEVELOPMENT.md).
- Para ejecutar: `libunwind.dll` en `PATH` o junto al `.exe`.

## Inicio rápido

```powershell
# 1. Compilar (ver DEVELOPMENT para el PATH de llvm-mingw)
cd daemon
cargo build --release

# 2. Parar la app oficial: comparten HID y puerto 18618
schtasks /end /tn StreamDock

# 3. Comprobar el teclado (solo lectura)
.\target\release\orcadeck.exe probe

# 4. Arrancar con tu perfil
.\target\release\orcadeck.exe run "$env:APPDATA\HotSpot\StreamDock\profiles\<ID>.sdProfile"

# 5. Editar escenas en el navegador (proceso aparte, no toca el HID)
.\target\release\orcadeck.exe edit
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

Sintaxis, variables `ORCADECK_*` y flujos completos: [docs/USAGE.md](docs/USAGE.md).

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
daemon/                 workspace Rust y binario orcadeck
  src/                  CLI, HID, render, perfiles, escenas, run, edit
  crates/host/          host de plugins (WebSocket 18618)
  crates/builtin/       acciones Orca integradas
  crates/plugman/       instalador de plugins y overlays
  crates/web/           servidor y SPA del editor
docs/                   guías, estado, diseño e investigación
tools/                  autoarranque, instalador Node de referencia, perfiles de ejemplo
com.miguelpicado.orcadock.sdPlugin/   plugin Node antiguo (legado)
```

## Documentación

- [docs/README.md](docs/README.md) — índice completo.
- [docs/USAGE.md](docs/USAGE.md) — uso, subcomandos, variables, Scene Shift, plugins, autoarranque.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — hilos, canales, lotes, formato de escena, estado.
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) — compilar, tests, probar sin dispositivo.
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) — errores y diagnóstico.

## Reglas

- **No flashear el firmware** ni lanzar `FirmwareUpgradeTool.exe` con 4 argumentos: flashea al
  instante. Tampoco enviar `CRT\0\0APP NEW`. Solo con orden explícita y plan de recuperación.
- **No ejecutar Stream Dock y OrcaDeck a la vez**: mismo HID y puerto 18618.
- Los ficheros grandes de la raíz (zips, instalador, `.rar`, carpeta del driver) son del usuario: no
  se tocan.

## Legado y créditos

`com.miguelpicado.orcadock.sdPlugin/` es el plugin Node original para la app oficial (agente, cuota,
reloj y resumen). Sus acciones se migraron a `orcadeck-builtin`; el plugin sigue en el repo como
referencia y para usar con Stream Dock. `deploy.ps1` lo copia a la app oficial y
`node com.miguelpicado.orcadock.sdPlugin/test.js` ejecuta sus tests. `tools/streamdock/` contiene el
alta puntual del perfil y los manifiestos de ejemplo; `tools/plugin-install/` es el instalador Node
de referencia.

Los iconos del plugin Node (`com.miguelpicado.orcadock.sdPlugin/imgs/`) vienen de
[1am/orca-agent-deck](https://github.com/1am/orca-agent-deck) (MIT).
