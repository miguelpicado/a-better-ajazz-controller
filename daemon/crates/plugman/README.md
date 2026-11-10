# orcadeck-plugman

Instalador de plugins de Stream Deck/Stream Dock sin la app de Elgato. Es el port a Rust de
`tools/plugin-install/install.mjs` y lo usan el editor web y el binario `orcadeck-plugman`.

No descifra manifests del Marketplace. Cuando `manifest.json` empieza por `ELGATO`, reconstruye el
manifest funcional desde los ficheros en claro del plugin y lo guarda como **overlay** aparte; el
paquete original queda intacto.

## Ficheros

| Fichero | Responsabilidad |
|---|---|
| `src/lib.rs` | inspección, reconstrucción, instalación, overlays, watcher, límites de zip |
| `src/main.rs` | CLI `orcadeck-plugman` |
| `tests/plugman.rs` | fixtures de zip, manifests cifrados/planos, overlay, backup, traversal, límites |

## CLI

```text
orcadeck-plugman inspect <pluginDir|paquete> [--json]
orcadeck-plugman install <paquete> --plugins <dir> [--data <dir>] [--manifest <json>] [--force]
orcadeck-plugman watch <inboxDir> --plugins <dir> [--data <dir>] [--once]
orcadeck-plugman export --data <dir> --out <dir>
orcadeck-plugman import <dir> --data <dir>
```

- `--plugins` es la carpeta destino (`<uuid>.sdPlugin`).
- `--data` es la raíz de overlays (`<data>/manifests/<uuid>.json`). Por defecto, el padre de
  `--plugins`.
- `install --force` conserva la instalación anterior como `<uuid>.sdPlugin.bak-<timestamp>`.
- `watch --once` instala lo que haya y sale; sin `--once`, sondea la carpeta cada ~500 ms.
- `export`/`import` mueven los overlays a otra máquina.

El editor web no ejecuta el binario: llama a las funciones de la librería a través de su API.

## Flujo de instalación

1. Inspecciona el paquete (zip `.streamDeckPlugin` o carpeta `.sdPlugin`):
   - detecta el UUID (manifiesto o nombre de carpeta);
   - lee `manifest.json`: `Plain`, `Encrypted { size }`, `Missing` o `Invalid`;
   - si es cifrado, reconstruye desde `en.json`, código, iconos y, como último recurso, el registro
     de Stream Dock (`HKCU\Software\HotSpot\StreamDock\<uuid>.sdPlugin`).
2. Copia a `<plugins>/<uuid>.sdPlugin`.
3. Si el manifest es cifrado y la confianza es `full`, escribe
   `<data>/manifests/<uuid>.json`. Si el usuario aporta un manifest (`--manifest`), lo usa como
   overlay (`provided`).
4. Si la confianza es `partial`/`none`, el plugin se copia pero `needs_manifest = true`: no se debe
   lanzar hasta tener un manifest del fabricante.
5. Si una reinstalación no genera overlay, borra el overlay viejo (`removed_overlay`): ya no describe
   al plugin nuevo.

### Confianza de la reconstrucción

| Valor | Significado |
|---|---|
| `plain` | el manifest original es JSON; se usa tal cual |
| `full` | cifrado, reconstruido con `CodePath` y al menos una acción; se escribe overlay |
| `partial` | cifrado, solo se encontró `CodePath` o acciones; sin overlay |
| `none` | nada utilizable; sin overlay |
| `provided` | manifest del fabricante aportado por el usuario |

## Seguridad y límites

- No descifra `ELGATO` ni extrae claves: solo detecta el magic y mira el tamaño.
- Nombres de entrada de zip: rechaza rutas absolutas, `:` (unidades y flujos alternativos NTFS) y
  componentes `..`.
- Límites de zip bomb: 20 000 entradas, 256 MiB por fichero, 512 MiB total y ratio 200:1 (el ratio
  solo se comprueba por encima de 1 MiB).
- El borrado de un plugin solo lo decide el editor, y solo para UUIDs que él instaló. `plugman` no
  borra plugins de Stream Dock.
- `--force` nunca borra sin dejar el `.bak-<timestamp>`.

## Tests

```powershell
cd daemon
cargo test --release -p orcadeck-plugman
```

Cubren zips de fixture, manifests cifrados y planos, reconstrucción, overlay, `force`/backup, overlay
obsoleto, límites de zip bomb, traversal e import/export. No tocan plugins reales ni comparan contra
el instalador Node.

## Relación con el instalador Node

`tools/plugin-install/install.mjs` sigue siendo la implementación de referencia y sirve para
inspeccionar paquetes a mano. `plugman` busca el mismo resultado en los 20 plugins de esta máquina;
si cambias una regla de reconstrucción, contrasta ambos y actualiza
`docs/design/plugin-install-handoff.md` si el contrato cambia.
