# Investigación Stream Dock AJAZZ: resumen de lo averiguado (2026-11-09)

Objetivo: sustituir "Stream Dock AJAZZ" por una app propia, ligera y sin admin, que ejecute plugins de Stream Deck,
Stream Dock o propios sin tener instaladas ninguna de las dos apps. Todo se obtuvo por análisis estático (REA con
Ghidra 12.1.4, inspección de ficheros, logs). **Nada se ha enviado todavía al dispositivo.**

**Para retomar el trabajo, empieza por `STATUS.md`** (estado, entorno de compilación y pasos en orden).

Documentos:
- `research/hid-protocol.md`: protocolo HID del AKP153E.
- `research/plugin-host-protocol.md`: cómo Stream Dock aloja plugins (WebSocket, manifest, lanzamiento).
- `research/manifest-profiles-admin.md`: manifests cifrados de Elgato, formato de perfiles, causa del admin.
- `research/elgato-plugin-encryption.md`: registro del cifrado de manifests de Elgato.
- `design/architecture.md`: diseño propuesto del demonio Rust.
- `design/plugin-install-handoff.md`: instalación nativa de plugins sin la app de Elgato (resuelto; prototipo en
  `tools/plugin-install/`, 5/5 plugins cifrados reconstruidos, a portar a Rust).

## Entorno y herramientas

