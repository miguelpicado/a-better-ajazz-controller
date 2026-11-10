# Estado del proyecto y cómo continuar (2026-11-09, noche)

Punto de entrada para retomar. Contexto general en `README.md`; diseño en `design/architecture.md`.

## Dónde estamos

Sustituto en Rust de la app "Stream Dock AJAZZ" para el AKP153E: un único daemon (driver HID, host de plugins, servidor web
del editor), sin admin, que lee los perfiles actuales sin migrarlos. El análisis está hecho y el código existe en cinco
piezas independientes que **aún no están integradas** ni probadas con el dispositivo (los monitores, donde cuelga el USB,
estaban apagados).

Nada está commiteado: `daemon/`, `docs/`, `tools/plugin-install/`, `.opencode/` y `.playwright-mcp/` (basura de Playwright,
conviene añadirla a `.gitignore`) están sin seguimiento; `README.md` raíz tiene cambios. Commit solo con rutas explícitas y
push solo si lo pide el usuario.

## Piezas de código (todas con `cargo test` verde, verificado)

| Ruta | Qué es | Tests |
|---|---|---|
| `daemon/` (crate `orcadeck`) | Driver HID: protocolo `CRT`, render JPEG, lector de `.sdProfile`, CLI `render`/`probe`/`scene`/`bright`/`watch` | 7 |
| `daemon/crates/host` | Host de plugins: WebSocket 18618, lanzamiento `.exe`/`.js`, overlays, Origin allowlist, token de PI, contextos CSPRNG | 16 |
| `daemon/crates/plugman` | Port a Rust de `tools/plugin-install/install.mjs`: zip seguro, detección `ELGATO`, overlay, force+`.bak`, export/import. Idéntico a Node en los 20 plugins de esta máquina | 11 |
| `daemon/crates/builtin` | `orcadock` como acciones integradas (`.agent`, `.quota`, `.clock`, `.summary`), RPC a Orca por tubería, SVG con resvg | 6 |
| `daemon/crates/web` | Servidor HTTP en `std::net` + editor SPA de 44 KB (español), token/Host/Origin, lee `DataCache.db` | 8 |

Cada crate de `crates/` es independiente (su propio `[workspace]` vacío y `rust-toolchain.toml`); `daemon/Cargo.toml` no las
referencia todavía. Informes detallados de los workers: cada `src/` y el `README.md` de `web`.

## Sondeo en vivo (2026-11-10, hecho parcialmente)

