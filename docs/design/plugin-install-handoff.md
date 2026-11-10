# Handoff: instalación nativa de plugins de Stream Deck

> **Nota (2026-11-10):** portado a `orcadeck-plugman` e integrado en `edit`; se conserva como contrato
> funcional del instalador. Estado en [`../project/STATUS.md`](../project/STATUS.md).

Fecha: 2026-11-09. Para el worker que está construyendo la app (demonio Rust).
Documento autocontenido. La implementación de referencia está en
`tools/legacy/plugin-install/install.mjs`; las evidencias de investigación, en
`docs/research/plugins/elgato-plugin-encryption.md`, `docs/research/profiles/manifest-profiles-admin.md` y
`docs/research/plugins/plugin-host-protocol.md`.

Si eres el worker: §0–§6 son el contexto y las decisiones; §7–§9 son el contrato de implementación;
§11 es el orden de trabajo sugerido; §13 es el criterio de aceptación.

## Encargo en una frase

Integrar en la app un gestor de plugins que instale paquetes `.streamDeckPlugin` sin la app de
Elgato, reconstruya el manifest de host cuando venga cifrado y sepa qué plugins necesitan un
manifest manual. **No descifrar manifests ni extraer claves de Elgato.**

## 0. Estado actual (lo que ya está probado)

- Instalador nativo en Node: `tools/legacy/plugin-install/install.mjs`.
- Acepta `.streamDeckPlugin` (zip) o una carpeta `.sdPlugin`.
- Detecta manifests cifrados por el magic `ELGATO` y **no los descifra**.
- Reconstruye el manifest de host desde ficheros en claro y lo guarda como overlay.
- Modo `watch` para instalar automáticamente lo que caiga en una carpeta (probado en vivo).
- `inspect` acepta carpeta o zip; sirve para verificar una descarga del navegador.
- `export`/`import` de overlays para máquinas nuevas.
- Resultado en esta máquina: **5/5 plugins cifrados reconstruidos a `full`**:

| Plugin | Acciones | Fuente principal |
|---|---:|---|
| `com.elgato.camerahub` | 18 | `en.json` |
| `com.elgato.obsstudio` | 19 | `en.json` |
| `com.elgato.volume-controller` | 11 | `en.json` |
| `com.barraider.advancedlauncher` | 4 | literales del `.exe` (registro opcional) |
| `com.vivremotion.windowscockpit` | 1 | registro de Stream Dock / overlay |

- Tests: `node --test tools/legacy/plugin-install/install.test.mjs` → 4/4.

## 1. Decisión de diseño: no se descifra el manifest

Los plugins del Marketplace llevan `manifest.json` cifrado (`ELGATO` + versión + tamaño + sobre).
El esquema es `x25519-hkdf-sha256-aes-256-gcm`; la clave privada no está en el plugin y parece
ligada a la app/cuenta. Descifrarlo o extraer la clave es eludir una medida de protección, no
interoperabilidad. Esta decisión es deliberada y no debe revertirse sin una decisión explícita del
responsable.

Lo que sí se hace: reconstruir el manifest **funcional** (UUIDs de acciones, `CodePath`, nombres,
estados, PIs) desde los ficheros en claro del propio plugin. Es lo mismo que ya se hizo a mano con
Insta360 (`tools/streamdock/insta360-manifest.json`).

Consecuencia: el host nunca modifica el paquete descargado. Guarda el manifest reconstruido aparte.
El plugin sigue siendo byte-idéntico al que el usuario obtuvo, y las actualizaciones de Elgato no
rompen nada.

## 2. Flujo de instalación

```
paquete .streamDeckPlugin (zip)
  └─ <uuid>.sdPlugin/
       ├─ manifest.json          (plano o ELGATO cifrado)
       ├─ en.json                (textos/acciones, si existe)
       ├─ bin/plugin.js | index.html | source/main.html | *.exe
       └─ imgs/, pi/, ...
```

Pasos:

1. Validar el zip: rechazar entradas con ruta absoluta, letra de unidad o `..`.
2. Extraer a una carpeta temporal.
3. Localizar la carpeta `*.sdPlugin` que contiene `manifest.json`.
4. Copiar a `<pluginsRoot>/<uuid>.sdPlugin`.
   - Si ya existe y no hay `--force`, error.
   - Con `--force`, renombrar la anterior a `<uuid>.sdPlugin.bak-<timestamp>` y copiar la nueva.
