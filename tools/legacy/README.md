# tools/legacy — archivado, sustituido por el daemon Rust

| Ruta | Qué era | Sustituido por |
|---|---|---|
| `com.miguelpicado.orcadock.sdPlugin/` | Plugin Node para Stream Dock (agente, cuota, reloj, resumen) | acciones `builtin` de `daemon/crates/builtin` (UUID `com.miguelpicado.orcadock.*`) |
| `deploy.ps1` | Despliegue del plugin Node en la app oficial | `orcadeck run` / `orcadeck edit` |
| `plugin-install/` | Instalador de plugins (Node) | `daemon/crates/plugman` y la pestaña de plugins de `orcadeck edit` |

No se mantienen. Se conservan por historial y por si hace falta consultar el comportamiento original.