- `probe` OK: descriptor 70 B, N = 512. `watch` **exige `init(80)` antes** (sin él no llegan teclas; ya añadido).
- Teclas 1–15, un solo evento por pulsación (`ACK\0\0OK\0\0<idx>`), `state` siempre 0. Medido con tecla mantenida ~5 s:
  **el evento llega al SOLTAR, no al pulsar** (nada mientras se mantiene). Con esto la duración de pulsación es
  invisible; larga/corta y down/up **no se pueden distinguir** con el firmware tal cual (requisito del usuario: los quiere).
  Probado `CRT MOD` con modos 0,1,2,3 (`orcadeck watch <modo>`, prueba con pitidos pulsar≈3 s / soltar≈9 s): el
  evento llega siempre ≈ al soltar, `state=0`; `MOD` no cambia nada. Sin otra interfaz HID (solo `0xffa0`).
  También probados sin efecto (`ORCADECK_CMD="VERB:off=val,..." orcadeck watch`): `QUCMD` 0/1, `SET` (0,0)/(1,0)/(0,1),
  `MOD` 4. Captura Frida de la app original: no recibe nada al pulsar (su código ignora `state`). **Conclusión: límite de
  hardware/firmware; solo hay evento "tecla pulsada" al soltar. Larga/corta y down/up no son posibles en el AKP153E.**
  Repos de terceros (clonados en /tmp/rr, informe en `community-ideas.md` si existe): solo el hardware v2/v3 (AKP153E REV2
  `0x3010`, firmware `V3.AKP153E.02.009`, ver `ramtic-lab/ajazz-pi`) y HSV293SV3 emiten down/up (byte 10 = 1/0). Probado
  también el init de `ajazz-pi` (QUCMD payload `1f 11 00 11 00 11`, imágenes+STP, CONNECT): sin cambio en nuestra unidad v1
  (`ORCADECK_CMD=... ORCADECK_SCENE=<perfil> orcadeck watch`). Firmware nuestro: `V2.293S.00.003`.
  El usuario ve una **línea amarilla mientras la tecla está pulsada, también sin la app** → el firmware detecta la pulsación
  internamente (feedback local), pero no la expone: `orcadeck poll input` (GET_REPORT id 0 cada 30 ms) devuelve siempre la
  cadena de versión sin cambios al mantener; los feature reports dan "Función incorrecta".
  Frida sobre `NtDeviceIoControlFile` (IOCTL de HID) del arranque de la app original: solo lecturas de informes
  (`0xb0193`), consultas de colección/descriptor y **un único `GET_INPUT_REPORT` (`0xb01a2`, cadena de versión)**; ningún
  `SET_FEATURE`/`SET_OUTPUT_REPORT` fuera de `WriteFile`. Es decir, la init oculta que USBPcap añadiría no existe: lo que
  vimos con `WriteFile` es toda la conversación de control. (USBPcap/Wireshark necesita driver y admin; no hace falta.)
  **Informe del worker Opus (`research/keystate-modes.md`, desensamblado de `%APPDATA%\HotSpot\StreamDock\update\AKP153E.bin`):
  el firmware solo envía el informe al soltar (byte [10] nunca se escribe); no existe comando de host que lo cambie.
  Manejadores: CLE STP BAT LOG DRA HAN DIS MOD LIG APP+NEW (`SET`/`QUCMD`/`CONNECT`/`VER`/`M_V` no tienen).
  NUNCA enviar `APP`+`NEW` (reinicio/bootloader). Down/up solo en firmware "293SV3" (nombre USB con espacio final).**
  Ideas de repos: `research/community-ideas.md`.
  **Firmware propio** (`research/firmware-feasibility.md`, `research/firmware-v3-search.md`): parche de 36 bytes diseñado
  (`research/firmware/work/AKP153E_pressrelease_NO_PROBADO.bin`, nunca ejecutado) pero **bloqueado**: el .bin no incluye el
  bootloader (0x08000000-0x08003FFF), no hay herramienta de flasheo instalada (`UpDateTool11-11.exe`, solo en el instalador) y
  no hay firmware V3 publicado. `APP`+`NEW` solo marca actualización y reinicia: riesgo de brick blando. Plan: UpDateTool
  estático -> chip/SWD -> volcado -> emulación -> flash solo por SWD, idealmente en unidad de sacrificio. Corrección: el USB
  anuncia fabricante "355" / producto "35549"; lo del espacio final del nombre queda descartado.
  **UpDateTool** (`research/updatetool-analysis.md`): `UpDateToolV2\UpDateTool11-11.exe` NO está en disco ni en el instalador
  actual (377 MB); `SDSetting::updateFirmware` lo lanza por ruta relativa. Los logs muestran el flujo real (2024-02-20 11:54:14
  updateFirmware y 3 s después el dispositivo desaparece = reenumera a bootloader; igual 2024-02-22/26, 2025-01-22). Las
  carpetas `%APPDATA%\HotSpot\Stream Dock AJAZZ Global 2.9.174/2.9.177.122/3.10.191.0421` están vacías (versiones anteriores
  desinstaladas, probablemente con la herramienta). Referencia AK820 (`HFD_IAPTool`): HID Win32, `McuProgramIAP_*`, VID/PID
  parametrizable, verificación solo CRC, sin firma; chip distinto, no drop-in. Hipótesis (INFERRED, media): el bootloader del
  AKP153E tampoco exige firma. Falta conseguir una build antigua de la app (≤ 2025-01) con UpDateToolV2.
  **Instaladores revisados** (561 MB `8e91648f…`, y el `.exe` de 276 MB dentro del .rar V2.10.185 de dic-2024, descomprimido
  por el usuario con B1 en la raíz del repo): contenedor MSI (Advanced Installer), sin `UpDateTool*`/`McuProgramIAP`; solo
  incluyen `FirmwareUpgradeTool.exe`, que es de otro SoC (Actions, `*.img`, uboot/ramboot, libusb) y NO sirve para el
  AKP153E. La herramienta que usó la app para el AKP153E (`UpDateToolV2`, feb-2024) no está en ninguna build disponible.
  Quedan: pedirla a soporte AJAZZ, buscar build ≤ feb-2024, o volcado SWD. Ficheros grandes del usuario en la raíz del repo
  (zip AK820, instalador 561 MB, .rar y su carpeta): fuera de git, no commitear (añadir a `.gitignore` o moverlos).
  **Búsqueda web de builds antiguas** (`research/old-builds-search.md`): sin mirrors/Wayback de instaladores antiguos. Dato:
  la build 2.9.177.122 (la del usuario en 2024) incluía `FirmwareUpgradeTool.exe` (~1,62 MB; la actual pesa 1 809 384 B), así
  que el actualizador de la época era esa herramienta, no `UpDateTool`. La actual solo habla `*.img`/uboot/ramboot/libusb
  (SoC Actions) pero enlaza hidapi (`hid_write`, `HidD_SetFeature`). Pista descartada: `RarSFX0\FirmwareUpdateTool.exe` del
  registro es de Logitech (`vid_046d`). Precedente CDN: `cdn1.key123.vip/VSDCraft/VSD%20M18UpDatetool.zip` (otra marca).
  Probado con OK del usuario (2026-11-10): `FirmwareUpgradeTool.exe` abierta sin fichero muestra "Updating… 0%" con Close
  desactivado: espera un dispositivo en SU modo (SoC Actions) y no reconoce el AKP153E; no escribió nada. Cerrada con
  Stop-Process en espera. ~~Conclusión: esa build de la herramienta NO es el actualizador del AKP153E.~~
  **CORREGIDO (worker Opus, `research/updatetool-origin.md`, 2026-11-10): `FirmwareUpgradeTool.exe` SÍ ES el actualizador del
  AKP153E y ya está instalado.** Con exactamente 4 argumentos (`768 4112 <fichero.bin> <icono>` = VID 0x0300, PID 0x1010 en
  decimal) actualiza el chip por HID; con cualquier otro nº de argumentos arranca el modo Actions ("Updating… 0%"), que es lo
  que vimos al abrirla a mano. **¡Con esos 4 argumentos empieza a flashear de inmediato, sin confirmación!** Protocolo: intenta el
  bootloader (EEEF:2929); si no, `CRT\0\0APP NEW` reinicia al bootloader; borrado, bloques de 60 B, verificación y reinicio
  (D2, D1, D3, AA 02; informes de 65 B). Solo CRC-16, sin firma (nuestro AKP153E.bin = 0xC065). La app 2.10.185 lo lanza con
  `downloadURL` del manifiesto; la actual solo permite actualización online a MBox-N1/N4, ajazzN1 y ControllerDeviceS1/S3
  (`NotOnlineDevice` para el AKP153E). `UpDateToolV2` NO tiene que ver con este dispositivo. La actualización real fue una
  sola vez (2024-02-20 11:54); los otros `updateFirmware` del log no desconectaron el dispositivo; "fabricante genérico /
  producto vacío" NO era el bootloader (la app antigua lo reportaba siempre). Quedan erróneos los textos previos sobre
  UpDateTool/SoC Actions/`RarSFX`. Copias extraídas (~330 MB) en `research/firmware/work/` (fuera de git).
  Manifiestos del CDN referenciados por la app (GET público de JSON, 2026-11-10): `StreamDock/firmware/firmware-version-libcef.json`
  y `…-check.json` solo listan MBox-N1 (`d13x_N1-HID_01.011.img`) y VSD_M18 (`d13x_V3.VSDM18.02.015.img`, md5 8e7f0fb1…):
  firmwares `d13x` = SoC Actions = `FirmwareUpgradeTool`. `Custom/Prajnasys/firmware/…` da 403; `Stock/…` 404. El AKP153E ya no
  tiene entrada (en feb-2024 sí se descargó `AKP153E.bin` desde ahí). Otras URLs: bucket `hotspot-oss-bucket.oss-cn-shenzhen`.
  Estático sobre `Stream Dock AJAZZ.exe` (2026-11-10): la app solo referencia `UpDateToolV2\UpDateTool11-11.exe` (0x25d81e8)
  con argumento `StreamDockUpDate` (0x25d8210) vía `QProcess::start(programa, QStringList)` + `setWorkingDirectory`, y la
  función `GetDevUpdateToolCanUpdate_Version`. NO referencia `FirmwareUpgradeTool.exe`. `FirmwareUpgradeTool.exe` no tiene
  parser de argumentos (sus "Invalid arguments" vienen de la librería USB). Changelogs públicos del bucket Alibaba (`*-Version_Windows.txt`):
  sin detalle de la herramienta ("7.增加更新固件确认弹窗" en Morphling 2.9.146).
  Segunda prueba (app abierta, Frida sobre libusb de la herramienta, 12+40 s): cero llamadas a libusb; con "Pause"
  LoadFile sigue desactivado: la herramienta arranca ya en modo "actualizando" y espera que la lance la app con fichero
  asignado; no se puede usar a mano. Strings: "usbupg", "image.target", "fwc", "*.img" (SoC Actions).
  Si algún día se cambia a hardware v2/v3: down/up reales y larga pulsación con temporizador de 550 ms (como `radisky`).
  Índices: filas = pasos de 3 (1,4,7,10,13 / 2,5,8,11,14 / 3,6,9,12,15).
