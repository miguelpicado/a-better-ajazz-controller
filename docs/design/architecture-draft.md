# Diseño: demonio Rust para Stream Dock AKP153E (borrador, 2026-11-09)

Objetivo: sustituir "Stream Dock AJAZZ" (Qt + CEF, 12 procesos, ~330 MB en el proceso principal, elevado a admin) por
un demonio mínimo. Prioridad del usuario: ágil, ligero, ultraoptimizado, sin interfaz nativa. Fuentes:
`docs/research/hid/hid-protocol.md`, `plugin-host-protocol.md`, `manifest-profiles-admin.md`, `elgato-plugin-encryption.md`.

## Decisiones

- **Un único binario Rust, sin UI residente.** Hace tres cosas: driver HID, host de plugins (WebSocket) y servidor
  HTTP local. Objetivo de reposo: unos pocos MB de RAM y 0 % CPU.
- **Editor web servido por el propio demonio** en `127.0.0.1:<puerto>`, abierto bajo demanda en el navegador. Cuando
  no se edita no existe ningún proceso de interfaz (no hay CEF). El icono de bandeja es opcional y solo abre esa URL.
- **Property inspectors de los plugins se cargan en esa misma web** (iframe/página servida por el demonio, con el
  puente `connectElgatoStreamDeckSocket`). Esto elimina la necesidad de CEF.
- **Sin admin.** Verificado en la investigación: HID, hooks, entrada simulada, detección de app en primer plano,
  volumen y portapapeles funcionan sin elevar. Se pierde solo: temperaturas de CPU/GPU (driver de kernel) y teclas a
  ventanas elevadas; este último caso, con un helper pequeño opcional si hace falta.
- **Formato de perfiles compatible**: lee los `.sdProfile\manifest.json` actuales (`Actions["col,row"]`) tal cual,
  así tus escenas no se migran. Orden de escenas y escena actual en ficheros propios.

## Política de plugins

- Los plugins propios se escriben en Rust con preferencia, por economía de recursos, y cuando procede como **acciones
  integradas** en el daemon (sin proceso aparte ni WebSocket). `orcadock` (hoy Node) se migrará así.
- Los plugins de terceros no se migran (código ajeno): el host compatible los ejecuta. `.exe` directo; Node y WebView2
  son runtimes opcionales bajo demanda, solo para plugins que los necesiten (p. ej. Insta360, camerahub).
- Las acciones integradas y los plugins externos conviven bajo el mismo modelo de escena (`Actions["col,row"]` con
  `UUID`); una acción integrada se identifica con un UUID propio y no lanza proceso.

## Componentes

1. **hid**: crate `hidapi`. Abre VID 0x0300 / PID 0x1010 (preferir interfaz vendor). Envía comandos `CRT\0\0…` (DIS,
   LIG, CLE, BAT+JPEG, STP, CONNECT cada 10 s), responde `HANLDER` con `HAN`, lee eventos de tecla (byte 9 = tecla
   1..18, byte 10 = estado). Cola de escritura con espera de `ACK..OK` (máx. 1 s). Reconexión al volver el USB.
2. **render**: imágenes de tecla 85×85 y celdas laterales 80×80, JPEG rotado 270°, calidad descendente hasta ≤ 9000
   bytes. Un decodificador/codificador JPEG puro Rust (`image` o `zune-jpeg`/`jpeg-encoder`) y SVG (`resvg`) para los
   iconos de plugins y los de tipo tarjeta que hoy genera `core.js`.
3. **host de plugins**: un WebSocket (`tokio-tungstenite`) en el puerto fijo 18618 con el protocolo del SDK de Elgato
   v2 más los controladores propios de Stream Dock (`Information`, `SecondaryScreen`, `Knob`). Lanza cada plugin con
   `-port -pluginUUID -registerEvent -info`. `.exe` directo; `.js` con `node.exe` (versión indicada por
   `Nodejs.Version`); `.html` solo si hace falta (ver riesgos). Enruta por `context`. Descarta comandos desconocidos.
4. **store**: perfiles, ajustes de acción y ajustes globales en JSON bajo `%APPDATA%\<app>\`. Superposición de
   manifests (`manifests/<uuid>.json`) para plugins con manifest cifrado: sustituye al manifest ilegible sin tocar la
   carpeta del plugin.
5. **plugin manager**: instala `.streamDeckPlugin`/`.sdPlugin` (zip) en la carpeta de plugins, valida el manifest y
   genera la superposición reconstruida cuando el manifest empieza por `ELGATO`. Implementación de referencia y
   contrato para el port: `docs/design/plugin-install-handoff.md`.
6. **servidor web**: `axum`, API REST/WS local + frontend estático embebido en el binario (build previo, sin Node en
   ejecución). Solo escucha en loopback; token de sesión para impedir que cualquier página abierta en el navegador
   modifique la configuración.

## Frontend del editor

Una SPA pequeña (Svelte/Solid/preact, resultado de unos pocos cientos de KB embebidos): cuadrícula 5×3 + 3 celdas
laterales, arrastrar acciones desde la lista de plugins, escenas y páginas, property inspector del plugin
seleccionado, gestor de plugins (instalar, quitar, ver manifest reconstruido), brillo y estado del dispositivo.

## Fases

0. **Sondeo en vivo del dispositivo** (monitores encendidos, Stream Dock cerrado): tamaño real de reporte, estado de
   tecla, eventos de celdas laterales, rotación. Resuelve los 5 puntos abiertos de `hid-protocol.md` §9.
1. **Driver**: CLI Rust que enciende, pone brillo, dibuja imágenes y lee teclas.
2. **Escenas**: carga de `.sdProfile` actuales, render de tecla desde sus `States`, paginación/cambio de escena.
3. **Host de plugins** con nuestro `orca-stream-dock` como primer plugin real; después Zoom e Insta360.
4. **Servidor web y editor.**
5. **Gestor de plugins y manifests reconstruidos.**
6. **Sustitución**: desactivar la tarea `StreamDock` y poner el autoarranque del demonio como tarea de usuario normal.

## Riesgos y preguntas abiertas

- **Plugins HTML** (camerahub, windowscockpit) necesitan un navegador oculto. Sin CEF, la opción es lanzar un
  WebView/Edge oculto solo para esos plugins, bajo demanda. Decidir si merece soportarlos.
- **Node**: los plugins JS necesitan `node.exe`. Opciones: depender de Node instalado, descargar la versión que pida el
  manifest, o reutilizar `node20.exe` de la instalación de Stream Dock mientras exista.
- **Comandos propios de Stream Dock** (`sendToDevice`, `getScreenshot`, `getSystemAudioVolume`): solo se conocen los
  nombres; los payloads están por investigar si algún plugin los usa.
- **Ajustes globales** de plugin: no se sabe dónde los guarda Stream Dock (hipótesis: Credential Manager).
- **Instalación de plugins del Marketplace de Elgato sin la app de Elgato**: la instalación y la reconstrucción del
  manifest están resueltas (ver `plugin-install-handoff.md`); la descarga sigue requiriendo la cuenta/sesión del
  Marketplace. El manifest cifrado no se descifra (ver `elgato-plugin-encryption.md`).
- **Entrada simulada en ventanas elevadas**: sin admin falla (UIPI); helper opcional.
