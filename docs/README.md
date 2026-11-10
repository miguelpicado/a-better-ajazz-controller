# Documentación de OrcaDeck

Índice. `guide/` y `design/ARCHITECTURE.md` describen el código actual; `project/` es el estado de
trabajo (puede quedar desfasado) y `research/` es el registro de evidencias y decisiones, no
documentación de referencia.

Carpetas: [guide](guide/README.md) · [design](design/README.md) · [project](project/README.md) ·
[research](research/README.md) ([hid](research/hid/README.md), [firmware](research/firmware/README.md),
[plugins](research/plugins/README.md), [profiles](research/profiles/README.md),
[community](research/community/README.md)).

## Guías

| Documento | Qué contiene |
|---|---|
| [guide/USAGE.md](guide/USAGE.md) | subcomandos, variables `ORCADECK_*`, flujo de uso, Scene Shift, editor, plugins, autoarranque |
| [guide/DEVELOPMENT.md](guide/DEVELOPMENT.md) | entorno Windows/llvm-mingw, gate de tests, estructura de tests, probar sin dispositivo, reglas de firmware |
| [guide/TROUBLESHOOTING.md](guide/TROUBLESHOOTING.md) | `-1073741515`, dispositivo no encontrado, Stream Dock y puerto 18618, pulsaciones perdidas, plugins, escenas |

## Diseño

| Documento | Qué contiene |
|---|---|
| [design/ARCHITECTURE.md](design/ARCHITECTURE.md) | arquitectura vigente: workspace, hilos y canales, lotes, formato de escena, estado |
| [design/inspector.md](design/inspector.md) | diseño del Property Inspector y de `edit` dentro de `run`; nota con lo implementado |
| [design/scene-shift.md](design/scene-shift.md) | diseño de Scene Shift (`profile.rotate`), evidencia y riesgos |
| [design/plugin-install-handoff.md](design/plugin-install-handoff.md) | contrato del instalador nativo: no descifrar, reconstruir overlay, `plugman` |
| [design/architecture-draft.md](design/architecture-draft.md) | borrador inicial del demonio (histórico; la arquitectura vigente es `design/ARCHITECTURE.md`) |

`plugin-install-handoff.md` se queda en `design/` por ser el contrato funcional que aplica
`plugman`, no historia resuelta.

## Estado del proyecto

| Documento | Qué contiene |
|---|---|
| [project/STATUS.md](project/STATUS.md) | estado verificado, medidas, límites y pendientes en orden |
| [project/ROADMAP.md](project/ROADMAP.md) | qué falta para dar la app por terminada y qué se investiga después |
| [project/HANDOFF.md](project/HANDOFF.md) | resumen corto y vigente para retomar el trabajo |
| [project/REPO-AUDIT.md](project/REPO-AUDIT.md) | auditoría del repositorio: hallazgos y plan P0/P1/P2 |
| [project/PERF-AUDIT.md](project/PERF-AUDIT.md) | auditoría de rendimiento de `run`/`edit`: despertares, memoria, tamaño; plan P0/P1/P2 |

## Investigación

| Documento | Qué contiene |
|---|---|
| [research/README.md](research/README.md) | índice de temas y criterio de conservación |
| [research/hid/hid-protocol.md](research/hid/hid-protocol.md) | protocolo HID del AKP153E, comandos, imágenes, entrada |
| [research/hid/keystate-modes.md](research/hid/keystate-modes.md) | por qué el firmware solo informa al soltar; desensamblado |
| [research/plugins/plugin-host-protocol.md](research/plugins/plugin-host-protocol.md) | WebSocket 18618, manifiestos, lanzamiento y eventos del SDK v2 |
| [research/plugins/elgato-plugin-encryption.md](research/plugins/elgato-plugin-encryption.md) | registro del cifrado de manifests de Elgato |
| [research/profiles/manifest-profiles-admin.md](research/profiles/manifest-profiles-admin.md) | manifests cifrados, formato de perfiles, `DataCache.db`, elevación |
| [research/community/community-ideas.md](research/community/community-ideas.md) | ideas y repos de terceros (licencias y qué se puede reutilizar) |
| [research/firmware/firmware-feasibility.md](research/firmware/firmware-feasibility.md) | viabilidad del parche de pulsar/soltar |
| [research/firmware/firmware-v3-search.md](research/firmware/firmware-v3-search.md) | búsqueda de firmware V3 y alternativas de hardware |
| [research/firmware/patch-emulation.md](research/firmware/patch-emulation.md) | emulación del parche v2 y veredicto (**no flashear**) |
| [research/firmware/updatetool-analysis.md](research/firmware/updatetool-analysis.md) | análisis de las herramientas de actualización (superado en parte por `updatetool-origin.md`) |
| [research/firmware/updatetool-origin.md](research/firmware/updatetool-origin.md) | corrección: `FirmwareUpgradeTool.exe` sí actualiza el AKP153E |
| [research/firmware/old-builds-search.md](research/firmware/old-builds-search.md) | búsqueda de builds antiguas e instaladores |
| [research/firmware/work/emulation-worker-prompt.md](research/firmware/work/emulation-worker-prompt.md) | encargo histórico del worker de emulación (material de análisis) |

## Herramientas

| Documento | Qué contiene |
|---|---|
| [tools/legacy/README.md](../tools/legacy/README.md) | archivo del plugin Node, `deploy.ps1` e instalador de referencia |
| [tools/legacy/plugin-install/README.md](../tools/legacy/plugin-install/README.md) | instalador de referencia en Node (`install.mjs`) |
| [tools/autostart/orcadeck-task.ps1](../tools/autostart/orcadeck-task.ps1) | tarea programada de autoarranque (comandos en [guide/USAGE.md](guide/USAGE.md#9-autoarranque)) |

## Código

Cada crate tiene su README:

- [`daemon/crates/host/README.md`](../daemon/crates/host/README.md)
- [`daemon/crates/builtin/README.md`](../daemon/crates/builtin/README.md)
- [`daemon/crates/plugman/README.md`](../daemon/crates/plugman/README.md)
- [`daemon/crates/web/README.md`](../daemon/crates/web/README.md)