- **Dibujo OK** (`orcadeck scene`): patrón capturado con Frida de la app original: `DIS`, `LIG`, `CLE ff`, las 15 imágenes
  seguidas (BAT+chunks sin pausas ni esperas de ACK), un único `STP` al final. El dispositivo **no manda ACK** de
  comandos/imágenes; esperarlos (1 s) y meter `STP`/pausas por imagen provocaba imágenes perdidas. `BAT` y `DIS` coinciden con
  `hid-protocol.md`. Captura: Frida `attach` al proceso `Stream Dock AJAZZ` hookeando `kernel32!WriteFile` (len 513).
  Pendiente: el usuario ve las teclas "desordenadas" respecto a la app (son placeholders; el contenido real lo
  pintan los plugins) — revisar mapeo `key_index` cuando haya contenido real.
- Hardware real: **sin dial/rotación y la pantalla lateral no es táctil** → ignorar celdas laterales y diales en `host`.
- Pendiente del sondeo: `scene <perfil>` y si `STP` es obligatorio. Stream Dock quedó **parado**
  (`schtasks /run /tn StreamDock` para volver).
- Toolchain: el `gnullvm` de rustup desapareció (rustup 1.29 rechaza ese nombre). Reconstruido en
  `C:\Tools\rust-gnullvm` (1.99.0, standalone) y enlazado como `gnullvm-local`; compilar con
  `RUSTUP_TOOLCHAIN=gnullvm-local` y llvm-mingw en `PATH`.