5. Leer `manifest.json`:
   - JSON plano → usarlo tal cual; no hay overlay.
   - `ELGATO` → reconstruir; si `confidence === 'full'`, escribir
     `<dataRoot>/manifests/<uuid>.json`.
   - Si no es `full`, no escribir overlay: el plugin queda instalado pero marcado
     `needsManifest`.
6. `--manifest <json>` permite saltarse la reconstrucción y usar un manifest plano publicado por
   el vendor. Se escribe como overlay y `confidence = 'provided'`.

### 2.1 Detección del cifrado

```js
const ELGATO_MAGIC = Buffer.from('ELGATO');
if (buf.subarray(0, 6).equals(ELGATO_MAGIC)) encrypted = true;
```

No hay que interpretar el resto del fichero. No se intenta descifrar.

### 2.2 Formato del overlay

Es un manifest de Elgato normal, con los campos que el host necesita:

```jsonc
{
  "UUID": "com.elgato.camerahub",
  "Name": "Camera Hub",
  "Version": "1.0.0",
  "SDKVersion": 2,
  "CodePath": "index.html",
  "PropertyInspectorPath": "pi/index.html",
  "Actions": [
    {
      "UUID": "com.elgato.camerahub.selectwebcam",
      "Name": "Select Camera",
      "Tooltip": "...",
      "Icon": "images/key/selectCamera",
      "States": [{ "Name": "..." }],
      "Controllers": ["Keypad"]
    }
  ]
}
```

El host debe cargar el overlay **en lugar de** `manifest.json` cuando el original empieza por
`ELGATO`. La carpeta del plugin no se toca.

## 3. Reconstrucción: fuentes y prioridad

### 3.1 Action UUIDs

Orden de recogida (se unen; las dos primeras son autoritativas):

1. **`en.json`** (o el primer locale disponible). Se aceptan claves que empiezan por `<uuid>.` y
   cuyo valor es un objeto con `Name`, `Tooltip` o `States`. Es la fuente más fiable y la que trae
   nombres, tooltips y estados.
2. **Registro de Stream Dock** (solo Windows):
   `HKCU\Software\HotSpot\StreamDock\<uuid>.sdPlugin`. Cada nombre de valor es un action UUID;
   el dato es `true`/`false` (visibilidad). Ver §5.
3. **Literales del código**, solo si las dos anteriores no dieron nada. Se buscan cadenas
   reverse-DNS con el prefijo del vendor (`com.<vendor>.`) en `.js`, `.mjs`, `.cjs`, `.html`,
   `.exe` y `.dll` (ASCII y UTF-16LE). Se descartan valores que terminen en extensiones de fichero
   (`.exe`, `.dll`, `.sdplugin`, `.json`, etc.).
   - Esto es lo que hace portable Advanced Launcher: sus 4 UUIDs están en el `.exe`.
4. **`--manifest`** del vendor: sustituye todo lo anterior.

### 3.2 `CodePath`

Se prueba en orden:

```
bin/plugin.js, bin/plugin.mjs, bin/plugin.cjs,
plugin.js, plugin.mjs, plugin.cjs,
index.html, source/main.html, main.html,
primer *.exe en la raíz,
main.html/index.html/plugin.js en un subdirectorio de primer nivel
```

Si el `CodePath` termina en `.js/.mjs/.cjs`, el overlay añade `"Nodejs": {"Version": "20"}`.

### 3.3 Nombres, iconos y property inspectors

- `Name`: `en.json[action].Name`; si no, nombre del PI sin `inspector`/`pi`; si no, el último
  segmento del UUID humanizado.
- `Tooltip`, `States`: de `en.json` si existen.
- `Icon`: búsqueda por nombre en `imgs/actions`, `images/actions`, `imgs`, `images`, `Images`,
  `resources/actions`, `resources/assets`, `assets`. Sin extensión (el host prueba extensiones).
- `PropertyInspectorPath`: búsqueda puntuada en `pi/`, `propertyinspector/`, `ui/`,
  `source/actions/`, `resources/actions/`. Para acciones hijas se prueba también el UUID padre.
