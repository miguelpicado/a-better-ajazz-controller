# orcadeck-host

Host de plugins para OrcaDeck. Habla el protocolo del SDK v2 de Elgato/Stream Dock: escucha en un
WebSocket de loopback, lanza los procesos de los plugins, les envía eventos y entrega al demonio lo
que piden (imágenes, títulos, estados, ajustes, URLs).

No toca el hardware. El demonio coloca acciones y le pasa las pulsaciones; el host se encarga del
resto.

## Uso

```rust
use orcadeck_host::{discover, Config, Host, Instance, Output};

let host = Host::start(Config::default(), |out: Output| eprintln!("{out:?}"))?;
for plugin in discover(&["C:/plugins".into()], None) {
    host.launch(plugin)?;
}
let ctx = host.appear(Instance::new("com.example.plugin", "com.example.plugin.action", "AKP153E", 0, 0));
host.key(&ctx, true);
host.key(&ctx, false);
```

`Config::default()` usa el puerto **18618**, `node` como intérprete de `.js` y ningún origen de
navegador permitido. El demonio lo arranca con el dispositivo AKP153E (5 columnas × 3 filas) y sin
orígenes, porque el editor todavía no carga Property Inspectors.

## Ficheros

| Fichero | Responsabilidad |
|---|---|
| `src/host.rs` | servidor WebSocket, registro, routing de eventos, estados, Property Inspector, supervisión |
| `src/launch.rs` | argv del plugin, `-info`, UUID aleatorio, elección de `.exe`/node |
| `src/manifest.rs` | descubrimiento de `*.sdPlugin`, overlays de manifests cifrados |
| `tests/fake_plugin.rs` | plugin falso por WebSocket real |
| `tests/real_plugin.rs` | plugin Node real (se omite sin `node` o sin la carpeta) |

## Descubrimiento

`discover(roots, data_root)` recorre cada `*.sdPlugin` de cada raíz y:

- lee `manifest.json`; si empieza por `ELGATO`, usa `<data_root>/manifests/<uuid>.json`;
- omite los `PrivateAPI` (los implementa la app del fabricante), los que no tienen `CodePath` y los
  cifrados sin overlay;
- si dos raíces tienen el mismo id, gana la primera;
- devuelve `Plugin { id, dir, code_path, manifest }`.

`load(dir, data_root)` hace lo mismo para una carpeta concreta y explica el motivo con `Skip`.

## Lanzamiento y supervisión

`Host::launch(plugin)`:

- `.exe` se ejecuta directo; `.js`/`.mjs`/`.cjs` con `node`; `.html` devuelve `Unsupported`.
- Argumentos: `-port <puerto> -pluginUUID <32> -registerEvent registerPlugin -info <json>`.
- `cwd` = carpeta del plugin, sin consola (`CREATE_NO_WINDOW`).
- Un hilo `<plugin>-wait` bloquea en `Child::wait`. Si el proceso cae, se relanza con backoff 1, 2,
  4… 60 s; tras 60 s de vida el contador se reinicia. `Host::stop` mata por pid y no reinicia.
- Al registrarse, el plugin recibe `deviceDidConnect` y un `willAppear` por cada instancia suya.

Los contextos de las instancias y los UUID de lanzamiento son aleatorios (CSPRNG de 32 caracteres);
el plugin no puede elegirlos ni predecirlos.

## Eventos

| Dirección | Evento |
|---|---|
| host → plugin | `didReceiveSettings`, `didReceiveGlobalSettings`, `willAppear`, `willDisappear`, `keyDown`, `keyUp`, `deviceDidConnect`, `propertyInspectorDidAppear`, `propertyInspectorDidDisappear`, `sendToPropertyInspector`, `sendToPlugin` |
| plugin → host | `registerPlugin`, `setImage`, `setTitle`, `setState`, `showOk`, `showAlert`, `setSettings`, `getSettings`, `setGlobalSettings`, `getGlobalSettings`, `openUrl`, `logMessage`, `sendToPropertyInspector`, `sendToPlugin` |

`setImage`/`setTitle` con `target: 2` (pantalla de software) se ignoran. Los comandos no soportados
(`switchToProfile`, `sendToDevice`, `getScreenshot`, `setFeedback`, `setTriggerDescription`, etc.) se
descartan y se registran una sola vez.

## Salidas

El callback `Events` recibe `Output` en el hilo del socket, sin el lock del host:

| `Output` | Significado |
|---|---|
| `Image { key, image, state }` | imagen `data:` (base64 o SVG percent-encoded); el demonio la decodifica |
| `Title { key, title, state }` | título de la tecla |
| `State { key, state }` | cambio de estado; el demonio busca `States[state].Image` en el perfil |
| `ShowOk(Key)` / `ShowAlert(Key)` | feedback verde/rojo |
| `Settings { context, settings }` | ajustes de la acción; el demonio los persiste |
| `GlobalSettings { plugin, settings }` | ajustes globales; el demonio los persiste y los siembra al lanzar |
| `OpenUrl(String)` | abrir una URL `http(s)` en el navegador |

El host no interpreta imágenes, títulos ni estados: solo los transporta.

## Property Inspector

La mitad servidora está implementada:

- `Host::open_inspector(context)` emite un token de un solo uso;
- `registerPropertyInspector` con ese token asocia el socket al contexto y manda
  `propertyInspectorDidAppear` al plugin y `didReceiveSettings` al inspector;
- `setSettings`, `getSettings`, `setGlobalSettings`, `getGlobalSettings`, `sendToPlugin` y
  `sendToPropertyInspector` se enrutan entre plugin e inspector;
- al cerrar el socket se manda `propertyInspectorDidDisappear`.

Falta la integración en el editor web (iframe y llamada a `connectElgatoStreamDeckSocket`); está en
desarrollo. Ver `docs/research/inspector.md`. `run` arranca con `origins` vacío, así que un PI de
navegador no puede conectar todavía.

## Seguridad

- Escucha solo en `127.0.0.1`.
- Un socket de navegador se acepta únicamente si su `Origin` coincide **exactamente** con uno de
  `Config::origins`; los procesos nativos (sin `Origin`) siempre pasan. `"null"` (file:// o iframe
  sandboxed) se rechaza salvo que se liste.
- El Property Inspector necesita un token de un solo uso; el contexto no es una credencial.
- No hay CORS ni endpoints HTTP: solo WebSocket.

## Tests

```powershell
cd daemon
cargo test --release -p orcadeck-host
```

- `tests/fake_plugin.rs`: WebSocket de loopback real con un plugin falso; cubre registro,
  `willAppear`, teclas, ajustes, tokens de PI, `Origin`, reinicio con backoff, `stop` sin reinicio y
  reenvío de instancias.
- `tests/real_plugin.rs`: lanza el plugin Node `com.miguelpicado.orcadock.sdPlugin` si hay `node` y
  la carpeta; usa una copia temporal y `APPDATA`/`QUOTA_DIR` temporales. Se omite si faltan.
- Tests unitarios en `host.rs`, `launch.rs` y `manifest.rs`.

## Límites

- Sin plugins `.html` (haría falta WebView2/CEF).
- Sin `switchToProfile`, `sendToDevice`, `getScreenshot` ni `setFeedback`.
- Sin Property Inspector en el editor (solo la parte servidora).
- El host no instala logger: usa el crate `log`, pero el binario no inicializa ninguno, así que sus
  mensajes no se ven. El demonio imprime lo suyo por `eprintln`.
- Al matar el proceso padre, los hijos no se matan automáticamente en Windows.
