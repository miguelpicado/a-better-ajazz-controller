# Arquitectura de OrcaDeck

OrcaDeck sustituye a la app oficial «Stream Dock AJAZZ» para el AKP153E. Un único ejecutable Rust
(`orcadeck.exe`) hace de driver HID, host de plugins del SDK v2 y, solo cuando se pide, servidor del
editor web. Sin Qt, CEF, Electron ni runtime async; sin admin; sin UI residente.

Guías: [uso](USAGE.md) · [desarrollo](DEVELOPMENT.md) · [problemas](TROUBLESHOOTING.md).
Estado y trabajo pendiente: [HANDOFF](HANDOFF.md), [STATUS](STATUS.md), [ROADMAP](ROADMAP.md).
Diseño histórico (borradores anteriores a la integración): [design/architecture.md](design/architecture.md).

## 1. Principios

- **Un proceso para el uso normal.** `orcadeck run` es driver + host + acciones integradas.
- **UI web bajo demanda, en otro proceso.** `orcadeck edit` levanta el servidor, abre el navegador y
  muere al cerrar. `run` no sirve UI.
- **Hilos bloqueantes y canales.** Nada de tokio ni de polling de red; el objetivo es 0 % de CPU en
  reposo y memoria mínima.
- **Un solo escritor del HID.** Solo el hilo del dispositivo escribe al AKP153E. Todo lo demás pide
  pintados por un canal.
- **Sin dependencias nuevas** salvo necesidad medida. El perfil release usa `opt-level = "s"`, LTO,
  `panic = "abort"` y `strip`.
- **Se leen los perfiles existentes sin migrarlos.** El estado propio va a `%APPDATA%\OrcaDeck`.
- **Sin admin.** Se pierden temperaturas de CPU/GPU y las teclas enviadas a ventanas elevadas (UIPI).

## 2. Mapa del workspace

El workspace es [`daemon/Cargo.toml`](../daemon/Cargo.toml); el binario es `daemon/src/main.rs`.

| Ruta | Qué es | Responsabilidad |
|---|---|---|
| `daemon/` | binario `orcadeck` | CLI, protocolo HID, render, perfiles, escenas, `run`, `edit` |
| [`daemon/src/app.rs`](../daemon/src/app.rs) | `run` | hilos, canales, lotes de pintado, host, escenas |
| [`daemon/crates/host`](../daemon/crates/host) | `orcadeck-host` | WebSocket 18618, lanzamiento y supervisión de plugins, Property Inspector |
| [`daemon/crates/builtin`](../daemon/crates/builtin) | `orcadeck-builtin` | acciones Orca integradas (agente, cuota, reloj, resumen) |
| [`daemon/crates/plugman`](../daemon/crates/plugman) | `orcadeck-plugman` | instalación de plugins, reconstrucción de manifests cifrados |
| [`daemon/crates/web`](../daemon/crates/web) | `orcadeck-web` | servidor HTTP local, SPA del editor, `FsStore` |

`plugman` tiene además su propio binario, `orcadeck-plugman.exe`.

## 3. Proceso `run`

### 3.1 Arranque

1. `Scene::load` del `.sdProfile` indicado en la línea de comandos.
2. `SceneSet::open` lee el orden y la escena actual (`state.json`; si falta, se usa como respaldo
   `DataCache.db`/`StreamDockConfig.ini`, solo lectura).
3. `host::discover` recorre `%APPDATA%\HotSpot\StreamDock\plugins` (o `ORCADECK_PLUGINS`) y aplica los
   overlays de `%APPDATA%\OrcaDeck\manifests`.
4. `Host::start` enlaza `127.0.0.1:18618` y arranca el hilo aceptador.
5. Se crean `Runtime`, el canal de pintado y el canal de mensajes a `builtin`.
6. Nace el hilo `builtin`, que es el único dueño del `Registry`.
7. `place_scene` coloca cada acción: escena, acción integrada o plugin; guarda la imagen de
   placeholder y los estados del manifiesto.