- `Controllers`: `["Keypad"]` por defecto; añade `"Knob"` si el nombre de la acción contiene
  `dial`, `knob` o `encoder`. En el AKP153E no hay diales, así que es cosmético.

### 3.4 Confianza

| `confidence` | Significado | Acción del host |
|---|---|---|
| `plain` | manifest original en JSON | usar tal cual |
| `full` | `CodePath` + al menos 1 acción | escribir overlay y cargar |
| `partial` | solo una de las dos cosas | no escribir overlay; marcar `needsManifest` |
| `none` | no se pudo reconstruir | no cargar; marcar `needsManifest` |
| `provided` | manifest del vendor vía `--manifest` | escribir overlay y cargar |

## 4. Adquisición del paquete (Marketplace)

### 4.1 Lo que es público

- Catálogo: `GET https://mp-gateway.elgato.com/products?extension=plugins&limit=20&offset=0`
  devuelve `{count, next, previous, results[]}`. Cada producto trae `id`, `name`, `slug`,
  `org_name`, `description`, `app_icon_cdn`, `variants[]` (`id`, `is_free`, `price`).
- Búsqueda: `GET https://mp-gateway.elgato.com/search?q=<texto>` devuelve
  `{products: {Plugins: [...]}, makers: []}`.
- Esto sirve para buscar/descubrir y enseñar metadatos. **No** sirve para descargar.

### 4.2 Lo que requiere cuenta

La web del Marketplace usa `GET /api/proxy/purchase-link/<variantId>` para obtener dos enlaces:

- `deep_link`: abre la app de Elgato ("Open in Stream Deck").
- `direct_link`: descarga el `.streamDeckPlugin` ("Download").

Sin sesión iniciada, ese endpoint devuelve **401**. Con la sesión del navegador, el botón
**Download** hace `window.location = direct_link` y guarda el paquete. El paquete es el zip
estándar (`<uuid>.sdPlugin/` dentro); verificado con los `.streamDeckPlugin` que Elgato distribuye
en `Program Files\Elgato\StreamDeck\Distributables`.

**No construir** un cliente headless que suplante la sesión de la app. La vía aceptada es el
navegador ya autenticado + carpeta vigilada. Si en el futuro se añade un "resolver" de vendors,
que use solo URLs públicas (GitHub releases, web del fabricante, tienda de Stream Dock).

### 4.3 Flujo recomendado para el usuario

1. Iniciar sesión en `marketplace.elgato.com` (la sesión persiste).
2. Producto → **Download** (no "Open in Stream Deck").
3. `install.mjs watch "$env:USERPROFILE\Downloads" --plugins <pluginsRoot> --data <dataRoot>`.
4. El watcher espera a que el fichero deje de crecer (2 comprobaciones a 500 ms) y lo instala.
5. Para actualizar: descargar de nuevo; el watcher usa `force: true` y deja `.bak-<timestamp>`.

## 5. Registro de Stream Dock: qué es y qué no es

Clave:

```
HKCU\Software\HotSpot\StreamDock\<uuid>.sdPlugin
    <actionUuid>    REG_SZ    true|false
```

Ejemplo real:

```
HKCU\Software\HotSpot\StreamDock\com.vivremotion.windowscockpit.sdPlugin
    com.vivremotion.windowscockpit.plugin    REG_SZ    true
```

- Solo contiene action UUIDs y su visibilidad. **No** contiene nombres, iconos, `CodePath` ni el
  manifest.
- Es un caché histórico: existe porque Stream Dock vio el plugin en el pasado (cuando su manifest
  era legible). No es una fuente que el host pueda regenerar a voluntad.
- El instalador lo **lee** como respaldo, pero **no lo escribe**: es estado de UI de otra app.
- No es portable a una máquina nueva. La copia durable es el overlay JSON.

## 6. Máquina nueva: qué funciona y qué no

| Caso | Sin Stream Dock | Solución |
|---|---|---|
| Plugins con `en.json` (camerahub, obsstudio, volume-controller) | Sí | nada |
| Advanced Launcher | Sí | los UUIDs están en el `.exe` |
| Window Cockpit | No | exportar el overlay en la máquina vieja e importarlo, o manifest del vendor |
| Plugin sin locale, sin registro y sin literales | No | overlay exportado o manifest del vendor |