## Entorno de compilación (sin admin, sin MSVC)

- Toolchain `stable-x86_64-pc-windows-gnullvm` (fijado en `rust-toolchain.toml`).
- llvm-mingw portable en `C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin` debe estar en `PATH`.
- En bash: `export PATH="/c/Tools/llvm-mingw-20261104-ucrt-x86_64/bin:$PATH"` y, en una crate, `cargo test`.
- Los binarios gnullvm necesitan `libunwind.dll` en `PATH` al ejecutarse: al integrar, enlazar estático o distribuir la DLL.
- Ghidra 12.1.4 y JDK 21 en `C:\Tools` (solo para volver a analizar con REA).

## Pendiente, en orden

1. **Sondeo en vivo** (necesita monitores encendidos). Parar Stream Dock: `schtasks /end /tn StreamDock` (si no basta,
   cerrar el proceso desde una consola elevada). Luego, desde `daemon/`:
   `orcadeck probe` (solo lectura: descriptor y N), `orcadeck watch` (eventos de tecla crudos), `orcadeck scene <perfil>`.
   Cierra los 5 puntos de `research/hid-protocol.md` §9: tamaño real de reporte, estado de tecla, eventos de celdas
   laterales, dirección de la rotación, si `STP` es obligatorio. Escenas: `…\HotSpot\StreamDock\profiles\<ID>.sdProfile`
   (Orca R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88). Al terminar, volver a lanzar con `schtasks /run /tn StreamDock`.
