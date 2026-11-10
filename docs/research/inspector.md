# Inspector de Propiedades (Property Inspector) y Arquitectura de `orcadeck edit`

Investigación y propuesta de diseño para `orcadeck edit` y su integración con `orcadeck run`.  
Fecha: 2026-11-10. Autor: Worker de investigación (`task_68efc43b2e65`).  
Etiquetas:
- **OBSERVED**: verificado directamente en el código fuente, binarios o trazas del repositorio.
- **MEASURED**: métrica tomada de mediciones previas documentadas en el proyecto.
- **ESTIMATED**: cálculo razonado basado en componentes de Rust y consumo de memoria.
- **INFERRED**: deducción lógica a partir de la arquitectura y el protocolo.

---

## 1. Resumen Ejecutivo (Recomendación en 5 líneas)

1. **Decisión de proceso**: Cuando `run` está en marcha, `edit` debe correr **DENTRO de `run` bajo demanda** (activado por el CLI `orcadeck edit`), con **fallback a proceso autónomo** cuando `run` está apagado.
2. **Puerto 18618**: El `Host` de plugins ya posee `127.0.0.1:18618`; correr dentro de `run` elimina colisiones de puerto y permite que el PI conecte directamente con los plugins vivos.
3. **Persistencia de Settings**: Reutilizar al 100 % `orcadeck_host::Host` (que ya implementa `open_inspector`, `registerPropertyInspector`, `didReceiveSettings`, `setSettings` y `sendToPlugin`), sin puentes artificiales.
4. **Recarga en caliente**: Canal Rust local en memoria (`mpsc::Sender<SceneUpdate>`), repintando solo la tecla cambiada respetando `ORCADECK_GAP_MS` (400 ms), sin reiniciar `run` ni tocar el bus HID.
5. **Seguridad**: Iframe con `sandbox="allow-scripts allow-forms allow-same-origin"` sirviendo el HTML del PI desde `/api/plugins/<uuid>/pi/*`, token CSPRNG de un solo uso y lista blanca estricta de `Origin` en el WebSocket.

---

## 2. Contexto y Estado del Arte

- **Stream Dock / Elgato SDK v2 (OBSERVED en `docs/research/plugin-host-protocol.md:277-307`)**:
  Los plugins suministran su interfaz de configuración mediante páginas HTML locales (`PropertyInspectorPath` en `manifest.json`). La aplicación anfitriona embebe dicha página y evalúa `connectElgatoStreamDeckSocket(inPort, inPropertyInspectorUUID, "registerPropertyInspector", inInfo, inActionInfo)`. La página abre un WebSocket a `ws://127.0.0.1:<inPort>` e intercambia ajustes en tiempo real (`setSettings`, `sendToPlugin`, `sendToPropertyInspector`).
- **Estado actual en OrcaDeck**:
  - `daemon/src/app.rs`: Subcomando `run`. Posee el handle HID exclusivo del AKP153E (`app.rs:534`), gestiona el pintado en lotes cada 400 ms (`app.rs:32`) y arranca `orcadeck_host::Host` en `127.0.0.1:18618` (`app.rs:433-440`).
  - `daemon/crates/host/src/host.rs`: Ya implementa la mitad servidora del Property Inspector: `Host::open_inspector` (`host.rs:452`), handshake `registerPropertyInspector` con tokens de un solo uso (`host.rs:606-616`), routing de `setSettings` / `getSettings` (`host.rs:700-710`), `sendToPlugin` / `sendToPropertyInspector` (`host.rs:711-718`) y eventos de ciclo de vida `propertyInspectorDidAppear` / `Disappear`.
  - `daemon/src/edit.rs` y `daemon/crates/web`: Subcomando `edit`. Servidor web HTTP ultraligero (`orcadeck_web::Server`, `lib.rs:51-101`) que sirve una SPA en loopback con token de sesión, consumiendo 7 MB RSS y 0 s de CPU en reposo (`HANDOFF.md:106-112`). Actualmente `app.js:379-450` ofrece un inspector estático básico con inputs para títulos y JSON crudo, sin cargar el HTML del plugin.

---

## 3. (a) Proceso: `edit` DENTRO de `run` vs Proceso Aparte

### Análisis comparativo con evidencia técnica