Window Cockpit es el único caso residual actual: su único action UUID
(`com.vivremotion.windowscockpit.plugin`) no aparece en ningún fichero del paquete; solo en el
registro/manifest cifrado. El paquete contiene `com.vivremotion.windowscockpit.sdPlugin` en los
`.exe` de proxy, pero eso es el nombre de la carpeta, no el UUID de la acción.

Comandos de traslado:

```powershell
# Máquina vieja, después de instalar los plugins
node tools/legacy/plugin-install/install.mjs export --data <dataRoot> --out .\overlays

# Máquina nueva
node tools/legacy/plugin-install/install.mjs import .\overlays --data <dataRoot>
```

Los overlays son metadatos funcionales (JSON), no código del plugin. Se pueden guardar en el repo
o en una copia de seguridad. No incluir plugins de terceros en el repo.

## 7. API del módulo (referencia para el port a Rust)

### Funciones exportadas

```js
readManifestState(pluginDir)
// { kind: 'plain', manifest } | { kind: 'encrypted', size }
// | { kind: 'missing' } | { kind: 'invalid', size }

reconstructManifest(pluginDir)
// { uuid, encrypted, confidence, manifest, codePath?, actionCount? }

inspectPackage(packagePath)          // carpeta o zip
// mismo shape que reconstructManifest

installPackage(packagePath, {
  pluginsRoot, dataRoot, force = false, manifestPath
})
// { uuid, dest, overlay, needsManifest?, ...result }

watchPackages(inboxDir, { once = false, ...installOptions })
// sin retorno; instala al aparecer *.streamDeckPlugin

exportOverlays(dataRoot, outDir)     // número de overlays
importOverlays(inDir, dataRoot)      // número de overlays
```

### CLI

```powershell
node tools/legacy/plugin-install/install.mjs inspect <pluginDir|package> [--json]
node tools/legacy/plugin-install/install.mjs install <package> --plugins <dir> [--data <dir>] [--manifest <json>] [--force]
node tools/legacy/plugin-install/install.mjs watch <inboxDir> --plugins <dir> [--data <dir>] [--once]
node tools/legacy/plugin-install/install.mjs export --data <dir> --out <dir>
node tools/legacy/plugin-install/install.mjs import <dir> --data <dir>
```

### Rutas sugeridas para la app

```
%APPDATA%\OrcaDeck\plugins\<uuid>.sdPlugin     plugins instalados
%APPDATA%\OrcaDeck\manifests\<uuid>.json       overlays reconstruidos
%APPDATA%\OrcaDeck\overlays\                   export/import manual
```

La ruta exacta la decide la app; el módulo solo recibe `pluginsRoot` y `dataRoot`.

## 8. Integración en el host (Rust)

### 8.1 Descubrimiento

Para cada `<pluginsRoot>/*.sdPlugin`:

1. Leer `<dir>/manifest.json`.
2. Si empieza por `ELGATO`, cargar `<dataRoot>/manifests/<uuid>.json`.
   - Si no existe, marcar el plugin como `NeedsManifest` y **no lanzarlo**.
3. Si es JSON plano, usarlo.
4. Descartar manifests con `PrivateAPI` (built-ins de Stream Dock) o sin `CodePath`.
5. `pluginId` = `manifest.UUID` o el nombre de la carpeta sin `.sdPlugin`.

### 8.2 Lanzamiento

- `.exe` → spawn directo, `cwd = pluginDir`.
- `.js/.mjs/.cjs` → `node` + `CodePath`; `Nodejs.Version` es advisory. La app debe traer o
  localizar un Node 20.
- `.html` → navegador oculto (fase posterior; ver `plugin-host-protocol.md`).
- Argumentos: `-port <P> -pluginUUID <U> -registerEvent registerPlugin -info <json>`.
- `info.application.version`: `max(Software.MinimumVersion, "6.9")`.
- `info.devices`: la rejilla real (AKP153E: 6 columnas × 3 filas).

### 8.3 Qué NO hacer

- No descifrar `manifest.json`.
- No extraer ni usar claves de Elgato.
- No suplantar la sesión de la app para descargar.
- No modificar la carpeta del plugin; el overlay va aparte.
- No ejecutar plugins con privilegios de administrador (ver `manifest-profiles-admin.md` §3).

## 9. Seguridad y límites

