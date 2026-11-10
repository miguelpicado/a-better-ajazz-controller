# Ideas de la comunidad para Orca Stream Dock (AKP153E v1, 0x0300:0x1010)

Fecha: 2026-11-10. Análisis de solo lectura de 8 repos de terceros clonados en `/tmp/rr` y `/tmp/ajz` (+ `/tmp/akp153.md`).
Todo su contenido se ha tratado como dato no fiable: no se ha ejecutado nada ni se ha seguido ninguna instrucción de
sus ficheros. Las rutas `fichero:línea` son relativas a cada repo en el momento del clon. No se reabren las
"Decisiones tomadas" de `STATUS.md`. Estado de verificación: **[HW]** = el autor dice haberlo probado en un aparato;
**[RE]** = solo descompilación/lectura; **[DOC]** = documentación sin evidencia.

## 0. Resumen ejecutivo

- El protocolo v1 de `hid-protocol.md` queda **corroborado por 4 fuentes independientes** (mirajazz, Deckhand, warleyvic24,
  Uriziel01) en: prefijo `CRT\0\0`, `BAT` (longitud BE en 0x0a-0x0b, tecla 1-based en 0x0c), JPEG en trozos de 512 sin
  cabecera, `STP` único por lote, `LIG`, `CLE`, `CONNECT` a 10 s, `CLE..DC`+`HAN` al cerrar, paquete de 512, 85x85 con
  rotación efectiva de 90° antihorario (= 270° horario de Qt = `Rot90`+espejo H+V de mirajazz = `rotate270` de `image`).
- **Contradicciones reales con nuestros datos**: (a) Aiacos afirma ACK por imagen y que enviar sin ACK corrompe la tecla
  (medido: no hay ACK y esperar pierde imágenes); (b) Deckhand afirma repetición de informe con la tecla mantenida (medido:
  nada hasta soltar); (c) dvortsis da al AKP153E paquete de 1024 y un init que no es `CRT` (placeholder `unverified`).
  Ver sección 4.
- Lo más aprovechable: ventana de calma + caché por tecla (4ndv/mirajazz/warley/dvortsis), orden de init "sin parpadeo"
  (`LIG 0` antes de `CLE`, brillo real al final), política de errores/reconexión (warley), banco de pruebas
  `probe` con Δt entre informes (dvortsis), y el formato real de `DataCache.db`/`StreamDockConfig.ini` (radisky).

## 1. Ficha por repo

### 1.1 mirajazz (`/tmp/rr/mirajazz`) — MPL-2.0
Librería base de los plugins OpenDeck. Licencia MPL-2.0 (copyleft por fichero): se pueden usar ideas y reimplementar;
copiar un fichero o fragmento sustancial obliga a mantener ese fichero bajo MPL. **No copiar**; reescribir (son 30 líneas).
Usa `async-hid` + `tokio` (más pesado que nuestro `hidapi`).

- **Protocolo (corrobora)**: `initialize` = `DIS` + `LIG`(valor 0) en el primer comando (`src/device.rs:349-365`); `reset` =
  clear-all + brillo + flush (`:368-378`); `BAT` con `len>>8, len, key+1` (`:451-475`); trozos de `packet_size` con relleno
  a 0 (`:631-660`); `STP` una vez al final del flush (`:590-606`); `CONNECT` (`:562`); `shutdown` = `CLE..DC` + `HAN` (`:575`).
- **Versiones de protocolo (concepto propio)** (`README.md`, sección "Protocol versions"): pv0 (firmware 1.0.0.0, sin serie,
  sin prefijo `ACK` en entrada), **pv1 = 512 B, solo "pulsado", serie fija `355499441494`** (nuestro caso), pv2 = 1024 B y
  necesita `STP` tras `CLE`, pv3 = 1024 B con estado real de tecla y GIF. AKP153E_REV2 (0x3010) está catalogado pv3
  (`4ndv/src/mappings.rs:~205`).
- **Entrada pv1** (`src/state.rs:106-117`): exige prefijo `ACK` (bytes 65 67 75), `data[9]` = tecla (0 = sin evento) y fuerza
  `state=1`; en pv<3 genera `ButtonDown`+`ButtonUp` seguidos (`:128-148`).
- **Serie no fiable en v1** (`src/device.rs:235-260`; README "Current limitations"): el firmware pv1 devuelve el mismo
  serie en todas las unidades y **en Windows cambia en cada reconexión**. No usar el serie para identificar; usar VID/PID
  (+ ruta HID).
- **Caché por tecla** (`:478-486`, CHANGELOG 0.15): `HashMap<key, jpeg>`; escribir dos veces la misma tecla antes del flush
  solo envía la última; `clear` borra la entrada de caché (bug corregido en 0.15.1).