8. Se lanzan solo los plugins que usa la escena.
9. El hilo principal entra en `session`; si el dispositivo se pierde, reintenta cada 2 s y repinta
   `latest` al reconectar.

### 3.2 Hilos

| Hilo | Dónde nace | Qué hace | En qué bloquea |
|---|---|---|---|
| principal / dispositivo | `app::run` → `session` | dueño del HID: entrada, `tick`, vaciado del canal, lotes, latido, reconexión | `read_timeout(30 ms)` |
| `builtin` | `app::run` | `Registry::tick` cada 400 ms, pulsaciones y reemplazo de escena; RPC a Orca | `recv_timeout(400 ms)` y el RPC |
| `host-accept` | `Host::start` | acepta conexiones WebSocket | `listener.incoming()` |
| `host-socket` | una por conexión | lee el WebSocket, enruta eventos, llama a `Events::output` | lectura del socket |
| `<plugin>-wait` | `Host::launch` | `Child::wait`; reinicio con backoff si el plugin cae | `Child::wait` |
| RPC (efímero) | `PipeOrca::call_once` | una llamada a Orca con timeout de 5 s | lectura de la tubería |
| editor | `orcadeck edit` | servidor HTTP en loopback (proceso aparte) | `listener.incoming()` |

No hay un hilo por feedback ni por pintado: los avisos ok/alert son una fecha límite en
`Runtime::restores` que el bucle del dispositivo comprueba en cada `tick`.

### 3.3 Canales y estado compartido

| Objeto | Productor | Consumidor | Contenido |
|---|---|---|---|
| `mpsc::Sender<Paint>` | hilos de socket de plugins y hilo `builtin` | bucle del dispositivo | `{key, imagen, generación, contexto}` |
| `mpsc::Sender<BuiltinMsg>` | bucle del dispositivo | hilo `builtin` | `Press(ctx)` y `Scene(Replace { old, new, ctx_keys })` |
| `Runtime` (`Arc`) | todos | todos | vista por tecla, generación, contextos retirados, restores, ajustes pendientes, manifiesto actual, carpeta de datos |
| `Host` (`Clone`) | `run` | hilos de socket | registro de plugins, instancias, Property Inspectors, procesos |

`Runtime` protege cada campo con su propio `Mutex`. Los hilos de plugin llaman a `Runtime::output`
sin el lock del host; el hilo del dispositivo es el único que escribe al HID.

Estado que **solo** toca el hilo del dispositivo: `by_key`, `latest`, `pending`, `last_batch`,
`hb`, `SceneSet`. Estado que **solo** toca el hilo `builtin`: `Registry` y su mapa `ctx_key`.

### 3.4 Bucle del dispositivo

```text
read_timeout(30 ms)
  ├─ HANLDER            → responder HAN
  ├─ tecla 1..15        → builtin: Press(ctx)
  │                       plugin: keyDown + keyUp
  │                       escena: switch_scene(...)
  ├─ Runtime::tick()    → restores de feedback y ajustes pendientes
  ├─ drenar Paint       → descartar generación vieja/contexto retirado; pending[key] = última
  ├─ si pending y ha pasado el gap → JPEG + BAT/trozos por tecla + un STP
  └─ cada 10 s          → CONNECT
```

`ORCADECK_TRACE=1` escribe a stderr cada entrada HID y la duración de cada lote.

### 3.5 Flujo de pintado

1. Un plugin manda `setImage`/`setTitle`/`setState`/`showOk`/`showAlert`; una acción integrada
   devuelve SVG en su `tick`.
2. `Runtime::output`/`paint` decodifica (`data:` base64 o SVG percent-encoded), actualiza
   `ViewState` y envía un `Paint`.
3. `ViewState` guarda la última imagen base por tecla, la barra de título rasterizada (solo se
   rehace si cambia el título), las imágenes de `States[]` y el último estado aplicado.
4. El bucle del dispositivo se queda con la **última** imagen de cada tecla y descarta los pintados
   de una escena anterior (generación) o de un contexto retirado.
5. Cada `batch_gap()` codifica todas las teclas pendientes y las envía en un solo lote: `BAT` +
   trozos por tecla, y **un** `STP` al final.