- **Validación del zip**: se rechazan rutas absolutas, letras de unidad y `..`. El port a Rust debe
  mantener esta comprobación (crate `zip`, validar cada `file.name()`).
- **Sin verificación de firma/checksums**: el manifest cifrado incluye checksums de Elgato, pero no
  podemos verificarlos sin la clave. El instalador confía en el origen del paquete. Si se quiere
  endurecer: hash del paquete + allowlist de vendors, o verificación de firma si el vendor la
  publica aparte.
- **Plugins como código no confiable**: el host los lanza como procesos; no elevar el host. Ya está
  en la arquitectura.
- **`tar`**: el prototipo usa `tar` (bsdtar en Windows 10+) para leer/escribir zips. En Rust, usar
  el crate `zip`; no depender de `tar`.
- **Registro**: solo Windows; el port puede usar `windows-registry` o `reg.exe`. Si no existe la
  clave, seguir sin ella.
- **Nombres/iconos/PIs**: heurísticos. Un nombre feo o un icono ausente no impide lanzar el plugin;
  el host debe tener un fallback visual.

## 10. Tests y fixtures

- `node --test tools/legacy/plugin-install/install.test.mjs` (4 tests):
  1. reconstrucción desde locale + `CodePath`;
  2. fallback a literales de código sin locale/registro;
  3. manifest plano intacto;
  4. instalación desde zip + overlay + `--force` + `.bak`.
- Para el port a Rust, replicar estos casos con fixtures sintéticos. **No** commitear binarios de
  plugins de terceros.
- Prueba manual recomendada antes de integrar: descargar un plugin con la cuenta real y ejecutar
  `inspect` sobre el `.streamDeckPlugin`. Debe decir `plain` o `encrypted` y listar acciones.

## 11. Trabajo pendiente (orden sugerido)

1. **Port del instalador a Rust** (`src/plugin_manager.rs`): zip, detección `ELGATO`, reconstrucción,
   overlay, `force` + backup.
2. **Carga de overlays en el host**: si `ELGATO`, leer `manifests/<uuid>.json`; si falta, estado
   `NeedsManifest` en la UI.
3. **UI**: instalar desde fichero, estado `installed/needs manifest/update`, botón "exportar
   overlays" / "importar overlays".
4. **Watcher**: vigilar `Downloads` o una carpeta de entrada; instalar al estabilizarse el fichero.
5. **Tests de integración**: lanzar los 5 plugins reconstruidos contra el host real y comprobar
   `registerPlugin`, `willAppear`, `keyDown`, `setImage`.
6. **Opcional**: resolver de vendors públicos (GitHub releases, web del fabricante) y catálogo
   público del Marketplace para buscar/instalar sin navegador.
7. **Opcional**: registro de overlays compartido (JSON por UUID + versión) para los casos sin
   pistas en el paquete.

## 12. Referencias

- `docs/research/plugins/elgato-plugin-encryption.md` — formato `ELGATO`, esquema, qué está cifrado.
- `docs/research/profiles/manifest-profiles-admin.md` §1 — evidencias del cifrado, opciones (a/b/c),
  reconstrucción, registro y perfiles.
- `docs/research/plugins/plugin-host-protocol.md` — protocolo WebSocket, lanzamiento, `info`, eventos.
- `docs/design/architecture-draft.md` — diseño del demonio, componente 5 (plugin manager).
- `tools/legacy/plugin-install/README.md` — uso del prototipo y estado de los 5 plugins.
- `tools/streamdock/insta360-manifest.json` — ejemplo manual de manifest reconstruido.

## 13. Criterio de aceptación

- Instalar un `.streamDeckPlugin` con manifest plano → plugin en `<pluginsRoot>`, sin overlay.
- Instalar uno con manifest `ELGATO` y datos reconstruibles → plugin + overlay `full`; el host lo
  lista y lo lanza.
- Instalar uno sin datos suficientes → plugin copiado, estado `NeedsManifest`, sin lanzar; la UI
  permite importar un overlay o un manifest del vendor.
- Actualizar con `--force` → `.bak-<timestamp>` y overlay actualizado.
- Exportar overlays en una máquina y importarlos en otra → los plugins `NeedsManifest` pasan a
  cargar.
- Ningún camino descifra el manifest ni escribe en la carpeta del plugin.
