# Uso de OrcaDeck

`orcadeck` es el sustituto ligero de «Stream Dock AJAZZ» para el teclado AKP153E. Lee los perfiles
de Stream Dock tal cual y ejecuta plugins del SDK v2 (`.exe` y Node) y acciones Orca integradas.

- Arquitectura y por qué de los tiempos: [ARCHITECTURE.md](../design/ARCHITECTURE.md).
- Compilar y probar: [DEVELOPMENT.md](DEVELOPMENT.md).
- Fallos y mensajes de error: [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
- Estado y pendientes: [HANDOFF.md](../project/HANDOFF.md), [STATUS.md](../project/STATUS.md), [ROADMAP.md](../project/ROADMAP.md).

## 1. Antes de empezar

1. **Detén Stream Dock** si está en marcha: `orcadeck run` usa el mismo HID y el mismo puerto 18618.
   ```powershell
   schtasks /end /tn StreamDock
   ```
   Para devolverle el control: `schtasks /run /tn StreamDock`.
2. Localiza tu perfil:
   ```powershell
   Get-ChildItem "$env:APPDATA\HotSpot\StreamDock\profiles" -Directory
   ```
   El perfil es la carpeta `<ID>.sdProfile`; se pasa esa ruta, no el `manifest.json`.
3. Comprueba que el dispositivo aparece (solo lectura, no envía nada):
   ```powershell
   .\orcadeck.exe probe
   ```
   Con los monitores apagados el hub USB no alimenta el teclado y `probe` no lo encontrará.

## 2. Subcomandos

Todos se implementan en [`daemon/src/main.rs`](../../daemon/src/main.rs); `run` y `edit` delegan en
[`app.rs`](../../daemon/src/app.rs) y [`edit.rs`](../../daemon/src/edit.rs).

| Comando | Sintaxis | Qué hace | ¿Toca el dispositivo? |
|---|---|---|---|
| `render` | `render <perfil.sdProfile> <carpeta>` | Renderiza la escena a `keyNN.jpg` sin abrir el HID. Ideal para revisar un perfil. | No |
| `probe` | `probe` | Enumera el AKP153E, imprime el descriptor HID y el `N` deducido. Solo lectura. | No (solo lectura) |
| `scene` | `scene <perfil.sdProfile>` | Arranca el HID, limpia, pinta todas las teclas y manda un `STP`. | Sí |
| `run` | `run <perfil.sdProfile>` | La app completa: driver + host de plugins + acciones integradas. | Sí |
| `edit` | `edit [--port <N>] [--no-open]` | Levanta el editor web en loopback y lo abre en el navegador. | No |
| `bright` | `bright <0-100>` | Cambia el brillo (`LIG`). Por encima de 100 se recorta. | Sí |
| `watch` | `watch [modo]` | Init, `MOD` opcional y volcado crudo de la entrada; latido cada 10 s. Experimental. | Sí |
| `poll` | `poll [input\|feature]` | Lee `GET_REPORT` cada 30 ms y muestra cambios. Experimental. | Sí (lectura) |
| `hidwatch` | `hidwatch` | Enumera HID cada 100 ms, sin abrir ni escribir. Marca `EEEF:2929` (bootloader). | No (solo enumera) |

Notas:

- `render` crea la carpeta de salida si no existe y escribe `key01.jpg`…`key18.jpg`. Las teclas
  1–15 sin acción se pintan en negro; las laterales 16–18 solo si la escena las usa.
- `scene`, `watch` y `poll` hacen `init(80)`: `DIS`, `LIG` y `CLE ff`, así que **borran la pantalla**
  antes de seguir. `bright` solo manda `LIG`.
- `watch` acepta un modo numérico (`watch 2` envía `CRT MOD` con ese valor), pero en el AKP153E v1
  `MOD` no cambia el comportamiento: el evento sigue llegando al soltar y con `state = 0`.
- `poll feature` devuelve «Función incorrecta» en esta unidad; es esperado. `poll input` devuelve la
  cadena de versión y no refleja una tecla mantenida.
- No hay un `orcadeck --help` global. `edit --help` sí existe.

## 3. Flujo de uso

### 3.1 Uso normal

```powershell
# 1. Parar la app oficial (mismo HID y puerto 18618)
schtasks /end /tn StreamDock

# 2. Arrancar OrcaDeck con la escena Orca
cd daemon
.\target\release\orcadeck.exe run "$env:APPDATA\HotSpot\StreamDock\profiles\R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile"
```

`run` imprime `device ready, N = …` y queda en primer plano. Ctrl+C o cerrar la consola termina el
proceso (no hay parada ordenada de los plugins hijos). Si el dispositivo se desconecta, reintenta
cada 2 s y repinta la última imagen de cada tecla al volver.

Qué se lanza:

- El host de plugins en `127.0.0.1:18618`, fijo.
- Solo los plugins que usa la escena; si un plugin ya estaba vivo de una escena anterior, se
  reutiliza (no se reinicia) para no perder sus ajustes.
- El hilo de acciones integradas (agente, cuota, reloj, resumen), aunque la escena no las use.

### 3.2 Parar y volver a la app oficial

```powershell
# Ctrl+C en la consola de orcadeck, o cerrar la ventana
schtasks /run /tn StreamDock
```

No ejecutes `run` mientras Stream Dock está abierto: el `open` del HID o el `bind` del 18618 fallan.

## 4. Variables de entorno

Las rutas del demonio cuelgan de `%APPDATA%`; el editor mira además `%USERPROFILE%\Downloads` y el
autoarranque usa `%LOCALAPPDATA%`. Las variables `ORCADECK_*` se leen en el punto indicado; no hay un
fichero de configuración para ellas.

| Variable | Por defecto | Efecto | Dónde se lee |
|---|---|---|---|
| `ORCADECK_REPORT_LEN` | descriptor HID; si falla, 512 | Fuerza el tamaño `N` del payload de salida. Útil si el descriptor no se puede leer. | [`daemon/src/main.rs`](../../daemon/src/main.rs) |
| `ORCADECK_PLUGINS` | `%APPDATA%\HotSpot\StreamDock\plugins` | Carpeta de plugins para `run`, `render`, `scene` y `watch`; también la raíz de plugins del editor. | [`main.rs`](../../daemon/src/main.rs), [`edit.rs`](../../daemon/src/edit.rs) |
| `ORCADECK_PROFILES` | `%APPDATA%\HotSpot\StreamDock\profiles` | Carpeta de perfiles **solo para `edit`**. `run` recibe el perfil por argumento. | [`edit.rs`](../../daemon/src/edit.rs) |
| `ORCADECK_DATA` | `%APPDATA%\OrcaDeck` | `state.json` y overlays **solo para `edit`**. `run` usa siempre `%APPDATA%\OrcaDeck`. | [`edit.rs`](../../daemon/src/edit.rs) |
| `ORCADECK_BUILTIN_PLUGINS` | vacío; se añaden las carpetas `plugins` de Stream Dock si existen | Raíces de plugins de solo lectura extra para `edit`, separadas por `;`. | [`edit.rs`](../../daemon/src/edit.rs) |
| `ORCADECK_GAP_MS` | `400` | Mínimo de milisegundos entre lotes de pintado. Se lee al abrir cada sesión del dispositivo. **No bajar.** | [`app.rs`](../../daemon/src/app.rs) |
| `ORCADECK_TRACE` | desactivado (cualquier valor lo activa) | Escribe a stderr cada entrada HID y la duración de cada lote. | [`app.rs`](../../daemon/src/app.rs) |
| `ORCADECK_CMD` | desactivado | Hook de experimento de `watch`: `VERBO:0a=01,0b=00` (offsets y valores en hexadecimal). Envía un `CRT` y sigue. | [`main.rs`](../../daemon/src/main.rs) |
| `ORCADECK_SCENE` | desactivado | Con `ORCADECK_CMD`: tras el comando, envía `LIG`, `CLE`, las imágenes de ese perfil (con `STP` por tecla) y `CONNECT`. | [`main.rs`](../../daemon/src/main.rs) |
| `QUOTA_DIR` | `%APPDATA%\OrcaDeck\quota` (si no hay `APPDATA`: `<temp>\OrcaDeck\quota`) | Carpeta de `quota-status.json` y `.wake` para las teclas de cuota. | [`builtin/src/actions.rs`](../../daemon/crates/builtin/src/actions.rs) |
| `APPDATA` | la del usuario de Windows | Base de todas las rutas por defecto. | `main.rs`, `app.rs`, `edit.rs`, `orca.rs` |

`ORCADECK_CMD` y `ORCADECK_SCENE` son herramientas de sondeo, no opciones de uso normal. `watch`
también hace `init`, así que pueden aparecer imágenes o limpieza de pantalla.

## 5. `run` en detalle

- **Puerto**: 18618 fijo, sin opción. Si está ocupado, `run` falla al arrancar el host.
- **Perfil**: se lee al arrancar y al cambiar de escena. Las ediciones del editor no se recargan solas,
  salvo la recarga en caliente de una tecla cuando `edit` corre dentro de `run` (ver §7).
- **Escena Shift**: sí recarga el manifiesto de la escena destino (ver §6).
- **Lotes**: hasta el gap de espera entre lotes (400 ms por defecto, `ORCADECK_GAP_MS`). Es
  deliberado: el firmware pierde pulsaciones si se le pinta sin pausas.
- **Latido**: `CONNECT` cada 10 s.
- **Reconexión**: si el USB desaparece, `device: …; retrying in 2 s`; al volver, `init` y repintado.
- **Ajustes de plugin**: `setSettings` se escribe en el `manifest.json` de la celda con debounce de
  1 s; `setGlobalSettings` en `%APPDATA%\OrcaDeck\globals\<plugin>.json`.
- **Feedback**: `showOk`/`showAlert` superponen check o aspa 600 ms; luego vuelve la imagen actual.
- **Títulos**: máximo 14 caracteres; el resto se recorta con `…`.
- **Plugins `.html`**: no se lanzan. Los `.js` necesitan `node` en `PATH`.

## 6. Scene Shift

La acción `com.hotspot.streamdock.profile.rotate` («Scene Shift») la implementa el propio `run`, no
un plugin. En la escena Orca ocupa `2,0` (salto directo a Grabación) y `4,2` (salto directo a
Monitorización).

`Settings.ProfileUUID` decide el destino:

| Valor | Destino |
|---|---|
| ausente o `""` | siguiente escena |
| `"PreviousScene"` | escena anterior |
| cualquier otro id | esa escena (se acepta con o sin `.sdProfile`) |

- El orden es el de la lista del dispositivo en `%APPDATA%\OrcaDeck\state.json` (`order`). Si ese
  fichero aún no existe, se toma de `DataCache.db` (solo lectura). El id directo solo funciona si
  existe `<id>.sdProfile\manifest.json` y el id es seguro.
- «Siguiente/anterior» da la vuelta al llegar a un extremo. Si la escena actual no está en la
  lista, «siguiente» es la primera y «anterior» la última.
- Al cambiar: carga la escena, escribe `state.json.current`, retira los contextos viejos, coloca los
  nuevos, lanza los plugins que falten y repinta en un lote todas las teclas que puedan cambiar.
- No mata los plugins de la escena anterior: siguen vivos para conservar sus ajustes.
- Si el destino es la escena actual, no hace nada. No hay soporte de páginas múltiples:
  `PageIndex` se ignora.
- **Estado**: implementado y con tests unitarios; pendiente la prueba física con el teclado (ver
  [ROADMAP](../project/ROADMAP.md)). El diseño y las evidencias están en
  [`design/scene-shift.md`](../design/scene-shift.md).

Para cambiar el destino desde el editor: selecciona la tecla y usa **Ir a la escena** (Siguiente escena
o una escena concreta), o edita `Settings.ProfileUUID` en **Ajustes en JSON**. El desplegable no
ofrece «anterior»; para eso escribe `PreviousScene` a mano:

```json
{ "DeviceUUID": "", "ProfileUUID": "PreviousScene" }
```

Reinicia `run` si has añadido o quitado la tecla, y también si cambias el destino de una tecla de
la escena actual: `run` no relee esa escena hasta que vuelve a entrar en ella. Si cambias el destino
de otra escena, se lee al saltar a ella.

## 7. Editor web: `edit`

```powershell
cd daemon
.\target\release\orcadeck.exe edit
# o:
.\target\release\orcadeck.exe edit --port 8765 --no-open
```

- Escucha solo en `127.0.0.1`; el puerto por defecto es uno libre. Abre el navegador salvo
  `--no-open`.
- La URL impresa lleva un token de sesión; se cambia por una cookie `HttpOnly` y se exige en cada
  llamada a la API. No compartas esa URL.
- Ctrl+C detiene el servidor y libera el puerto.
- No toca el HID directamente y no necesita el teclado. Si `run` está vivo, `edit` se ejecuta dentro
  de `run` (named pipe `ctl.rs`); si no, arranca standalone. En el primer caso, guardar una celda
  recarga en caliente esa tecla; en el segundo, los cambios no se recargan.
- Dos vistas: **Teclas** (escenas, cuadrícula 5×3 + panel lateral, arrastrar acciones, inspector de
  título/JSON, brillo) y **Plugins** (estado, manifest, instalar/quitar).
- El brillo que guarda el editor es un stub: `run` arranca a 80 y el brillo real se cambia con
  `orcadeck bright`.
- El Property Inspector del plugin se carga en un iframe aislado (sandbox sin `allow-same-origin`)
  con token de un solo uso por `postMessage`; si el plugin no trae PI, el inspector muestra título
  y JSON de ajustes.

Para probar sin tocar tus perfiles reales, apunta el editor a copias:

```powershell
$env:ORCADECK_PROFILES = "C:\temp\perfiles-copia"
$env:ORCADECK_PLUGINS  = "C:\temp\plugins-copia"
$env:ORCADECK_DATA     = "C:\temp\datos-copia"
.\orcadeck.exe edit
```

## 8. Instalar y quitar plugins desde `edit`

El editor usa `orcadeck-plugman`; no necesita la app de Elgato ni descifra manifests del Marketplace.

### Doble clic en un paquete: `register`, `install`, `unregister`

```powershell
.\target\release\orcadeck.exe register              # una vez, sin admin
.\target\release\orcadeck.exe install C:\ruta\MiPlugin.streamDeckPlugin
.\target\release\orcadeck.exe unregister            # deshacer
```

- `register` escribe solo en `HKCU\Software\Classes` (usuario, sin admin): crea el ProgID
  `OrcaDeck.Plugin` con comando `«ruta de orcadeck.exe» install "%1"` y asocia las extensiones
  `.streamDeckPlugin` y `.sdPlugin`. A partir de ahí, doble clic sobre el paquete (o «Abrir con» →
  OrcaDeck) abre el editor en la página de instalación con ese paquete preparado.
- `install <paquete>` hace lo mismo desde la consola. Si `orcadeck run` está vivo, pide el editor
  por el named pipe (la instalación se ve en caliente); si no, arranca un editor independiente.
  Ctrl+C cierra el editor, igual que en `edit`.
- La página muestra nombre, UUID, versión, autor y si ya hay una versión instalada. **Nada se
  instala solo**: hay que pulsar **Instalar plugin** (y confirmar si se reemplaza). Después,
  arrastra sus acciones a una tecla desde la vista **Teclas**.
- La ruta del fichero no viaja en la URL: el servidor la guarda tras un token de un solo uso. Si
  instalas y recargas la página, el enlace ya no sirve.
- Si mueves `orcadeck.exe`, vuelve a ejecutar `register`.
- `unregister` borra el ProgID y devuelve a cada extensión el valor por defecto que tenía antes del
  primer `register` (se guarda como `PreviousDefault.<ext>` bajo el ProgID). Si Windows sigue
  abriendo otra app, elige OrcaDeck en «Abrir con…» o restablece la asociación en Configuración:
  Windows puede tener su propia elección por usuario (`UserChoice`) por encima de esta entrada.

1. Descarga el `.streamDeckPlugin` con tu sesión del navegador (Marketplace → Download). También vale
   un `.sdPlugin` (carpeta o zip) o un `.zip`. Con la asociación registrada, haz doble clic sobre el
   fichero y salta al paso 3.
2. En **Plugins**, pulsa **Buscar en Descargas**. Se escanea `%USERPROFILE%\Downloads` **solo al
   pulsar**: paquetes `.streamDeckPlugin`/`.zip` y carpetas `.sdPlugin` con manifest, modificados en
   los últimos 30 días, máximo 20. No hay vigilante ni polling.
3. Se inspecciona antes de instalar: UUID, acciones, si el manifest está cifrado y si hace falta un
   manifest manual.
4. **Instalar plugin** copia a `%APPDATA%\HotSpot\StreamDock\plugins\<uuid>.sdPlugin` (la misma
   carpeta que lee `run`). Si ya existe, el editor pide confirmación y reinstala con `force`,
   dejando `<uuid>.sdPlugin.bak-<timestamp>`.
5. Si el manifest es `ELGATO` y la reconstrucción es `full`, se escribe el overlay
   `%APPDATA%\OrcaDeck\manifests\<uuid>.json`. Si es `partial`/`none`, el plugin se copia pero no se
   lanza hasta aportar un manifest del fabricante con `orcadeck-plugman install --manifest <json>`.
6. **Quitar plugin** solo está disponible para plugins instalados por OrcaDeck (`state.json` →
   `installed`). No borra integrados ni plugins que ya estaban antes. Para quitar uno que `run` está
   usando, para `run` primero.

Los `.js` necesitan `node` en `PATH`; los `.html` no se ejecutan. Detalle del formato y del
instalador de referencia: [`design/plugin-install-handoff.md`](../design/plugin-install-handoff.md) y
[`tools/legacy/plugin-install/README.md`](../../tools/legacy/plugin-install/README.md).

## 9. Autoarranque

[`tools/autostart/orcadeck-task.ps1`](../../tools/autostart/orcadeck-task.ps1) registra una tarea de
usuario normal (sin admin) que arranca `orcadeck run` al iniciar sesión.

```powershell
# Copia el exe y libunwind.dll a %LOCALAPPDATA%\OrcaDeck\bin y registra la tarea DESACTIVADA
.\tools\autostart\orcadeck-task.ps1 install

# Actívala solo cuando la tarea de Stream Dock esté desactivada
# (esto último exige una consola elevada):
schtasks /change /tn StreamDock /disable
.\tools\autostart\orcadeck-task.ps1 enable

.\tools\autostart\orcadeck-task.ps1 status
.\tools\autostart\orcadeck-task.ps1 disable
.\tools\autostart\orcadeck-task.ps1 uninstall
```

- Perfil por defecto: `R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88` (la escena Orca). Se cambia con
  `-Profile <id>`.
- `-Build` apunta a `daemon\target\release` por defecto.
- Copia `libunwind.dll` junto al ejecutable: evita el error `-1073741515` al arrancar sin el `PATH`
  de llvm-mingw.
- Log: `%LOCALAPPDATA%\OrcaDeck\orcadeck.log` (se añade, nunca se rota).
- La tarea `OrcaDeck` se registra desactivada a propósito; activarla con Stream Dock activo rompe
  ambos (mismo HID y puerto 18618).

## 10. Reglas duras

- **No flashear el firmware.** No lanzar `FirmwareUpgradeTool.exe` con 4 argumentos (flashea al
  instante, sin confirmación), ni enviar `CRT\0\0APP NEW` ni comandos de actualización. Solo con
  orden explícita del usuario y plan de recuperación.
- **No ejecutar Stream Dock y OrcaDeck a la vez.**
- No borrar ni mover los ficheros grandes de la raíz del repositorio: son del usuario.