6. Al reconectar, `dock.init(80)` (DIS, LIG, CLE) y se reenvía `latest`: todas las teclas.

Composición sobre la imagen base:

| Señal | Efecto |
|---|---|
| `setTitle` | barra inferior negra translúcida (18/85 del alto), Segoe UI, máximo 14 caracteres + `…` |
| `setState` | repinta `States[state].Image` del perfil si existe; un estado repetido no repinta |
| `showOk` / `showAlert` | superpone check verde o aspa roja; 600 ms después vuelve a enviar la imagen actual |
| `setImage` | sustituye la imagen base; el título se recompone encima |

El feedback no crea hilos ni escribe directamente al HID: dos pintados (overlay y vuelta) pasan por
el mismo canal y el mismo mecanismo de lotes.

### 3.6 Por qué el gap de 400 ms

Medido con el dispositivo (ver [HANDOFF](HANDOFF.md)):

- Cada imagen cuesta **25–30 ms fijos** de escritura HID, independientemente de su tamaño.
- El firmware **pierde pulsaciones mientras recibe imágenes**. Pintar por tecla cada ~120 ms dio
  cero pulsaciones en 80 s, sin línea amarilla local.
- El arreglo es doble: **lotes** (todas las teclas pendientes juntas, para dejar huecos libres) y
  **JPEG q75** (~3 veces menos bytes que q100; el texto y las formas planas se ven bien).
- `ORCADECK_GAP_MS` (400 por defecto) fija el mínimo entre lotes. Se lee al abrir cada sesión del
  dispositivo. **No se baja**: si se pierden pulsaciones, se sube.
- Un cambio de escena pinta todas las teclas afectadas en un lote; una pulsación inmediata puede
  esperar hasta el siguiente hueco.

### 3.7 Pulsación

El firmware del AKP153E entrega **un solo evento por pulsación y al soltar**; el byte de estado
siempre es 0. `run` lo trata como un clic: las acciones integradas reciben `key_down` y los plugins
reciben `keyDown` seguido de `keyUp`. No hay pulsación larga ni distinguir pulsar/soltar con este
firmware (ver `research/keystate-modes.md`).

## 4. Escenas y perfiles

Un perfil es una carpeta `<id>.sdProfile` con `manifest.json`:

```json
{
  "Name": "Orca",
  "DeviceUUID": "AKP153E",
  "Actions": {
    "2,0": {
      "UUID": "com.hotspot.streamdock.profile.rotate",
      "Name": "Scene Shift",
      "State": 0,
      "States": [{ "Image": "ARROW_RIGHT.png" }],
      "Settings": { "ProfileUUID": "H33G4VTZ-25Z8-PYN7-R34M-ST39SX8TRT2Q" }
    }
  }
}
```

- `Actions` se indexa por `"col,row"`. En el AKP153E `col` va de 0 a 5 (5 = panel lateral) y `row`
  de 0 a 2.
- `State` indexa `States[]`; `States[].Image` es el placeholder estático. Los plugins repintan en
  vivo; la imagen del manifiesto es solo el marcador inicial.
- Resolución de una imagen: primero `<perfil>/Images/<nombre>`, después la carpeta del plugin dueño
  (el UUID de la acción empieza por `<plugin>.`) probando `""`, `.png`, `.jpg`, `.jpeg`.
- Las celdas laterales (16–18) se dibujan a 80×80; el resto a 85×85. En este hardware no son
  táctiles; se usan solo para mostrar reloj y resumen.

Mapa de coordenadas a índice de tecla (`daemon/src/proto.rs`):

| | col 0 | col 1 | col 2 | col 3 | col 4 | col 5 |
|---|---:|---:|---:|---:|---:|---:|
| row 0 | 13 | 10 | 7 | 4 | 1 | 16 |
| row 1 | 14 | 11 | 8 | 5 | 2 | 17 |
| row 2 | 15 | 12 | 9 | 6 | 3 | 18 |