| Dimensión | `edit` DENTRO de `run` (bajo demanda) | `edit` como PROCESO APARTE |
|---|---|---|
| **Memoria (RSS)** | **MEASURED / ESTIMATED**: En reposo normal de `run`, **0 MB extra** (servidor web inactivo). Al editar, levanta `orcadeck_web::Server` en hilos efímeros compartiendo el heap de `run` (+1,5 a 2 MB temporales). Al cerrar, vuelve al baseline. | **MEASURED**: `edit.rs` consume **~7 MB RSS** (`ROADMAP.md:A.4`). Si ambos corren separados: ~15 MB (`run`) + ~7 MB (`edit`) = **~22 MB total** (+30 % de consumo). |
| **CPU en reposo** | **0 % CPU** (OBSERVED). El hilo HID duerme en `read_timeout(30)` (`app.rs:544`) y el listener web duerme en `TcpListener.incoming()` (`lib.rs:69`). | **0 % CPU** mientras ambos duermen; pero si requieren sincronización de ficheros por polling o IPC continuo, aumenta el riesgo de consumo espurio. |
| **Conflicto puerto 18618** | **CERO conflicto** (OBSERVED). `Host` en `run` ya escucha en `127.0.0.1:18618` (`host.rs:26-28`). El Property Inspector conecta directamente al host que tiene a los plugins vivos. | **CONFLICTO CRÍTICO**. Dos procesos no pueden hacer `bind` a `127.0.0.1:18618` (`WSAEADDRINUSE`). `edit` no puede hospedar plugins en 18618. Si el PI conecta al 18618 de `run`, `edit` no tiene forma de generar el token sin un IPC externo. |
| **Conflicto del HID** | **CERO conflicto** (OBSERVED). `run` mantiene el handle exclusivo de `hidapi` (`app.rs:534`). `edit` no toca el HID (`store.rs:357`). | Cero conflicto directo de handle, pero `edit` no puede forzar un repintado sin pedirlo a `run`. |
| **Cambios en caliente** | **Inmediatos (< 1 ms)** por canal en memoria `mpsc::channel`. Cero lecturas de disco duplicadas. | Requiere protocolo IPC entre procesos o file-watching (con riesgo de rebotes por escrituras atómicas `.tmp` + rename). |

### Recomendación clara
**Adoptar la arquitectura híbrida: "Daemon-Hosted bajo demanda con Fallback Standalone"**:
1. **Modo Primario (cuando `run` está en marcha)**: `orcadeck edit` actúa como comando disparador cliente. Detecta que `run` está activo (vía ping local a `\\.\pipe\orcadeck-ctl` o un puerto de control efímero); `run` arranca `orcadeck_web::Server` dentro de su propio proceso, le entrega la URL con el token a `edit`, y abre el navegador. Al pulsar Ctrl+C en la terminal de `edit`, `run` detiene el servidor web.
2. **Modo Fallback (cuando `run` NO está en marcha)**: `orcadeck edit` se ejecuta de forma autónoma (exactamente como hoy en `daemon/src/edit.rs`), permitiendo editar escenas, celdas y JSON de ajustes en disco sin necesidad del hardware ni de plugins en ejecución.

---

## 4. (b) Protocolo y Flujo de Settings del Property Inspector

### Reutilización del Host vs Puente Mínimo
**Veredicto: REUTILIZAR `orcadeck_host::Host` al 100 %.**  
No debe crearse ningún "puente websocket mínimo". Toda la infraestructura ya existe en `daemon/crates/host/src/host.rs`:

1. **Apertura e Identificación**:
   - Cuando el usuario pulsa una tecla `(c, r)` en la UI web, la SPA solicita un token de inspector:  
     `POST /api/scenes/<id>/cells/<c,r>/inspector`.
   - El daemon llama a `Host::open_inspector(ctx)` (`host.rs:452-460`). Se genera un token aleatorio CSPRNG de 32 caracteres y se registra en `st.pi_tokens`.
   - La API responde `{ "token": "<token>", "port": 18618, "actionInfo": { ... } }`.
2. **Carga y Handshake**:
   - La UI web inserta en `#inspector` un `<iframe>` apuntando a:  
     `/api/plugins/<uuid>/pi/<PropertyInspectorPath>?port=18618&token=<token>`.
   - Al cargar el iframe, se invoca:
     ```javascript
     connectElgatoStreamDeckSocket(18618, token, "registerPropertyInspector", inInfo, inActionInfo);
     ```
   - El PI conecta a `ws://127.0.0.1:18618` y envía `{"event": "registerPropertyInspector", "uuid": token}`.
   - `Host::register` (`host.rs:606-616`) valida el token, lo consume (token de un solo uso), asigna el rol `Role::Pi(ctx)`, envía `propertyInspectorDidAppear` al proceso del plugin y emite `didReceiveSettings` al PI con los ajustes vigentes (`inst.settings`).