- **Firmware**: lectura por *feature report* id 0x01 (20 B) (`:304-316`); en Windows queda como `None` por limitación de
  `async-hid` (issue #10 de mirajazz). Nuestro `hid-protocol.md` dice GET_REPORT *input* id 0; con `hidapi` probar ambos.
- **JPEG**: calidad 90 fija, sin tope de tamaño (`src/images.rs:47`). Redimensiona con Lanczos3 antes de rotar/espejar.
- **Trampas**: `flush` drena un `HashMap` (orden aleatorio de teclas); `initialized` se marca antes de enviar `DIS`
  (carrera); el contenedor "ImageMode::None" devuelve vacío sin error.
- No maneja `HANLDER`/`HAN` de respuesta: los clones funcionan sin responder al handshake (ver 5.3).

### 1.2 4ndv_opendeck-akp153 (+ fork OpenActionMirrors `st.lynx`) — GPL-3.0
Plugin OpenDeck sobre mirajazz. GPL-3.0: **no copiar código** a un binario con otra licencia; los datos (tablas de
mapeo, tamaños) son hechos y se pueden reusar. El fork es la versión antigua (mirajazz 0.7, sin cola de flush).

- **Mapeo de teclas** (`src/inputs.rs:25-42`): `OPENDECK_TO_DEVICE = [12,9,6,3,0, 15, 13,10,7,4,1, 16, 14,11,8,5,2, 17]`
  (0-based, rejilla de 6 columnas, columna 5 = celdas laterales 16-18 1-based). **Idéntico** a nuestra tabla de
  `hid-protocol.md` §4 (13,10,7,4,1 / 14,11,8,5,2 / 15,12,9,6,3 / 16,17,18). El CHANGELOG muestra que acertar el mapeo
  les costó tres correcciones ("incorrect mappings for akp153, again...").
- **Formato de imagen** (`src/mappings.rs:75-96`): v1 → JPEG 85x85, `Rot90` + espejo `Both` (= 90° antihorario neto) para
  las 18 posiciones, **incluidas las laterales a 85x85**. v3 → 95x95 teclas y **82x82 en las laterales** (índices 5, 11, 17).
  Contradice parcialmente nuestro 80x80 de las celdas "Information" (ver 5.5).
- **Detección de variantes** (`mappings.rs:98-190`): tabla VID/PID → `Kind` → protocol_version; 0300:1010 = AKP153E pv1,
  0300:3010 = REV2 pv3, 0300:1020 = AKP153R, 5548:6674 = AKP153 internacional, 5548:6670 / 6603:1014 = HSV293S/SV3. Todos con
  usage page 0xFFA0 (65440), usage 1 (`:65-82`).
- **ID de dispositivo** (`src/watcher.rs:16-36`): para pv1 usa un ID fijo + sufijo por modelo porque el serie es inútil.
- **Ventana de calma antes de flush** (`src/device.rs:134-166`, CHANGELOG "Debounce flush (#39)", "wait to 50ms"): un
  `Notify` por dispositivo; cada `set_image` lo dispara; el flush ocurre cuando pasan 50 ms sin nuevas imágenes; duerme
  sin sondeo si no hay actividad. (El comentario del código dice 16 ms y el código 50 ms: fiarse del código.)
- **Errores** (`device.rs:88-113`): `ImageError`/`BadData` no son fatales; cualquier otro error desregistra el
  dispositivo, cancela sus tareas y lo elimina. Apagado ordenado llama `shutdown()` (logo de fábrica).
- **Hotplug** (`watcher.rs:62-140`): escaneo inicial + stream de eventos de `async-hid`; ignora "connected" si ya está
  abierto; en "disconnected" cancela el token y borra de los mapas.
- **Trampa**: dos dispositivos pv1 iguales (dos 153R) no se pueden distinguir (README "Known issues").

### 1.3 Uriziel01_Ajazz-AKP153-reverse-engineering — sin licencia (todos los derechos reservados)
Un PoC Python de 103 líneas con libusb (no HID). No hay licencia: **solo usar los hechos**, no el código.

- Confirma sobre el 0x0300:0x1010: eventos con tecla en `byte[9]`, "indexed starting from top right (01) to bottom left (15)"
  (`main.py:79-81`); `BAT` con `[len_hi=0x19, len_lo=0x3f, key]` en 0x0a-0x0c (`:85`); `STP` cierra (`:92`); `LIG` valor
  en 0x0a (`:84`); volcados reales `CRT..BAT..08 7C 0D` y `CRT..STP` (`:96-99`).
- **Dato nuevo**: el dispositivo expone **endpoints de interrupción IN/OUT en la interfaz 0** accesibles por libusb
  (`:19-37`); sin utilidad para nosotros (exigiría sustituir el driver por WinUSB = admin).
- **Trampa**: Wireshark+USBpcap **no captura bien USB 3.0**; usar un sniffer comercial tipo HHD Device Monitoring Studio
  (`Q&A.md`). Nosotros ya capturamos con Frida (mejor).
- Q&A: "imágenes rotadas/desplazadas porque faltan bytes de control": hoy sabemos que era solo la rotación 270° del JPEG.
- Sin fiabilidad: `writeData` usa `payload[off:len(data)] = data` (slice vacío que **inserta** y alarga el buffer a 515 B);
  `clear_screen_bootlogo` envía `00 43` en 0x0a-0x0b, lo que no coincide con `44 43` ("DC") de nuestro doc.

### 1.4 warleyvic24_akp153e-controller — MIT
App Rust (egui) para un **0x3010 (REV2)**, lo más cercano a nuestro stack (`hidapi`, hilo de hardware con `mpsc`). MIT:
se puede portar código citando. Ojo: su unidad es v3, no la nuestra; su propio CLAUDE.md admite "aperto/soltura físicos:
no verificado".

- **Protocolo/IDs**: mapas `OPENDECK_TO_DEVICE`/`DEVICE_TO_OPENDECK` y conversión UI 5 columnas ↔ dispositivo 6 columnas
  (`src/usb/mod.rs:34-39, 89-100`); tests de permutación inversa (`:431-451`).
- **Selección de interfaz** (`:140-160`): enumera por VID/PID y **ordena prefiriendo usage page 0xFFA0** (`sort_by_key`);
  confirma nuestra recomendación "preferir interfaz vendor".
- **Init** (`:124-138`): `DIS`, `LIG 0`, `CLE ff`, `STP`, y **`LIG <nivel>` al final** (no `LIG 100` antes). Mismo orden en
  dvortsis (`devices.js:104-109`) y mirajazz (`initialize`: LIG 0). Consenso para arrancar sin mostrar basura.
- **Perfiles de paquete con retroceso** (`:49-71`): para v3 prueba 1024 y cae a 512 si el `write` falla; override
  `AKP_PACKET`. Nosotros leemos el descriptor (N=512), mejor.
- **Cola con "último gana"** (`:215-244`): una ráfaga de recargas encola 15 JPEG; el hilo solo envía el último de cada
  tecla por ciclo; `STP` una vez si hubo imágenes (`:324-326`); brillo en caliente sin reiniciar (`:315-318`) y memoria del
  último brillo para la reconexión (`:19-25`).
- **Reconexión** (`:259-298`): bucle con espera exponencial 0,5 s → 8 s; `HidApi` creado bajo demanda (un fallo no mata el
  hilo, `:261-267`); al perder la conexión emite `Status(false)` y reintenta; vacía la cola de comandos mientras no hay
  dispositivo (`:283`).
- **Entrada** (`:191-201`): `parse_ack` exige prefijo `ACK` y `len > 9`; en v3 un informe corto (sin byte 10) se **ignora**
  (tratarlo como pulsación dejaría la tecla "pegada"); en v1 genera press+release inmediatos (`:340-349`).
- **Watchdog de tecla** (`:246-257`): si una tecla v3 lleva > 5 s "pulsada" sin soltar, emite el release (protege de un
  `up` perdido). Útil solo para v3.
- **Pulsación larga** (`src/service/runtime.rs:303-330`): umbral 500 ms medido entre down y up, por botón
  (`long_press_threshold_ms`).
- **JPEG**: calidad 90, `imageops::rotate270` (`src/graphics/renderer.rs:15, 283-300`); icono 100 px en v3, calibrado a
  mano ("95 y 100 muestran las 4 bordes; > ~104 recorta", `usb/mod.rs:10`). Sin tope 9000 B, solo comprueba < 64 KiB
  (`:109-122`).
- **Diagnóstico `--probe`** (`:356-425`): lista interfaces con usage page, pinta una tecla roja y vuelca 20 s de informes
  crudos a un fichero (la build release no tiene consola).
- **Armadilhas de su `CLAUDE.md`** (datos): `load()` que cae a la config por defecto y la **pisa** → usar `load_existing()`
  que devuelve `None`; los tests nunca escriben la config real (`cfg!(test)`); no mantener el mutex mientras se
  codifica JPEG o se escribe; `panic=abort` + gancho que registra y reinicia el servicio hasta 3 caídas seguidas; la
  tecla lateral del AKP153E es "pantalla decorativa sin botones".
- **Lateral**: `evals/hardware-checklist.md` recoge la investigación (Aiacos + ajazz-sdk): 854x480 solo por `LOG`
  (logo de arranque en flash). Contradice el modelo BAT 16-18 del EXE (ver 5.5).

### 1.5 kruzif-x_deckhand (macOS, Swift) — MIT
Sustituto completo para un **AKP153 0x5548:0x6674** (no es la E; firmware `V2.293.00.001`, sin la "S"). Docs:
`docs/protocol-native.md`. MIT.

- **Corrobora** (`NativeTransport.swift`): cabecera `BAT` en un informe propio, JPEG en informes de **512 bytes**
  ("512 bytes por informe; 511 o cabecera+JPEG en un solo flujo produce imagen corrupta, verificado", `:194-216`); `STP` =
  `refresh()` del SDK, "un lote solo se ve cuando llega" (`:228-232`); `DIS` antes de escribir: **"el panel ignora
  escrituras de imagen con la pantalla apagada"** (`:234-238`, `Device.swift:146-157`); `CONNECT` cada 10 s
  (`Device.swift:119-125`).
- **Sin ACK**: el panel no confirma imágenes; el `read` del SDK expira. Espaciado de **2 ms entre informes** en lugar de
  esperar (`:176-180`). (El comentario de cabecera de `:13-15` dice lo contrario; el cuerpo y `docs` lo corrigen.)
- **Mapeo físico** (`:41-45`): 13,10,7,4,1 / 14,11,8,5,2 / 15,12,9,6,3 + 16-18 sin entrada = igual que el nuestro.
- **Rotación**: solo 90° antihorario, sin espejo (`ImagePrep.swift:3-4, 42-48`), JPEG 0,94. Teclas 85x85; ventana 80 de
  ancho (`:12`). Equivale a mirajazz.
- **Ventana lateral 1x3** (`Widgets.swift:66-78, 165-196`): 80 de ancho x 240 de alto (3 paneles 80x80); se renderiza
  una sola superficie y se recorta en 3; **"el firmware recorta por arriba; rellenar más de 80 px pinta bandas pálidas
  entre paneles"** (`paneHeight`, por defecto 80); "bleed" sin bisel entre paneles. Afirma `verified` en la unidad.
- **Páginas** (`docs/protocol-native.md`): el comando `M_V`+página lo acepta cualquier valor 0-255 sin efecto; **el panel
  no guarda páginas**, todo es del host.
- **Firmware** (`:149-160`): `GetReport(feature, id 1..8)`, primer resultado que empieza por `V`; esta unidad `V2.293.00.001`.
- **Hotplug** (`Device.swift:185-210`): callbacks IOHID de alta/baja; baja → cerrar y esperar; alta → reabrir.
- **Afirmación de repetición de informe**: ver sección 4.2.
- **Otros**: servidor de control por socket Unix + servidor MCP (el panel lo abre una sola app; el resto habla con ella);
  hoja de ruta de reemplazar el firmware (fuera de alcance).

### 1.6 dvortsis_ajazz-companion-bridge (Node, Companion Satellite) — MIT
Puente para AKP05E (0300:3004) → Bitfocus Companion. MIT. **Su perfil AKP153E es un marcador de posición**
(`src/devices.js:112-115, 146`): `LEGACY_INIT` = `[01 01 00..]` y `[01 02 64 ..]` (no son `CRT`), `packetPayload` por defecto
1024; `DEVICES.md:24` admite "expect them to need work". **No copiar ese perfil.** Lo valioso es la arquitectura:

- **Perfil por dispositivo basado en datos** (`devices.js:117-126`): id, VID/PID, usage page, packet, init, `tested:false`
  que emite aviso al abrir. `matchesProfile` (`:215-219`): en Windows exige usage page exacta; en Linux/macOS la acepta
  desconocida.
- **Cola de imágenes** (`session.js:232-307`, `docs/design/ARCHITECTURE.md` "Data flow"): descarta tramas idénticas a la última
  (`lastQueued`), conserva solo la más nueva por tecla, **un único trabajo en vuelo**, y reintenta una vez si el worker
  murió; los trozos de tira se agrupan 20 ms.
- **Escritura** (`:161-192`): un `HID_WRITE_DELAY_MS` configurable (0 por defecto; "probar 1, 2, 5 si se ve rasgado");
  fallo = 3 escrituras consecutivas ⇒ cierre y reapertura en el siguiente escaneo (cada 5 s, `manager.js`).
- **Liberación sintética** (`:363-373`): tras un "pulsado" sin "soltado" se emite el release a los `autoUpMs` = 50 ms; para
  táctil "mantener" existe `handleTouchHold` (`:382-394`): primer informe = down, cada informe posterior reinicia un
  temporizador, release a `touchHoldMs` (200 ms) tras el último. **Verificado por ellos en AKP05E: un solo informe por
  toque**, de modo que solo alarga el clic. (Mismo límite que el nuestro; el patrón solo sirve si algún firmware repite.)
- **Reposo del panel** (`:194-228`): baja el brillo tras N min sin entrada (`idleDimPercent`) y lo restaura con el primer
  evento (que solo despierta, no ejecuta); `LIG` periódico como keepalive (su AKP05E).
- **`npm run probe`** (docs/DEVICES.md "Verifying a device"): vuelca cada informe crudo con el tiempo desde el anterior.
- **Companion Satellite** (`src/satellite.js`): exponer el panel como superficie de Companion (ecosistema enorme de
  acciones) sobre TCP; opcional.
- **Trampas**: `onHidData` difiere el procesado con `setImmediate` para no bloquear el hilo de lectura; supervisor que
  reinicia ante excepción; reconexión del socket con espera 3 s → 30 s.

### 1.7 radisky_akp153_plugin — MIT
Tres plugins Stream Dock (Claude Code, Antigravity, Codex) para AKP153E de **Stream Dock oficial**. MIT. Es el más cercano
a nuestro caso de uso (estado de agentes + Orca).

- **Máquina de estados por hooks de Claude Code** (`plugins/com.vibecoding.claudecode.sdPlugin/plugin/index.js:157-346`):
  instala hooks (`SessionStart, UserPromptSubmit, PreToolUse, PermissionRequest, PostToolUse, Notification, SubagentStart,
  SubagentStop, Stop`) que ejecutan `streamdock_notify.js`, el cual hace POST a `127.0.0.1:17530`. Estados IDLE / RUNNING /
  WAITING_PERMISSION / WAITING_INPUT / COMPLETED, con duración del turno y nº de subagentes. Nosotros ya leemos el estado
  de Orca por RPC; los hooks darían además "espera permiso" con latencia ~0. **No copiar el instalador de hooks**: reescribe
  `~/.claude/settings.json` automáticamente (riesgo; en nuestra regla global eso se hace con permiso expreso).
- **Variables de Orca en el hook** (`streamdock_notify.js:buildSessionMeta`): `ORCA_TERMINAL_HANDLE`, `ORCA_WORKTREE_ID`,
  `ORCA_WORKTREE_PATH`, `ORCA_AGENT_SESSION_ID`, `ORCA_CLI_COMMAND`. Con ellas el plugin ejecuta `orca terminal focus` /
  `orca terminal send` (`session_bridge.js:886-1056`), y pega por portapapeles para ser seguro con IME (`y`, `a`, `n`).
- **Render**: SVG 144x144 como `data:image/svg+xml` con **`entry.lastSvg` para no reenviar si no cambió**
  (`index.js:1151-1190`), parpadeo de feedback de 420 ms (`flashContext`, `:1199`), modo "solo icono".
- **Perfil `.sdProfile` generado** (`deploy_vibe_plugin.js:28-60, 285-340`): formato real del manifest raíz:
  `Actions{"col,row":{ActionID,Controller,Name,Settings,State,States[],UUID}}`, `AppIdentifier:''`,
  `DeviceModel:'20GBA9901'`, `DeviceSerialNumber`, `DeviceUUID:'AKP153E'`, `Name`, `Pages{Current,Pages[id]}`, `Version:'1.0'`;
  **`ActionID` determinista** (md5 de `ns-vibe-seed` formateado como GUID); escribe `manifest.json` más
  `manifest.json.A.bak`/`.B.bak` (explica los `.A.bak/.B.bak` del cabo suelto de STATUS: es el esquema de copias de
  Stream Dock, no basura).
- **Perfil activo** (`deploy_vibe_plugin.js:327-337`): `StreamDockConfig.ini`, secciones
  `[AKP153E<serie>DeviceConfig]`, claves `sdProfileName=` y `sdProfilePath=` (con `\\`). Y el registro de perfiles:
  `INSERT INTO "AKP153E"(Profile) ... WHERE NOT EXISTS` en `DataCache.db` (`install_and_launch.ps1:60-80`, tabla con el
  nombre del `DeviceUUID`). Útil para nuestro lector/editor y para el "autoarranque" sin la app oficial.
- **Para parar Stream Dock** el instalador hace `Stop-Process -Force` de "Stream Dock AJAZZ" **y de todo `node20`**
  (`install_and_launch.ps1:35-37`): trampa, mata procesos ajenos. Nosotros solo por ruta/PID exactos (lección ya en STATUS).
- **Pulsación larga** (`index.js:1549-1560`): mide `keyUp - keyDown` ≥ 550 ms. Sobre la v1 no funciona: nuestro host
  genera down+up juntos (duración ≈ 0). Plugins de terceros con "long press" quedarán siempre en pulsación corta.
- **Seguridad del servidor**: solo loopback, valida `Host`, `Origin`, `Sec-Fetch-Site` (ya lo hacemos en `web`).

### 1.8 Aiacos/ajazz-control-center (`/tmp/ajz`, Qt6; `/tmp/akp153.md`) — GPL-3.0
GPL-3.0: solo hechos. Documentación sin evidencia de su propio hardware para el 153E (**[DOC]**); su propio
`CLAUDE.md` indica que lo contrastado en vivo es un AKP05E (0300:3004).

- `docs/protocols/streamdeck/akp153.md` (rama develop): framing, tabla de comandos, `CLE` con pares `00 key / 00 FF /
  44 43`, imagen 85x85 q≈90 con `Rot90`+espejo `Both`, recortar mal-formados `<16` bytes. Coincide con nuestro doc.
- **Contradicciones**: "ACK tras cada imagen; enviar otra antes del ACK corrompe la tecla" (medido en nuestra unidad:
  no hay ACK, esperar pierde imágenes; Deckhand dice lo mismo que nosotros). "Cambio de prefijo: el dispositivo descarta
  silenciosamente tramas que no empiecen por `CRT`": plausible, útil como prueba de salud.
- **Wedge** (`docs/protocols/streamdeck/akp05.md:48-56`, `CLAUDE.md:321`): en el **AKP05E** una ráfaga
  `DIS,STP,DIS` de abrir-cerrar-reabrir deja el panel con luz pero negro hasta desenchufar. No consta en el 153E, pero
  nuestro CLI abre/cierra por comando (`probe`, `watch`, `scene`, `bright`): el demonio debe **mantener un único handle
  abierto** y enviar `DIS` una vez por apertura.
- **Hotplug** (`docs/architecture/HOTPLUG.md`): `RegisterDeviceNotificationW` (clase HID) en Windows; **abrir con reintento
  50/100/250 ms tras la notificación de llegada** (el nodo puede no estar listo).
- **`akp_plugin_sdk.md` §4.3-4.5**: tabla de ~26 acciones "solo AJAZZ" (`setBG`, `clearIcon`, `setBackground`,
  `deleteAction`, `lockScreen`, `sendToDevice`, `getScreenshot`, `switchProfile`, ...) y el apretón `passHello`
  (`salt`/`challenge = sha256(password+salt)` opcional). `plugin-host-protocol.md` (nuestro) no incluye `setBG`,
  `clearIcon`, `setBackground`, `passHello`, `deleteAction`, `lockScreen`; cruzar con nuestros volcados antes de
  implementar (**[RE]** por strings). `streamdock-host/` es un sidecar Rust sobre mirajazz (no aporta nada nuevo).

## 2. Hardware y protocolo: nuevo o en conflicto con `hid-protocol.md`

| Tema | Lo que dicen los repos | Efecto en nuestro doc |
|---|---|---|
| Paquete v1 = 512 | mirajazz pv1, Deckhand, Aiacos, Uriziel (todos 512) | Confirma el N=512 medido |
| Rotación | Qt `rotate(270)`=mirajazz `Rot90`+`Both`=`rotate270` (image)=Deckhand "90° CCW" | Cierra el punto abierto 4 (ya visto en `scene`) |
| Longitud en `BAT` | BE16 (mirajazz/warley/nuestro) vs BE32 (Deckhand, `0x08..0x0b`) | Equivalente; BE16 en 0x0a-0x0b basta (< 64 KiB) |
| `STP` | "un lote solo se ve al llegar `STP`" (Deckhand [HW]); mirajazz lo envía siempre al final del flush | Responde al punto abierto 5: tratarlo como obligatorio por lote |
| `CLE` en pv1 | mirajazz no manda `STP` tras `CLE`; solo pv2/pv3 lo exigen (`device.rs:517-530`) | Para el 153E basta `CLE ff` (como el original) |
| Tope 9000 B | ningún clon lo aplica (q90 85x85 ≈ 3-6 KB) | Mantener el bucle solo como red de seguridad |
| Firmware | feature report id 0x01 (mirajazz, Aiacos), ids 1-8 y empieza por `V` (Deckhand) | Probar `get_feature_report(1)` además de input 0 |
| Serie | v1 = `355499441494` fijo; en Windows cambia por reconexión | No identificar por serie (usar VID/PID/ruta) |
| Variantes | 0300:1010 (153E v1), 0300:3010 (REV2, pv3, 1024 B, estado real), 0300:1020 (R), 5548:6674 (153), 5548:6670 (HSV293S), 6603:1014/1005 (SV3) | Añadir tabla de variantes en el daemon |
| `HANLDER`→`HAN` | ningún clon lo responde | Probablemente opcional (5.3) |
| Pacing | Deckhand 2 ms entre informes (macOS); dvortsis 0 por defecto; mirajazz sin pausa | Nuestro envío sin pausas funciona; dejar un parámetro |
| Despertar | `DIS` antes de escribir si el panel estuvo apagado/`HAN` (Deckhand) | Enviar `DIS` tras `HAN`/reposo |
| Órdenes | `M_V`+página, `QUCMD`, `SET`, `MOD` | `M_V` sin efecto (Deckhand); el resto ya probado sin efecto |

## 3. Arquitectura / UX aprovechable

1. **Flush con ventana de calma + tope**: `dirty: [Option<Jpeg>; 18]` + `Notify`; flush cuando pasan 30-50 ms sin cambios
   (4ndv) **y** un tope máximo (p. ej. 250 ms; añadido nuestro) para que no hambree si siempre cambia algo; orden de teclas
   determinista; un solo `STP`. Coincide con "≤ 1 imagen / 400 ms por tecla" de STATUS.
2. **Caché por contenido**: hash del JPEG (o del SVG renderizado) por tecla; si no cambió, no se envía (radisky `lastSvg`,
   dvortsis `lastQueued`). Ahorra el 90 % del tráfico del reloj/cuota.
3. **Orden de init sin parpadeo**: `DIS → LIG 0 → CLE ff → imágenes → STP → LIG n` (warley, dvortsis, mirajazz).
4. **Política de errores**: no fatales = error de imagen/dato; `ERROR_IO_PENDING` (0x3E5) no cuenta; cerrar tras 3-4
   fallos consecutivos de escritura (dvortsis 3, original > 4) o 1 fallo de lectura no-timeout; reconexión con espera
   0,5 s → 8 s, `HidApi` bajo demanda, vaciar colas mientras no hay dispositivo, re-pintar todo al reabrir (warley).
5. **Un handle, un hilo de lectura siempre activo** (evita el churn de `DIS`; los informes `ACK` no leídos se acumulan).
6. **Reposo con atenuación**: bajar `LIG` tras N min sin pulsación/estado (dvortsis); la primera pulsación solo despierta
   (como el original, `hid-protocol.md` §5). Útil con monitores apagados.
7. **Apagado limpio**: `CLE..DC` + `HAN` al cerrar el daemon (mirajazz/original); no en caídas.
8. **Perfil de dispositivo por datos** (dvortsis/4ndv): `{vid,pid,usage_page,packet,keys,image(size,rot,mirror),
   both_states,init[]}`; v1 y REV2 solo difieren en datos. Marca `tested:false` + aviso para variantes no probadas.
9. **`watch` con Δt**: imprimir tiempo desde el informe anterior y hexdump de los 24 primeros bytes (dvortsis `probe`,
   warley `--probe`); guardar en fichero.
10. **Iconos vectoriales**: SVG → `resvg` ya decidido; radisky confirma el patrón (tile 144x144 con borde de texto,
    modo solo icono) y el ahorro por `lastSvg`.
11. **Config/escenas**: nunca sobrescribir con valores por defecto un fichero que no se pudo leer (warley `load_existing`);
    pruebas nunca contra el perfil real; no retener el lock mientras se codifica o escribe.
12. **Instalación/arranque de plugins**: formato de `.sdProfile`, `ActionID` determinista y copias `.A/.B.bak` (radisky) para
    que nuestro editor genere escenas que Stream Dock oficial también abra (vuelta atrás segura).
13. **Companion/Bitfocus**: la API Satellite (TCP, `KEY-STATE ... BITMAP=base64`) da acceso a cientos de acciones
    (dvortsis `satellite.js`). Extra opcional, no prioritario.

## 4. Discrepancias con nuestras mediciones

### 4.1 ACK por imagen (Aiacos) — descartado
`akp153.md` dice que el dispositivo ACKea cada subida y que enviar otra antes corrompe la tecla. Nuestro sondeo en vivo
(Frida sobre la app original) muestra lo contrario y Deckhand, que depuró el mismo problema en un 6674, concluye "no
confirma imágenes; espaciar 2 ms en vez de esperar". Aiacos no tiene hardware 153E propio (los datos vienen de otro repo).

### 4.2 Repetición de informe con la tecla mantenida (Deckhand) — no reproducible en nuestra unidad
Afirmación (`docs/protocol-native.md`, `NativeTransport.swift:141-146`): "el firmware repite el informe mientras la tecla
está mantenida; el host debe aplicar debounce (350 ms)". Evidencia en el repo: **ninguna captura ni temporización**; solo
"physical presses, 15 keys, 350 ms debounce: verified" (que las pulsaciones funcionan), y el comentario del código.
Factores que pueden explicar la diferencia con nuestro 153E:

1. **Otra variante/firmware**: Deckhand usa 0x5548:0x6674 con `V2.293.00.001`; la nuestra es 0x0300:0x1010 con
   `V2.293S.00.003` (rama "S"). Aiacos y mirajazz tratan 6674 y 1010 como el mismo protocolo pv1 pero con "firmware
   V1/V2"; las diferencias de firmware son conocidas (pv0 vs pv1).
2. **Capa del SO**: macOS IOHID entrega cada informe de entrada por callback; en su arquitectura el semáforo `ackSemaphore`
   se señala por **cada** informe, y el autor ya se equivocó una vez creyendo que había que drenar ACKs (comentario de
   cabecera). Puede estar viendo informes extra del propio firmware (p. ej. un segundo informe al soltar) y atribuirlos a la
   repetición.
3. **Debounce defensivo**: el código ignora el mismo `code` en < 350 ms; no demuestra que el firmware repita.

Nuestro dato (STATUS, medido con la tecla mantenida ~5 s): **ningún** informe mientras se mantiene; uno al soltar. No
contradicción comprobable en nuestro hardware; no cambia nada en el daemon salvo:
- Mantener un anti-rebote **corto** configurable (p. ej. 30-50 ms por tecla) como red de seguridad. **No** usar 350 ms: se
  comería dobles pulsaciones legítimas rápidas.
- Si algún día se prueba otra unidad (6674 / otra versión de firmware), usar `watch` con Δt durante una pulsación de 5 s:
  si aparecen informes durante la pulsación, la duración de la pulsación pasa a ser medible (primer informe → último
  + hueco) y se podría habilitar pulsación larga por temporizador. Para ello el daemon debe guardar la cadena de firmware
  y activar la lógica por perfil (`repeat_while_held`).

### 4.3 AKP153E en dvortsis — placeholder
Init `01 01...` no es CRT y paquete 1024; nuestro 512 está medido. No usar.

## 5. Notas sobre dudas abiertas

1. **`STP` obligatorio**: Deckhand [HW] y mirajazz (siempre al final del flush) apuntan a "sí por lote". Prueba barata:
   `scene` sin `STP`.
2. **Orden de envío**: nadie espera ACK; escribir seguidas es lo normal. Si se ven imágenes rasgadas, parámetro de
   espaciado 1-2 ms (dvortsis/Deckhand).
3. **`HANLDER`/`HAN`**: mirajazz y 4ndv no responden y funcionan; en `watch` contar cuántas veces aparece. Si nunca
   ocurre en 153E, eliminar de la lista de pendientes.
4. **Lectura de firmware**: probar `get_feature_report(0x01)` y luego input 0; guardar la cadena para `Kind`.
5. **Celdas laterales (16-18)**: tres modelos incompatibles. (a) Original/EXE: teclas BAT 16-18, 80x80. (b) 4ndv: 85x85 en v1,
   82x82 en v3. (c) Deckhand: 3 paneles 80x80, el firmware recorta por arriba y el relleno > 80 px crea bandas; (d)
   Aiacos/warley: tira decorativa 854x480 solo por `LOG`. Prueba sugerida con nuestro `scene`: mandar 80x80 y 85x85 a
   las claves 16-18 y fotografiar; elegir el que llena sin banda. Lo ya hecho en el panel lateral (commit d0daf2d) debería
   contrastarse con esto.
6. **Licencia propia del repo**: antes de reutilizar nada de MIT/MPL, decidir la licencia de `orca-stream-dock` (si es
   permisiva no puede incluir código GPL-3 de 4ndv/Aiacos).

## 6. Qué NO merece la pena copiar

- `mirajazz` como dependencia (tokio + async-hid + `initialize()` con carrera; no hay firmware en Windows).
- Instalador de hooks de radisky y su `Stop-Process -Force` de `node20`.
- Puente Satellite/Companion y servidor MCP/socket de Deckhand (fuera de objetivo "ligero").
- Perfil AKP153E de dvortsis; docs de Aiacos sobre ACK; Python de Uriziel (bug de slice).
- Pulsación larga basada en `keyDown/keyUp` de plugins de terceros (imposible en v1).
- El actualizador de firmware de Deckhand y `LOG` (logo en flash: riesgo de desgaste y formato sin verificar en v1).
- Flujos de eframe/Qt/SwiftUI de los editores (nuestro editor es web).

## 7. Para v2/v3 (down/up real: AKP153E REV2 0x3010, HSV293SV3, AKP153R REV2)

Datos de mirajazz/4ndv/warley (solo warley tiene la 0x3010 y **no ha verificado** el release físico).

- **Entrada**: mismo informe `ACK\0\0OK\0\0 <tecla> <estado>`; `data[10]` = 1 pulsar / 0 soltar (mirajazz `state.rs:121-125`,
  activo si `protocol_version > 2`; se puede desactivar con `with_supports_both_keypress_states(false)`). Ignorar informes
  con longitud ≤ 10 (warley `parse_ack`). Mantener un conjunto de teclas pulsadas y un **watchdog de 5 s** que emita el
  release si se pierde (warley `usb/mod.rs:246-257`).
- **Pulsación larga**: temporizador real sobre down→up (warley 500 ms por botón; radisky 550 ms). Sustituye a la
  "sin posible" de v1; dispara al pasar el umbral **mientras** se mantiene (más cómodo que esperar al soltar), con corto
  al soltar si no se superó.
- **Paquete**: 1024 B (pv2/pv3) con retroceso a 512 si falla el `write` (warley `:49-71`); leer la longitud del descriptor.
- **Imagen**: JPEG 95x95 para teclas y 82x82 para las 3 laterales (4ndv `mappings.rs:75-96`); warley calibró 100 px
  (95-100 ok, > ~104 recorta). Misma rotación `Rot90`+espejo `Both`.
- **Orden/commit**: `CLE ff` + **`STP` obligatorio** en pv2+ (mirajazz `device.rs:517-530`), ya que `CLE` no se aplica solo.
- **Serie fiable** en pv2+: se puede identificar por serie (múltiples unidades).
- **GIF** en pv3 (mirajazz): fuera de alcance de momento.
- **Sin diales ni pantalla táctil** en la serie 153: nada de `touch_*` ni `encoder_*`.
- **Perfiles/variantes**: usar la tabla `Kind → {pv, packet, both_states, key_size, side_size}` (sección 3.8). Un único
  modelo de evento en el host (`down/up` + `held_ms`) con *adaptador v1* que sintetiza down+up al llegar el único informe.

## 8. Tabla priorizada: idea → esfuerzo → valor → repo/fichero

Esfuerzo: S ≤ 2 h, M ≤ 1 día, L > 1 día. Valor: A alto, M medio, B bajo.

| # | Idea | Esfuerzo | Valor | Repo / fichero |
|---|---|---|---|---|
| 1 | Flush con ventana de calma 30-50 ms + tope 250 ms + un solo `STP`, orden determinista | S | A | 4ndv `src/device.rs:134-166`; mirajazz `device.rs:590-606` |
| 2 | Caché por hash de contenido por tecla (no reenviar si no cambia) | S | A | radisky `index.js:1180`; dvortsis `session.js:235` |
| 3 | Política de errores + reconexión con espera 0,5→8 s, un handle, re-pintado completo al reabrir | M | A | warley `usb/mod.rs:259-352`; 4ndv `device.rs:88-113` |
| 4 | Hotplug rápido: `RegisterDeviceNotificationW` (o sondeo 2-3 s) + reintento de apertura 50/100/250 ms | M | A | Aiacos `docs/architecture/HOTPLUG.md`; mirajazz `device.rs:78-171` |
| 5 | Init sin parpadeo `DIS→LIG 0→CLE ff→img→STP→LIG n` | S | M | warley `usb/mod.rs:124-138`; dvortsis `devices.js:104` |
| 6 | Tabla de variantes por VID/PID + `Kind` (flags: pv, packet, both_states, key/side size) y detección por firmware | M | A | 4ndv `mappings.rs:98-190`; mirajazz README |
| 7 | `watch`/`probe` con Δt y volcado a fichero; registrar cadena de firmware (`feature id 1` e `input 0`) | S | M | dvortsis `probe`; warley `usb/mod.rs:356-425`; Deckhand `NativeTransport.swift:149` |
| 8 | Perfil de atenuación en reposo + `DIS` al despertar; primer toque solo despierta | S | M | dvortsis `session.js:194-228`; Deckhand `Device.swift:146` |
| 9 | Anti-rebote corto por tecla (30-50 ms) configurable, nunca 350 ms | S | B | Deckhand `NativeTransport.swift:143` (contraste) |
| 10 | Estado de agente por hooks de Claude (permiso/notificación) con latencia ~0, opcional y con consentimiento | M | M | radisky `index.js:232-346`, `streamdock_notify.js` |
| 11 | Formato `.sdProfile`/`DataCache.db`/`StreamDockConfig.ini` (perfil activo, `ActionID` md5, `.A/.B.bak`) en `web` y `plugman` | M | A | radisky `deploy_vibe_plugin.js:28-60,285-340`; `install_and_launch.ps1:60-80` |
| 12 | Prueba de celdas laterales (80x80 vs 85x85 vs 82x82, recorte superior) | S | M | Deckhand `Widgets.swift:66-78`; 4ndv `mappings.rs:75-96` |
| 13 | Apagado limpio `CLE..DC`+`HAN` y `DIS` único por handle | S | M | mirajazz `device.rs:575`; Aiacos `akp05.md:48` |
| 14 | Guardas de config: `load_existing`, tests con ruta temporal, no retener lock | S | M | warley `CLAUDE.md` "Armadilhas" |
| 15 | Cruzar acciones "solo AJAZZ" (`setBG`, `clearIcon`, `setBackground`, `deleteAction`, `passHello`) con nuestro host | M | M | Aiacos `akp_plugin_sdk.md` §4.3-4.5 |
| 16 | Superficie Bitfocus Companion (Satellite) | L | B | dvortsis `src/satellite.js` |
| 17 | Pulsación larga/doble por temporizador (solo v2/v3; con down/up real) | M | A (v2/v3) | warley `runtime.rs:303-330`; mirajazz `state.rs:121-148` |
| 18 | Boot logo 854x480 (`LOG`) | L | B | Aiacos `akp153.md`; warley `hardware-checklist.md` |

## 9. Licencias (resumen)

| Repo | Licencia | ¿Reutilizar código? |
|---|---|---|
| mirajazz | MPL-2.0 | Solo ideas / reescritura; copiar ficheros mantiene MPL en ellos |
| 4ndv/opendeck-akp153 y fork | GPL-3.0 | No copiar código; datos (tablas) sí |
| Uriziel01 | Sin licencia | Solo hechos |
| warleyvic24 | MIT | Sí, con aviso de copyright |
| kruzif-x/deckhand | MIT | Sí, con aviso |
| dvortsis/companion-bridge | MIT | Sí, con aviso |
| radisky/akp153_plugin | MIT | Sí, con aviso |
| Aiacos/ajazz-control-center | GPL-3.0 | No copiar código; documentación solo como pista |
