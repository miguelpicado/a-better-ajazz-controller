# Documentación de OrcaDeck

Índice. La documentación de uso y desarrollo describe el código actual; la de estado y la
investigación son registro de trabajo y pueden quedar desfasadas respecto a la última rama.

## Guías

| Documento | Qué contiene |
|---|---|
| [USAGE.md](USAGE.md) | subcomandos, variables `ORCADECK_*`, flujo de uso, Scene Shift, editor, plugins, autoarranque |
| [ARCHITECTURE.md](ARCHITECTURE.md) | workspace, hilos y canales, lotes de pintado, gap de 400 ms, formato de escena, dónde vive cada estado |
| [DEVELOPMENT.md](DEVELOPMENT.md) | entorno Windows/llvm-mingw, gate de tests, estructura de tests, probar sin dispositivo, reglas de firmware |
| [TROUBLESHOOTING.md](TROUBLESHOOTING.md) | `-1073741515`, dispositivo no encontrado, Stream Dock y puerto 18618, pulsaciones perdidas, plugins, escenas |

## Estado del proyecto

Los actualiza el coordinador; no son documentación de referencia del código.

| Documento | Qué contiene |
|---|---|
| [HANDOFF.md](HANDOFF.md) | contexto de la sesión, hallazgos de hardware, medidas, encargos |
| [STATUS.md](STATUS.md) | estado del proyecto, sondeo en vivo, entorno y pendientes en orden |
| [ROADMAP.md](ROADMAP.md) | qué falta para dar la app por terminada y qué se investiga después |

## Diseño

| Documento | Qué contiene |
|---|---|
| [design/plugin-install-handoff.md](design/plugin-install-handoff.md) | contrato del instalador nativo: no descifrar, reconstruir overlay, `plugman` |
| [design/architecture.md](design/architecture.md) | borrador inicial del demonio (histórico; la arquitectura vigente es [ARCHITECTURE.md](ARCHITECTURE.md)) |
| [research/inspector.md](research/inspector.md) | diseño del Property Inspector y de `edit` dentro de `run` (**en desarrollo**) |
| [research/scene-shift.md](research/scene-shift.md) | diseño de Scene Shift (`profile.rotate`), evidencia y riesgos |

## Investigación

| Documento | Qué contiene |
|---|---|
| [research/hid-protocol.md](research/hid-protocol.md) | protocolo HID del AKP153E, comandos, imágenes, entrada |
| [research/keystate-modes.md](research/keystate-modes.md) | por qué el firmware solo informa al soltar; desensamblado |
| [research/plugin-host-protocol.md](research/plugin-host-protocol.md) | WebSocket 18618, manifiestos, lanzamiento y eventos del SDK v2 |
| [research/manifest-profiles-admin.md](research/manifest-profiles-admin.md) | manifests cifrados, formato de perfiles, `DataCache.db`, elevación |
| [research/elgato-plugin-encryption.md](research/elgato-plugin-encryption.md) | registro del cifrado de manifests de Elgato |
| [research/community-ideas.md](research/community-ideas.md) | ideas y repos de terceros (licencias y qué se puede reutilizar) |
| [research/firmware-feasibility.md](research/firmware-feasibility.md) | viabilidad del parche de pulsar/soltar |
| [research/firmware-v3-search.md](research/firmware-v3-search.md) | búsqueda de firmware V3 y alternativas de hardware |
| [research/firmware/patch-emulation.md](research/firmware/patch-emulation.md) | emulación del parche v2 y veredicto (**no flashear**) |
| [research/updatetool-analysis.md](research/updatetool-analysis.md) | análisis de las herramientas de actualización |
| [research/updatetool-origin.md](research/updatetool-origin.md) | corrección: `FirmwareUpgradeTool.exe` sí actualiza el AKP153E |
| [research/old-builds-search.md](research/old-builds-search.md) | búsqueda de builds antiguas e instaladores |

## Herramientas

| Documento | Qué contiene |
|---|---|
| [tools/plugin-install/README.md](../tools/plugin-install/README.md) | instalador de referencia en Node (`install.mjs`) |
| [tools/autostart/orcadeck-task.ps1](../tools/autostart/orcadeck-task.ps1) | tarea programada de autoarranque (comandos en [USAGE.md](USAGE.md#9-autoarranque)) |

## Código

Cada crate tiene su README:

- [`daemon/crates/host/README.md`](../daemon/crates/host/README.md)
- [`daemon/crates/builtin/README.md`](../daemon/crates/builtin/README.md)
- [`daemon/crates/plugman/README.md`](../daemon/crates/plugman/README.md)
- [`daemon/crates/web/README.md`](../daemon/crates/web/README.md)