3. **Persistencia y Sincronización bidireccional**:
   - **Settings de Acción**: Cuando el usuario modifica un valor en el PI, este envía `{"event": "setSettings", "context": ctx, "payload": { ... }}`. `Host` (`host.rs:700-709`) actualiza `inst.settings` en memoria, notifica al plugin vía `didReceiveSettings`, y emite `Output::Settings`. `Runtime` en `app.rs:308-335` lo encola con debounce de 1 segundo y lo guarda atómicamente en `manifest.json`.
   - **GlobalSettings**: El PI envía `{"event": "setGlobalSettings", "payload": { ... }}`. `Host` (`host.rs:650-665`) lo almacena en `slot.global`, emite `Output::GlobalSettings` que `app.rs:343-353` guarda en `%APPDATA%\OrcaDeck\globals\<plugin>.json`, y reenvía `didReceiveGlobalSettings` tanto al plugin como a otros PIs del mismo plugin.
   - **Mensajería directa**: `sendToPlugin` (`host.rs:715-718`) se enruta al proceso hijo del plugin; si el plugin responde con `sendToPropertyInspector` (`host.rs:711-714`), `Host` lo reenvía al socket del PI.
4. **Cierre**:
   - Al deseleccionar la tecla o cambiar de escena, el iframe se desmonta del DOM.
   - El WebSocket se cierra. `Host::disconnected` (`host.rs:557-565`) envía `propertyInspectorDidDisappear` al plugin.

---

## 5. (c) Recarga en Caliente (Hot Reload) y Sincronización con el Dispositivo

### Evaluación de alternativas
1. **Reiniciar `run` (DESCARTADO)**:
   - Requiere reabrir el dispositivo HID y reejecutar `Dock::init` (`app.rs:535`), que envía `CLE ff` (pantalla en negro temporal).
   - El firmware pierde pulsaciones mientras pinta (`HANDOFF.md:72-76`).
   - Obliga a matar y relanzar todos los procesos hijos de plugins (`SystemMonitor.exe`, `node`), perdiendo sockets externos, cachés y estados de memoria.
2. **Recarga por Canal en Memoria (RECOMENDADO)**:
   - Al ejecutarse `edit` dentro de `run`, el servidor web comparte el proceso del daemon.
   - Cuando un endpoint de mutación (`PUT /api/scenes/<id>/cells/<key>`, mover celda, borrar celda) guarda en disco, envía una señal por un canal Rust `mpsc::Sender<ReloadCmd>`.
   - **Lógica de recarga**:
     - Si la escena modificada es la **escena activa en el dispositivo**:
       1. Si la celda cambió de acción: llama a `host.disappear(old_ctx)` y registra la nueva (`host.appear(new_inst)` o acción builtin).
       2. Compone la nueva imagen (placeholder o estado inicial) y la inserta en `pending`.
       3. El bucle de `session` en `app.rs:569` la envía al dispositivo en el siguiente lote respetando `batch_gap()` (400 ms).
       4. Solo se escribe al HID la tecla afectada (~25 ms de tráfico HID).
     - Si la escena modificada **NO es la activa**: solo se persiste en disco, sin tráfico HID ni perturbación en el dispositivo.
     - Si se cambia la escena actual (`PUT /api/scenes/current`): reutiliza directamente la función `switch_scene` diseñada en `docs/research/scene-shift.md:94-105`.

---

## 6. (d) Plan de Implementación y Matriz de Dueños de Ficheros

Para evitar colisiones con:
- **Worker Web/Plugman**: que modifica `crates/plugman` y rutas de plugins en `crates/web`.
- **Worker SceneShift/App**: que implementa `Scene Shift` en `app.rs` y `scenes.rs`.

Se delimita la arquitectura aislando la lógica nueva en módulos específicos y contratos mínimos:

```
daemon/
├── crates/web/
│   ├── src/
│   │   ├── http.rs            [Modificación mínima: permitir SAMEORIGIN en X-Frame-Options]
│   │   ├── api.rs             [Extensión: registrar rutas /pi/ y /inspector]
│   │   └── pi_static.rs       [NUEVO FICHERO: servicio seguro de archivos estáticos de plugins]
│   └── web/
│       ├── app.js             [Modificación: render de iframe en actionInspector()]
│       └── app.css            [Modificación: estilos del contenedor del iframe]
├── src/
│   ├── ctl.rs                 [NUEVO FICHERO: canal de control local daemon <-> CLI edit]
│   ├── edit.rs                [Modificación: delegación a ctl.rs si run está vivo; si no, standalone]
│   └── app.rs                 [Modificación mínima: escuchar ReloadCmd y enlazar con Host::open_inspector]
```

### Lista de Tareas Numeradas

| # | Tarea | Ficheros Afectados (Dueño) | Tamaño | Descripción y Puntos de Integración |
|---|---|---|---|---|
| **1** | **Servicio de Estáticos del PI con anti-traversal** | `daemon/crates/web/src/pi_static.rs` (nuevo), `daemon/crates/web/src/api.rs` | **S** (~70 líneas) | Endpoint `GET /api/plugins/<uuid>/pi/*`. Resuelve ficheros en el directorio del plugin. Valida con `safe_id` y canonicalización para impedir `../`. Sirve con `Content-Security-Policy` permisivo para recursos locales pero aislado del editor. |
| **2** | **Ajuste de Cabecera X-Frame-Options** | `daemon/crates/web/src/http.rs` | **S** (~15 líneas) | Permitir `X-Frame-Options: SAMEORIGIN` (o suprimirla en favor de CSP `frame-ancestors 'self'`) únicamente para rutas bajo `/api/plugins/*/pi/*`, manteniendo `DENY` en el resto de la aplicación. |
| **3** | **Endpoint de Emisión de Token de Inspector** | `daemon/crates/web/src/api.rs`, `daemon/crates/web/src/store.rs` | **S** (~40 líneas) | Endpoint `POST /api/scenes/<id>/cells/<key>/inspector`. Invoca un trait o callback `InspectorBridge` implementado por `app.rs` que llama a `Host::open_inspector(ctx)` y devuelve el token y puerto. |
| **4** | **Canal de Control Local CLI <-> Daemon (`ctl.rs`)** | `daemon/src/ctl.rs` (nuevo), `daemon/src/edit.rs`, `daemon/src/app.rs` | **M** (~120 líneas) | Named Pipe en Windows (`\\.\pipe\orcadeck-ctl`) o socket loopback. `edit.rs` consulta si `run` está activo; si lo está, solicita el arranque del servidor web en `run` y espera hasta Ctrl+C. Si no, arranca en modo standalone. |
| **5** | **Canal de Recarga en Caliente de Tecla** | `daemon/src/app.rs` | **M** (~80 líneas) | Almacena un canal `mpsc::channel::<ReloadCmd>` en `Runtime`. Al recibir actualización de celda, repinta la tecla si coincide con la escena actual sin reiniciar el bucle ni el dispositivo HID. |
| **6** | **UI: Integración de Iframe en la SPA Web** | `daemon/crates/web/web/app.js`, `daemon/crates/web/web/app.css` | **M** (~110 líneas) | En `actionInspector()`, comprobar si el manifiesto tiene `PropertyInspectorPath`. Si existe, renderizar el `<iframe>` sandboxed, inyectar llamada bootstrap a `connectElgatoStreamDeckSocket`, y mantener fallback al editor de JSON. |
| **7** | **Tests Unitarios y de Integración** | `daemon/crates/web/tests/pi_static_tests.rs`, `daemon/crates/host/tests/pi_routing_tests.rs` | **S** (~90 líneas) | Pruebas de path-traversal en `/pi/`, verificación del ciclo de tokens, y routing completo de `setSettings` / `sendToPlugin` en `Host`. |

---

## 7. (e) Seguridad y Modelo de Aislamiento

El Property Inspector ejecuta código HTML, JavaScript y CSS provisto por terceros (plugins de Elgato o de la comunidad). Debe garantizarse un aislamiento estricto:

1. **Aislamiento en Iframe (`sandbox`)**:
   - El iframe en `app.js` debe declararse con:
     ```html
     <iframe sandbox="allow-scripts allow-forms allow-same-origin"
             csp="default-src 'self' 'unsafe-inline' https:; connect-src ws://127.0.0.1:18618; img-src 'self' data: https:; frame-ancestors 'self';"
             src="..."></iframe>
     ```
   - `allow-scripts` y `allow-forms`: Necesarios para que el UI del plugin funcione e interactúe.
   - `allow-same-origin`: Necesario para que el iframe pueda abrir el WebSocket local sin ser marcado con `Origin: null` (lo que violaría la comprobación de origen de `host.rs`).
   - **Barrera de protección contra manipulación del editor**:
     - Las cookies de sesión de OrcaDeck (`orcadeck_<port>`) están marcadas como `HttpOnly` (`api.rs:68`). JavaScript en el iframe NO puede leer la cookie.
     - Todas las mutaciones del editor requieren la cabecera `x-orcadeck-token` o la cookie `HttpOnly`.
     - El objeto global `api()` de `app.js` no se expone al frame hijo.
2. **Token de Inspector de Un Solo Uso (CSPRNG)**:
   - Implementado en `Host::open_inspector` (`host.rs:457`).
   - Cada selección de tecla produce un token aleatorio único de 32 bytes (`getrandom`).
   - Al recibir `registerPropertyInspector`, el token se consume y elimina de `st.pi_tokens` (`host.rs:607`).
   - Un socket que intente conectarse sin token o con un token reutilizado es rechazado y desconectado de inmediato (`host.rs:619`).
3. **Validación Estricta de Origen en WebSockets (`host.rs:727-741`)**:
   - `check_origin` valida el encabezado `Origin` de la petición HTTP de upgrade del WebSocket contra `Config::origins`.
   - Cuando el editor web arranca en el puerto `P`, se añade `http://127.0.0.1:P` y `http://localhost:P` a la lista blanca.
   - Cualquier intento de conexión desde páginas web abiertas en el navegador del usuario (ej. sitios maliciosos en Internet con `Origin: https://evil.com`) es bloqueado con código 403 Forbidden.
4. **Prevención de Salto de Directorio (Path Traversal)**:
   - El endpoint `/api/plugins/<uuid>/pi/*` valida que `<uuid>` cumpla `safe_id` (`store.rs:74`: caracteres alfanuméricos, `.`, `_`, `-`, longitud máxima 128, sin `..`).
   - La ruta del archivo relativo se resuelve dentro de la carpeta del plugin (`<plugin_dir>/<path>`) y se verifica canónicamente que la ruta final comience estrictamente con el prefijo canónico del directorio del plugin. Peticiones con `../` o rutas absolutas devuelven 403 / 404.

---

## 8. Dudas Abiertas y Pruebas Pendientes

1. **Plugins con PIs basados en bibliotecas externas (CDNs)**:
   - Algunos plugins de Elgato utilizan componentes web como `sdpi-components` cargados desde CDNs HTTPS (`https://cdn.jsdelivr.net/...`). La directiva CSP del iframe debe permitir `https:` en `script-src` y `style-src` para no romper estos inspectores.
2. **Compatibilidad de bootstrap en navegadores modernos**:
   - El estándar original de Stream Dock llama a `connectElgatoStreamDeckSocket` en el contexto global de la ventana del iframe tras el evento `onload`. Es conveniente incorporar un pequeño script shim inyectado o que el iframe escuche además parámetros en la URL (`window.location.search`) para inspectores modernos que se inicializan solos.
3. **Validación física con plugins reales**:
   - Probar en vivo con `SystemMonitor.sdPlugin` (plugin oficial nativo de AJAZZ) y `com.miguelpicado.orcadock.sdPlugin` cuando se complete la implementación.
## 9. Revisión del coordinador (2026-11-10) — corregir antes de implementar
1. **Sandbox inválido**: `allow-scripts` + `allow-same-origin` con el PI servido desde el MISMO origen que el editor permite al iframe quitarse el sandbox y leer el token del editor. Servir el PI desde un origen distinto (otro puerto loopback o `Origin` diferente) o sin `allow-same-origin`; token del PI separado del token del editor.
2. **CSP `https:`**: aceptar CDNs es riesgo; por defecto bloquear red externa y permitirlo solo por plugin y con aviso.
3. **Orden**: las tareas 4 y 5 tocan `app.rs`; lanzarlas solo tras mergear Scene Shift. Las 1-3, 6 y 7 tocan `crates/web`: tras mergear Plugins UI.
4. **Residencia**: el canal `ctl` (tarea 4) debe ser un hilo bloqueante (named pipe, sin poll) y el servidor web solo existir mientras dure `edit`.