- Aplicación: `C:\Program Files (x86)\Stream Dock AJAZZ Global\` (Qt5 + CEF). Datos: `%APPDATA%\HotSpot\StreamDock\`.
- Dispositivo: AKP153E, HID `VID_0300 PID_1010`, 15 teclas LCD 5×3 + panel lateral de 3 celdas sin botón.
- Consumo observado de la app oficial: 12 procesos `CefViewWing` (~50 MB cada uno), 2 `node20`, proceso principal
  ~330 MB.
- Instalados en esta máquina, sin admin: JDK 21 (`C:\Tools\jdk\jdk-21.0.13+9`) y Ghidra 12.1.4
  (`C:\Tools\ghidra_12.1.4_PUBLIC`). REA 6.8.0 exige Ghidra 12.1.x (12.2 no vale). Variables para REA:
  `JAVA_HOME`, `GHIDRA_INSTALL_DIR`. Rust: toolchain `stable-x86_64-pc-windows-gnullvm` (fijado en `daemon/rust-toolchain.toml`). No hay linker MSVC ni
  admin, así que se usa llvm-mingw portable en `C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin`, que debe estar en
  `PATH` al compilar.
- Reiniciar Stream Dock desde una sesión remota: `schtasks /run /tn StreamDock`. Lanzar el exe a mano pide UAC y se
  queda colgado.

## Por qué corre como administrador

- El exe tiene manifest `asInvoker`. La elevación viene de una marca de compatibilidad `RUNASADMIN` en HKLM y de la
  tarea programada `StreamDock` (RunLevel Highest, `/background`), creada para evitar el UAC. Cambiar la tarea requiere
  una consola elevada (desde una sesión normal da "Acceso denegado").
- Funcionan sin admin: HID, hooks de teclado/ratón, entrada simulada, detección de app en primer plano, volumen,
  portapapeles, captura de pantalla, autoarranque en HKCU.
- Necesitan admin: temperaturas/potencia de CPU y GPU (el plugin SystemMonitor carga un driver de kernel) y enviar
  teclas a ventanas elevadas (UIPI; Elgato usa un helper firmado).

## Protocolo HID (resumen; detalle y direcciones en `hid-protocol.md`)

- Reportes de salida con ID 0x00, `1 + N` bytes (N = longitud de reporte − 1, sin confirmar: probablemente 512 o
  1024). Comandos: prefijo `CRT\0\0` + verbo. `DIS` despertar, `LIG` brillo (0–100 en `[0x0a]`), `CLE` borrar
  (`[0x0b]` = tecla, 0xFF = todas), `STP` refrescar, `CONNECT` latido cada 10 s, `BAT` cabecera de imagen
  (longitud BE16 + tecla) seguida del JPEG en trozos de N bytes y espera de `ACK..OK` (≤ 1 s).
- Imágenes: JPEG, 85×85 teclas y 80×80 celdas laterales, rotado 270°, calidad bajando de 4 en 4 hasta ≤ 9000 bytes.
- Teclas: fila superior 13, 10, 7, 4, 1; media 14, 11, 8, 5, 2; inferior 15, 12, 9, 6, 3; celdas laterales 16, 17, 18.
- Entrada: tecla en el byte 9, estado en el 10. La app oficial ignora el estado para este modelo.
- Pendiente de confirmar en vivo: N real, estado de tecla, si las celdas laterales emiten eventos, dirección de la
  rotación, si `STP` es obligatorio por imagen. El dispositivo cuelga del hub USB del monitor; con los monitores
  apagados no aparece en el sistema.

## Plugins

- Un único WebSocket en el puerto fijo 18618, mismo formato y eventos que el SDK v2 de Elgato. Cada plugin se lanza
  con `-port 18618 -pluginUUID <32 chars> -registerEvent registerPlugin -info <json>`. `.exe` directo, `.js` con
  `node20.exe`, `.html` en una página CEF oculta.
- Extensiones de Stream Dock: controladores `Information` (celdas laterales), `SecondaryScreen`, `Knob`; comandos
  `sendToDevice`, `getScreenshot`, `getSystemAudioVolume` (solo se conocen los nombres). No tiene
  `setFeedbackLayout` ni `setTriggerDescription`. Las imágenes enviadas a menos de ~400 ms se descartan.
- Ajustes de acción en el `manifest.json` de cada perfil. Ajustes globales: ubicación sin confirmar.
- Perfiles: `profiles\<ID>.sdProfile\manifest.json` en JSON plano con `Actions["col,row"]` (columna 5 = panel lateral).
  Orden de escenas en `config\DataCache.db`, escena actual en `StreamDockConfig.ini`. `tools/streamdock/profiles.mjs`
  ya escribe ese formato.
- Manifests cifrados de Elgato: ver `research/elgato-plugin-encryption.md`. No se descifran; solo está cifrado el
  manifest, el resto del plugin está en claro. Plugins cifrados aquí: advancedlauncher, camerahub, obsstudio,
  volume-controller, insta360 (Elgato) y windowscockpit.

## Inventario de plugins por tecnología de ejecución (esta máquina)

| Tecnología | Plugins |
|---|---|
| `.exe` nativo | SystemMonitor, windowsgizmos, Zoom, switchAudio, OBSBOT, NVIDIA Broadcast, bluetooth.monitor |
| Node | insta360 (`bin/plugin.js`), orcadock (nuestro `plugin.js`) |
| HTML | memo, time, weather, calendar, neewer, web-requests, tutorial |
| Manifest cifrado | advancedlauncher, camerahub, obsstudio, volume-controller, windowscockpit |

## Viabilidad de prescindir de node.exe y del navegador HTML (mi valoración)

- **Plugins `.exe`:** se ejecutan tal cual, sin Node ni navegador. Es la mitad del inventario y no cambia nada.
- **Nuestro plugin (Node):** se puede reescribir en Rust y, mejor, convertirlo en una acción integrada dentro del
  propio demonio (sin proceso aparte ni WebSocket). La lógica de `core.js` es pequeña: RPC a Orca, render SVG, cuota.
  Hay que mantener `test.js`-equivalente como tests de Rust.
- **Plugins HTML de Stream Dock (reloj, calendario, tiempo, memo):** son widgets sencillos. Se pueden reemplazar por
  acciones integradas que dibujan SVG, sin navegador. Es el caso más rentable.
- **Insta360 (Node, 1497 ficheros, SDK incluido):** reescribirlo exige reimplementar su protocolo con la cámara, que
  se puede leer en su `bin/plugin.js` (JS legible, no cifrado), pero es trabajo de reingeniería por cuenta de un
  tercero. Alternativa: un modo "compatibilidad" opcional que lance Node solo si el plugin lo pide.
- **Plugins HTML de terceros y los de manifest cifrado con HTML (camerahub, windowscockpit):** no se pueden migrar de
  forma general. Para ejecutarlos hace falta un motor web. Opción: WebView2 de Windows (ya instalado en Windows 11)
  lanzado oculto y bajo demanda, sin CEF propio.
- **Conclusión:** migrar a Rust todos los plugins de terceros no es viable (son código ajeno, a menudo cerrado). Sí lo
  es y compensa migrar los tuyos y los widgets básicos como acciones integradas, y dejar el host compatible con
  `.exe` por defecto y Node/WebView2 como runtimes opcionales descargables o detectados bajo demanda. Así el caso
  normal queda sin Node ni navegador, y cualquier plugin sigue pudiendo ejecutarse.

## Decisiones del usuario (2026-11-09)

- **Limitación asumida:** no es viable migrar a Rust los plugins de terceros (código ajeno, a menudo cerrado). El
  daemon los ejecuta por compatibilidad: `.exe` directo; Node y WebView2 son runtimes opcionales, usados solo si un
  plugin concreto los pide. Insta360 y los plugins HTML de terceros dependen de esos runtimes.
- **Política para plugins propios:** se escriben siempre en Rust con preferencia, por economía de recursos. Cuando
  tenga sentido, como acciones integradas en el propio daemon (sin proceso aparte ni WebSocket).
- **Migración de orcadock:** el plugin actual (`com.miguelpicado.orcadock.sdPlugin`, Node: `core.js` + `plugin.js`)
  se reescribirá como acción integrada dentro del daemon. Cubre la lógica de `core.js` (RPC a Orca por la tubería de
  `%APPDATA%\orca\orca-runtime.json`, render SVG de teclas, cuota desde `quota-status.json`, `.wake`, parpadeos,
  badge de workers) y los tests de `test.js` pasan a tests de Rust. Hasta entonces el plugin Node sigue funcionando
  sobre el host compatible.
- **Interfaz:** web, servida por el propio daemon bajo demanda (sin interfaz nativa).
- **Widgets básicos** de Stream Dock (reloj, calendario, tiempo, memo): acciones integradas que dibujan SVG.

## Estado del código (2026-11-09, sin dispositivo)

`daemon/` (crate `orcadeck`, 7 tests en verde, release ~0,7 MB): `proto.rs` (comandos CRT, BAT, parseo de entrada,
mapa de teclas, N desde el descriptor HID), `render.rs` (85/80 px, rotate270, JPEG ≤ 9000 B), `profile.rs` (lee
`.sdProfile` tal cual) y CLI: `render` (offline, probado con la escena Orca), `probe` (solo lectura), `scene`,
`bright`, `watch`. Hallazgo: `States[].Image` es un nombre suelto en `<perfil>/Images/` o una ruta relativa al plugin
dueño (sin extensión); los plugins repintan en vivo, la imagen estática es solo el marcador.
Sin probar contra hardware: `probe`, `scene`, `bright`, `watch`.

## Pendiente

1. Sondeo HID en vivo (monitores encendidos, Stream Dock cerrado).
2. Fase 1 hecha en código; falta validarla con el dispositivo (`probe` → `watch` → `scene`) y resolver los 5 puntos de `hid-protocol.md` §9.
3. Decidir qué widgets y plugins se integran de forma nativa y cuáles solo por compatibilidad.
4. ~~Instalar plugins del Marketplace sin la app de Elgato~~ Resuelto en `design/plugin-install-handoff.md`: el
   usuario descarga el `.streamDeckPlugin` con su sesión del navegador, una carpeta vigilada lo instala y el manifest
   cifrado se sustituye por un overlay reconstruido (sin descifrar). Falta portar el instalador a Rust
   (`plugin_manager.rs`) y cargar overlays en el host. Caso residual: Window Cockpit (necesita overlay exportado).
5. Confirmar la ubicación de los ajustes globales de plugin y los payloads de `sendToDevice`/`getScreenshot`.