El editor escribe los manifiestos de forma atómica (`.tmp` + rename) con la disposición de Qt
(claves ordenadas, 4 espacios, sin BOM) y conserva el anterior como `manifest.json.orcadeck.bak`.
`run` solo escribe `Settings` de la celda (con debounce de 1 s) y el `state.json` propio al arrancar
(si la escena actual no coincide) o al cambiar de escena; nunca toca `DataCache.db` ni el `.ini` de
Stream Dock.

## 5. Dónde vive cada estado

| Ruta | Contenido | Escribe | Lee |
|---|---|---|---|
| `%APPDATA%\HotSpot\StreamDock\profiles\<id>.sdProfile\manifest.json` | acciones, ajustes, imágenes de la escena | `edit`; `run` solo `Settings` | `run`, `edit` |
| `%APPDATA%\HotSpot\StreamDock\config\DataCache.db` | orden de escenas del dispositivo | Stream Dock | `edit`/`SceneSet` (solo lectura) |
| `%APPDATA%\HotSpot\StreamDock\config\StreamDockConfig.ini` | escena actual, brillo | Stream Dock | `edit`/`SceneSet` (solo lectura) |
| `%APPDATA%\OrcaDeck\state.json` | `order`, `current`, `brightness`, `installed` | `edit`, `SceneSet` | `edit`, `SceneSet` |
| `%APPDATA%\OrcaDeck\manifests\<uuid>.json` | overlay del manifest cifrado | `plugman` | `host::discover` |
| `%APPDATA%\OrcaDeck\globals\<plugin>.json` | ajustes globales del plugin | `run` | `run` al lanzar |
| `%APPDATA%\orca\orca-runtime.json` | tubería y token de Orca | Orca | `PipeOrca` |
| `%QUOTA_DIR%\quota-status.json` | cuotas por cuenta | `orca-dock-rs` | acciones de cuota |
| `%QUOTA_DIR%/.wake` | petición de refresco | acción de cuota | `orca-dock-rs` |
| `%LOCALAPPDATA%\OrcaDeck\bin` y `orcadeck.log` | binario copiado y log del autoarranque | `tools/autostart/orcadeck-task.ps1` | tarea programada |