2. **Integración en el daemon**: convertir `daemon/` en workspace con las 4 crates; un binario que una driver + host +
   builtin + web. Mapa `context → casilla` en el daemon (los contextos del host son aleatorios por `appear`). El registro
   de `builtin` va en su propio hilo (la llamada de workers tarda ~450 ms cada 10 s y no debe frenar el giro de 400 ms).
   Escribir de `Output::Image` (SVG/PNG de plugins) al pipeline de render del driver (limitar a ≤ 1 imagen / 400 ms por tecla).
3. **Carga de overlays** y estado `NeedsManifest` en la UI de `web`; instalar/quitar plugins desde la UI usando `plugman`;
   watcher de `Downloads`.
4. **Sustitución**: desactivar la tarea `StreamDock` y crear el autoarranque del daemon como tarea de usuario normal.

## Decisiones tomadas (no reabrir sin motivo)

- Rust, un binario, editor web servido por el daemon bajo demanda en loopback con token; sin UI nativa.
- Sin admin. Se pierden temperaturas CPU/GPU (SystemMonitor) y teclas hacia ventanas elevadas (UIPI).
- Plugins propios siempre con preferencia en Rust, como acciones integradas; `orcadock` migrado así (hecho en `builtin`, falta integrar).
  Los de terceros no se migran: `.exe` directo, Node y WebView2 como runtimes opcionales.
- Manifests cifrados de Elgato: **no se descifran** (DRM). Overlay reconstruido desde ficheros en claro
  (`design/plugin-install-handoff.md`). Descarga del Marketplace: navegador con la sesión del usuario + carpeta vigilada.
- El editor trabaja sobre la carpeta de perfiles de Stream Dock con Stream Dock parado; interfaz en español; borrado de
  escenas y páginas múltiples quedan para después.
- Feedback ok/alert de plugins: parpadeo breve de la tecla (a decidir en la integración). Carpeta de cuota
  (`C:/Users/migue/Github/quota-monitor`, `QUOTA_DIR`) pasa a ajuste del daemon.

## Cabos sueltos

- Zips sin límite de tamaño/ratio (zip bomb) en `plugman`; añadir antes de aceptar paquetes de fuentes no fiables.
- `--force` que ya no sale `full` deja el overlay antiguo (igual que Node): decidir si se borra.
- `raise()` de `builtin` (sustituye a `raise.ps1`) no se pudo probar sin tocar el Orca en marcha.
- Sin reinicio de plugins caídos, sin plugins `.html` (WebView2 opcional), sin diales ni extras de Stream Dock en `host`.
- `manifest.json.A.bak`/`.B.bak` aparecen cada 10 min en el perfil Orca: parecen copias propias de Stream Dock (confirmar).
- Sin explicar: logs con plugins cifrados "conectados" hasta las 15:31 del 2026-11-09 (hipótesis: procesos que sobrevivieron).
- Ajustes globales de plugin de Stream Dock y payloads de `sendToDevice`/`getScreenshot`: sin investigar.
- La tarea `StreamDock` sigue en RunLevel Highest; cambiarla exige consola elevada.
- Nunca borrar nada que no se haya nombrado por ruta exacta (lección de `orca-dock`, que no es de este repo).