`run` usa siempre `%APPDATA%\OrcaDeck` como carpeta de datos; `ORCADECK_DATA` solo afecta a `edit`
(ver [USAGE](USAGE.md#4-variables-de-entorno)).

## 6. Acciones integradas

Viven en `orcadeck-builtin`, en el hilo `builtin`, sin proceso ni WebSocket.

| UUID | Acción | Qué muestra | Pulsación |
|---|---|---|---|
| `com.miguelpicado.orcadock.agent` | agente | un agente principal de Orca por tecla: repo, color, estado, tiempo en el estado y ⚙N workers vivos | enfoca su panel en Orca |
| `com.miguelpicado.orcadock.quota` | cuota | una cuenta por tecla: 5 h, semanal y mensual, con barras de % restante | fuerza un refresco (`.wake`) |
| `com.miguelpicado.orcadock.clock` | reloj | HH:MM y fecha | ninguna |
| `com.miguelpicado.orcadock.summary` | resumen | agentes vivos en working / waiting / done | ninguna |

Cadencias: `tick` cada 400 ms; agentes cada 1 s; repos cada 30 s; `orchestration.workerList` cada
10 s (tarda ~450 ms); cuota cada 30 s; parpadeo 20 s al pasar a done y 60 s al pasar a waiting;
spinner de 400 ms por fotograma. `Registry::tick` devuelve solo las imágenes que cambian, así que
una tecla quieta no genera tráfico HID.

Los datos salen de Orca por la tubería con nombre descrita en `%APPDATA%\orca\orca-runtime.json`
(JSON por líneas, timeout de 5 s y un reintento tras releer el fichero). La cuota es de solo
lectura: `orca-dock-rs` es el dueño de `quota-status.json`; la tecla de cuota escribe `.wake` y
relee cada 3 s hasta 45 s. El hilo `builtin` existe precisamente para que el `workerList` lento no
frene el giro del spinner ni el pintado.

## 7. Host de plugins

`orcadeck-host` habla el protocolo del SDK v2 de Elgato y de Stream Dock. Escucha en
`127.0.0.1:18618` (fijo), igual que la app oficial: **nunca ejecutes las dos a la vez**.

- **Descubrimiento**: carpetas `*.sdPlugin`, manifiesto plano o overlay si empieza por `ELGATO`;
  se omiten los `PrivateAPI` (los implementa la app del fabricante), los que no tienen `CodePath` y
  los cifrados sin overlay.
- **Lanzamiento**: `.exe` directo; `.js`/`.mjs`/`.cjs` con `node`; `.html` no soportado. Argumentos
  `-port 18618 -pluginUUID <32> -registerEvent registerPlugin -info <json>`, con el directorio del
  plugin como `cwd` y sin consola.
- **Ciclo de vida**: `registerPlugin` con el UUID del lanzamiento, `deviceDidConnect`, `willAppear`,
  `keyDown`/`keyUp`, `willDisappear`. Si un plugin cae, se relanza con backoff 1, 2, 4… 60 s; el
  contador se reinicia tras 60 s de vida. `Host::stop` mata por pid y no reinicia.
- **Salidas** (`Output`): `Image`, `Title`, `State`, `ShowOk`, `ShowAlert`, `Settings`,
  `GlobalSettings`, `OpenUrl`.
- **Comandos atendidos**: `setImage`, `setTitle`, `setState`, `showOk`, `showAlert`, `setSettings`,
  `getSettings`, `setGlobalSettings`, `getGlobalSettings`, `openUrl`, `logMessage`,
  `sendToPropertyInspector`, `sendToPlugin`.
- **No atendidos** (se registran una vez y se descartan): `switchToProfile`, `sendToDevice`,
  `getScreenshot`, `setFeedback`, `setTriggerDescription`, etc.
- **Seguridad**: contextos y UUID de lanzamiento aleatorios (CSPRNG), `Origin` exacto para sockets
  de navegador, token de Property Inspector de un solo uso. `run` arranca con la lista de orígenes
  vacía: un PI de navegador no puede conectarse hasta que se integre el editor.
- **Property Inspector**: la mitad servidora está implementada (`open_inspector`,
  `registerPropertyInspector`, routing de ajustes), pero el editor web todavía no lo usa. Está
  **en desarrollo**; el diseño y sus riesgos están en `research/inspector.md`.

## 8. Editor web (`edit`)

Proceso aparte. `Server::start` enlaza `127.0.0.1` (puerto libre por defecto), genera un token de
256 bits y sirve una SPA embebida (`include_str!`, sin build ni fuentes web). El token viaja en la
URL, se cambia por una cookie `HttpOnly; SameSite=Strict` y se exige en cada `/api/*`.

- `FsStore` apunta a los perfiles, plugins, overlays, `state.json` y la configuración heredada.
- Dos vistas: **Teclas** (cuadrícula 5×3 + panel lateral, arrastrar acciones, escenas, inspector de
  título/JSON) y **Plugins** (estado, manifest, instalar/quitar).
- Sin HID y sin hot reload: los cambios se guardan en disco, pero `run` no los recarga. Cambiar de
  escena en `run` sí recarga el manifiesto de la escena destino.
- El brillo del editor es un stub persistido; el `run` arranca a 80 y `orcadeck bright` cambia el
  brillo real.
- Property Inspector en iframe: **en desarrollo**, no existe todavía en la SPA.
- En reposo el servidor duerme en `accept`: 0 % de CPU; la última medición anotada es ~7 MB RSS.

## 9. Instalador de plugins

`orcadeck-plugman` es el port a Rust de `tools/plugin-install/install.mjs`.

- Acepta `.streamDeckPlugin` (zip) o una carpeta `.sdPlugin`.
- **No descifra** un `manifest.json` que empieza por `ELGATO`; reconstruye el manifest funcional
  desde los ficheros en claro (`en.json`, código, iconos) y lo guarda como overlay en
  `<data>/manifests/<uuid>.json` solo si la confianza es `full`.
- `force` conserva la instalación anterior como `<uuid>.sdPlugin.bak-<timestamp>`.
- Límites de zip bomb: 20 000 entradas, 256 MiB por fichero, 512 MiB total, ratio 200:1.
- El editor solo permite quitar plugins que instaló él (`state.json.installed`); nunca un integrado
  ni uno que ya estaba antes.
- Casos sin overlay (`partial`/`none`) necesitan un manifest del fabricante; el plugin se copia pero
  no se lanza.

## 10. HID y render

- Dispositivo: AKP153E, `VID 0x0300 / PID 0x1010` (bootloader `0xEEEF/0x2929`).
- `N` (payload del reporte de salida): `ORCADECK_REPORT_LEN` si está; si no, se deduce del
  descriptor HID; si no, 512. Cada escritura antepone el ID de reporte `0x00`.
- Comandos: `CRT\0\0` + verbo. `DIS` despierta, `LIG` brillo, `CLE` borra, `BAT` cabecera de imagen
  (longitud BE16 + tecla), `STP` refresca, `CONNECT` latido cada 10 s.
- Secuencia de arranque observada en la app original: `DIS`, `LIG`, `CLE ff`, imágenes y **un**
  `STP`. El dispositivo no manda ACK de comandos ni de imágenes; no se espera ninguno.
- Imagen: redimensionado exacto a 85 px (teclas) u 80 px (laterales), rotación 270°, JPEG q75 que
  baja de 4 en 4 hasta ≤ 9000 bytes.
- Entrada: `HANLDER` → responder `HAN`; `ACK…OK`; `error`; `CRT MOD`; si no, tecla en el byte 9 y
  estado en el byte 10 (siempre 0 en este firmware).
- Protocolo detallado y dudas cerradas: `research/hid-protocol.md` y `research/keystate-modes.md`.

## 11. Límites conocidos

| Límite | Motivo |
|---|---|
| Sin pulsación larga ni down/up | el firmware v1 solo informa al soltar |
| Panel lateral no táctil y sin dial | hardware del AKP153E v1 |
| Plugins `.html` no soportados | necesitarían WebView2/CEF |
| Sin `switchToProfile`, `sendToDevice`, `getScreenshot` | no implementados en el host |
| Property Inspector en el editor | diseñado, aún en desarrollo |
| Sin recarga en caliente de `edit` | `run` lee el manifiesto al arrancar o al cambiar de escena |
| Plugins viejos no se matan al cambiar de escena | evita perder ajustes; consume RAM |
| Plugins de `Program Files` fuera de `run` | `run` solo descubre `%APPDATA%\HotSpot\StreamDock\plugins` u `ORCADECK_PLUGINS`; `edit` los lista, pero no se lanzan |
| Parpadeo ok/alert | 600 ms, dos pintados extra |
| Sin cierre ordenado de hijos al matar `run` | Windows no mata los procesos hijos del padre |
| Parche de firmware | diseñado y emulado, **nunca flasheado**; regla dura: no flashear |

El detalle y el orden de lo que falta están en [ROADMAP](ROADMAP.md).

## 12. Referencias

- [`daemon/src/app.rs`](../daemon/src/app.rs) — hilos, canales, lotes, escenas.
- [`daemon/src/proto.rs`](../daemon/src/proto.rs) — protocolo y mapa de teclas.
- [`daemon/src/render.rs`](../daemon/src/render.rs) — pipeline JPEG.
- [`daemon/src/profile.rs`](../daemon/src/profile.rs) — lectura de perfiles y escritura de ajustes.
- [`daemon/src/scenes.rs`](../daemon/src/scenes.rs) — orden y salto de escenas.
- [`daemon/crates/host/src/host.rs`](../daemon/crates/host/src/host.rs) — host y protocolo.
- [`daemon/crates/web/src/store.rs`](../daemon/crates/web/src/store.rs) — `FsStore`, `state.json`.
- `research/scene-shift.md`, `research/inspector.md`, `research/hid-protocol.md`,
  `research/plugin-host-protocol.md`, `design/plugin-install-handoff.md`.
