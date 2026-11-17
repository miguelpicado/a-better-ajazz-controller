# Uso de ABAC

> English: [USAGE.en.md](USAGE.en.md).

`abac` es el sustituto ligero de «Stream Dock AJAZZ» para el teclado AKP153E. Lee los perfiles
de Stream Dock tal cual y ejecuta plugins del SDK v2 (`.exe` y Node) y acciones Orca integradas.

- Arquitectura y por qué de los tiempos: [ARCHITECTURE.md](../design/ARCHITECTURE.md).
- Compilar y probar: [DEVELOPMENT.md](DEVELOPMENT.md).
- Fallos y mensajes de error: [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
- Estado y pendientes: [HANDOFF.md](../project/HANDOFF.md), [STATUS.md](../project/STATUS.md), [ROADMAP.md](../project/ROADMAP.md).

## 1. Antes de empezar

1. **Detén Stream Dock** si está en marcha: `abac run` usa el mismo HID y el mismo puerto 18618.
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
   .\abac.exe probe
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
| `install` | `install <paquete>` | Abre el editor con el paquete preparado para instalarlo; nada se instala solo. | No |
| `register` | `register` | Asocia `.streamDeckPlugin` y `.sdPlugin` a ABAC para el usuario actual (sin admin). | No |
| `unregister` | `unregister` | Deshace la asociación y restaura la anterior. | No |
| `help` | `help`, `-h`, `--help` | Imprime la lista de comandos. | No |

Notas:

- `render` crea la carpeta de salida si no existe y escribe `key01.jpg`…`key18.jpg`. Las teclas
  1–15 sin acción se pintan en negro; las laterales 16–18 solo si la escena las usa.
- `scene`, `watch` y `poll` hacen `init(80)`: `DIS`, `LIG` y `CLE ff`, así que **borran la pantalla**
  antes de seguir. `bright` solo manda `LIG`.
- `watch` acepta un modo numérico (`watch 2` envía `CRT MOD` con ese valor), pero en el AKP153E v1
  `MOD` no cambia el comportamiento: el evento sigue llegando al soltar y con `state = 0`.
- `poll feature` devuelve «Función incorrecta» en esta unidad; es esperado. `poll input` devuelve la
  cadena de versión y no refleja una tecla mantenida.
- `abac help` (también `-h`/`--help`) imprime la lista de comandos. `edit --help` también existe.

## 3. Flujo de uso

### 3.1 Uso normal

```powershell
# 1. Parar la app oficial (mismo HID y puerto 18618)
schtasks /end /tn StreamDock

# 2. Arrancar ABAC con la escena Orca
cd daemon
.\target\release\abac.exe run "$env:APPDATA\HotSpot\StreamDock\profiles\R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile"
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
# Ctrl+C en la consola de abac, o cerrar la ventana
schtasks /run /tn StreamDock
```

No ejecutes `run` mientras Stream Dock está abierto: el `open` del HID o el `bind` del 18618 fallan.

## 4. Variables de entorno

Las rutas del demonio cuelgan de `%APPDATA%`; el editor mira además `%USERPROFILE%\Downloads` y el
autoarranque usa `%LOCALAPPDATA%`. Las variables `ABAC_*` se leen en el punto indicado; no hay un
fichero de configuración para ellas.

| Variable | Por defecto | Efecto | Dónde se lee |
|---|---|---|---|
| `ABAC_REPORT_LEN` | descriptor HID; si falla, 512 | Fuerza el tamaño `N` del payload de salida. Útil si el descriptor no se puede leer. | [`daemon/src/main.rs`](../../daemon/src/main.rs) |
| `ABAC_PLUGINS` | `%APPDATA%\HotSpot\StreamDock\plugins` | Carpeta de plugins para `run`, `render`, `scene` y `watch`; también la raíz de plugins del editor. | [`main.rs`](../../daemon/src/main.rs), [`edit.rs`](../../daemon/src/edit.rs) |
| `ABAC_PROFILES` | `%APPDATA%\HotSpot\StreamDock\profiles` | Carpeta de perfiles **solo para `edit`**. `run` recibe el perfil por argumento. | [`edit.rs`](../../daemon/src/edit.rs) |
| `ABAC_DATA` | `%APPDATA%\ABAC` | `state.json` y overlays **solo para `edit`**. `run` usa siempre `%APPDATA%\ABAC`. | [`edit.rs`](../../daemon/src/edit.rs) |
| `ABAC_BUILTIN_PLUGINS` | vacío; se añaden las carpetas `plugins` de Stream Dock si existen | Raíces de plugins de solo lectura extra para `edit`, separadas por `;`. | [`edit.rs`](../../daemon/src/edit.rs) |
| `ABAC_GAP_MS` | `250` | Mínimo de milisegundos entre lotes de pintado. Se lee al abrir cada sesión del dispositivo. **No bajar.** | [`app.rs`](../../daemon/src/app.rs) |
| acción `com.miguelpicado.orcadock.panel_strip` | _(escena)_ | **Experimental.** Panel lateral como UNA barra (hora/fecha, CPU, GPU, RAM, temperaturas; ver §4b). Colócala en la celda (5,0): con `ABAC_DRA=1` se dibuja entera (alto = `h` de la ranura 16 de calibración, largo 378); sin DRA se trocea en las 3 celdas de 80 px. Ver [`dra-render-notes.md`](../research/hid/dra-render-notes.md). | [`strip.rs`](../../daemon/crates/builtin/src/strip.rs) |
| acción `com.miguelpicado.orcadock.panel_weather` | _(escena)_ | Panel lateral como UNA barra (como `panel_strip`): hora de España y de Colombia sobre un dibujo del tiempo en Sada (sol y suelo verde, nubes y claros, nubes, niebla, cristal mojado con gotas si llueve, nieve, tormenta; luna y estrellas de noche), temperatura actual y máx/mín del día. Datos de Open-Meteo con `curl` cada 20 min (sin clave). En Grabación y Monitorización (5,0). Las barras laterales se envían con `ABAC_STRIP_PAD` px (16 por defecto, máx. 40) de más en cada extremo, del color del borde, para que no se vean cortadas. | [`weather.rs`](../../daemon/crates/builtin/src/weather.rs) |
| `ABAC_DRA` | _(sin definir)_ | **Experimental.** `1` = pintar con `DRA` (rectángulos en vivo, sin BAT/STP) en vez de BAT. Todas las teclas cambiadas en una pasada. Ver [`dra-render-notes.md`](../research/hid/dra-render-notes.md). Otras: `ABAC_DRA_GAP_MS` (50, pausa entre DRA), `ABAC_CALIBRATION` (ruta de `calibration.json`; por defecto `%LOCALAPPDATA%\ABAC\calibration.json`), `ABAC_CALIB_GAP_MS` (150), `ABAC_DRA_TILE_PX` (98, máx. 384), `ABAC_DRA_SIDE_PX` (80), `ABAC_DRA_OFFSETS` (`tecla:dx,dy;…` en píxeles de panel). | [`app.rs`](../../daemon/src/app.rs) |
| `ABAC_TRACE` | desactivado (cualquier valor lo activa) | Escribe a stderr cada entrada HID y la duración de cada lote. | [`app.rs`](../../daemon/src/app.rs) |
| `ABAC_CMD` | desactivado | Hook de experimento de `watch`: `VERBO:0a=01,0b=00` (offsets y valores en hexadecimal). Envía un `CRT` y sigue. | [`main.rs`](../../daemon/src/main.rs) |
| `ABAC_SCENE` | desactivado | Con `ABAC_CMD`: tras el comando, envía `LIG`, `CLE`, las imágenes de ese perfil (con `STP` por tecla) y `CONNECT`. | [`main.rs`](../../daemon/src/main.rs) |
| `QUOTA_DIR` | `%APPDATA%\ABAC\quota` (si no hay `APPDATA`: `<temp>\ABAC\quota`) | Carpeta de `quota-status.json` y `.wake` para las teclas de cuota. | [`builtin/src/actions.rs`](../../daemon/crates/builtin/src/actions.rs) |
| `APPDATA` | la del usuario de Windows | Base de todas las rutas por defecto. | `main.rs`, `app.rs`, `edit.rs`, `orca.rs` |

`ABAC_CMD` y `ABAC_SCENE` son herramientas de sondeo, no opciones de uso normal. `watch`
también hace `init`, así que pueden aparecer imágenes o limpieza de pantalla.

## 4b. Modo DRA: teclas a 98 px y barra lateral (estado 2026-11-16)

Con `ABAC_DRA=1` (el autoarranque ya lo pone en `abac-run.cmd`) o `"enabled": true` en `calibration.json`, `run` deja
de usar `BAT`/`STP` y pinta cada tecla como un rectángulo `DRA` en vivo. Cambios visibles frente al modo clásico:

| | Clásico (BAT) | DRA (actual) |
|---|---|---|
| Teclas 1-15 | 85×85 px | **98×98 px** por defecto (calibrable por tecla, máx. 384 de ancho) |
| Panel lateral | 3 celdas de 80×80 con huecos | **una barra continua** (alto = `h` de la ranura 16, 80 por defecto, largo 378) |
| Envío | un lote con `STP` | solo las teclas que cambian, `ABAC_DRA_GAP_MS` (50 ms) entre ellas |
| Pulsar/soltar | solo un clic al soltar | igual (el firmware no se ha parcheado; ver abajo) |

**Calibración por tecla.** En el editor, pestaña **Calibración**: pulsa físicamente una tecla (queda seleccionada y su acción
no se ejecuta), mueve el cuadro con las flechas y ajusta ancho/alto; se guarda en
`%LOCALAPPDATA%\ABAC\calibration.json` (`{"enabled":true,"keys":{"1":{"dx":0,"dy":-4,"w":98,"h":98},…}}`, píxeles de panel,
desplazamiento desde el centro de la celda BAT). Las teclas 16-18 comparten la ranura 16 (la barra se dibuja entera).
Sin fichero: 98×98 sin desplazamiento y barra de 80.

**Barra lateral** (`com.miguelpicado.orcadock.panel_strip`, colócala en la celda (5,0) y deja (5,1)/(5,2) vacías). De arriba abajo:
reloj/fecha, CPU, GPU, RAM, temperatura de CPU («CPU °C») y de GPU («GPU °C»), cada una con etiqueta, valor a la derecha y barra
de color (verde/ámbar/rojo según los umbrales de `sys.rs`). Sin HWiNFO64 («Shared Memory Support» activo) la temperatura de CPU
muestra «N/D». En el editor, con `panel_strip` en (5,0) el panel lateral se ve como UNA celda alta y el espejo recibe la barra completa.

**Tecla de resumen** (`com.miguelpicado.orcadock.summary`, «activos / espera / hechos»): al pulsarla enfoca el agente de Orca
que lleva más tiempo en espera (el único, si solo hay uno); sin agentes en espera no hace nada.

**Firmware pulsar/soltar (probado en hardware 2026-11-16).** Con el firmware original solo llega un informe al soltar (`state 0`). El parche v3 (`docs/research/firmware/work/make_patch_v3.py`, no se publica el binario) añade un informe al pulsar (`state 1`) y deja el de soltar igual; `run` lo detecta solo (`state 1` = pulsar, `0` = soltar; con el firmware original sigue tratando `0` como clic). Flasheo: `abac bootwatch 25` (la unidad entra en `EEEF:2929`), luego `FirmwareUpgradeTool.exe 768 4112 <bin> <icono>` ejecutada desde su carpeta y con las rutas entre comillas, y reenchufar el USB. Ver `docs/research/firmware/`.

**`STP` al arrancar.** El firmware solo reporta teclas tras su primer `STP` (flag en RAM, se pierde al reenchufar); `run` envía uno al abrir la sesión, antes de pintar.

## 5. `run` en detalle

- **Puerto**: 18618 fijo, sin opción. Si está ocupado, `run` falla al arrancar el host.
- **Perfil**: se lee al arrancar y al cambiar de escena. Las ediciones del editor no se recargan solas,
  salvo la recarga en caliente de una tecla cuando `edit` corre dentro de `run` (ver §7).
- **Escena Shift**: sí recarga el manifiesto de la escena destino (ver §6).
- **Lotes**: hasta el gap de espera entre lotes (250 ms por defecto, `ABAC_GAP_MS`; con
  `ABAC_GAP_MS=400` se recupera el hueco conservador). Es deliberado: el firmware pierde
  pulsaciones si se le pinta sin pausas.
- **Latido**: `CONNECT` cada 10 s.
- **Reconexión**: si el USB desaparece, `device: …; retrying in 2 s`; al volver, `init` y repintado.
- **Ajustes de plugin**: `setSettings` se escribe en el `manifest.json` de la celda con debounce de
  1 s; `setGlobalSettings` en `%APPDATA%\ABAC\globals\<plugin>.json`.
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

- El orden es el de la lista del dispositivo en `%APPDATA%\ABAC\state.json` (`order`). Si ese
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

### 6.1 Pulsación larga

Cada tecla puede tener un segundo destino para la pulsación larga (editor, sección «Pulsación larga»): ir a una escena o una acción integrada. Se guarda como campo `Long` de la acción en el manifest. Umbral: 500 ms (`ABAC_LONG_MS`). Con destino largo, la acción corta se decide al soltar (o la larga a los 500 ms, sin esperar a soltar); sin destino largo, la corta actúa al pulsar como siempre. Los plugins no pueden ser destino largo. Las teclas de agente de Orca (`com.miguelpicado.orcadock.agent`) sin destino largo tienen uno propio: «OK rápido», abre el agente y, si está preguntando, le manda Enter (acepta la opción marcada; si no pregunta, solo lo abre). Requiere el firmware pulsar/soltar: con el original (solo `state 0`) no hay pulsación larga y la tecla actúa como corta.

## 7. Editor web: `edit`

```powershell
cd daemon
.\target\release\abac.exe edit
# o:
.\target\release\abac.exe edit --port 8765 --no-open
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
  `abac bright`.
- El Property Inspector del plugin se carga en un iframe aislado (sandbox sin `allow-same-origin`)
  con token de un solo uso por `postMessage`; si el plugin no trae PI, el inspector muestra título
  y JSON de ajustes.

Para probar sin tocar tus perfiles reales, apunta el editor a copias:

```powershell
$env:ABAC_PROFILES = "C:\temp\perfiles-copia"
$env:ABAC_PLUGINS  = "C:\temp\plugins-copia"
$env:ABAC_DATA     = "C:\temp\datos-copia"
.\abac.exe edit
```

## 8. Instalar y quitar plugins desde `edit`

El editor usa `abac-plugman`; no necesita la app de Elgato ni descifra manifests del Marketplace.

### Doble clic en un paquete: `register`, `install`, `unregister`

```powershell
.\target\release\abac.exe register              # una vez, sin admin
.\target\release\abac.exe install C:\ruta\MiPlugin.streamDeckPlugin
.\target\release\abac.exe unregister            # deshacer
```

- `register` escribe solo en `HKCU\Software\Classes` (usuario, sin admin): crea el ProgID
  `ABAC.Plugin` con comando `«ruta de abac.exe» install "%1"` y asocia las extensiones
  `.streamDeckPlugin` y `.sdPlugin`. A partir de ahí, doble clic sobre el paquete (o «Abrir con» →
  ABAC) abre el editor en la página de instalación con ese paquete preparado.
- `install <paquete>` hace lo mismo desde la consola. Si `abac run` está vivo, pide el editor
  por el named pipe (la instalación se ve en caliente); si no, arranca un editor independiente.
  Ctrl+C cierra el editor, igual que en `edit`.
- La página muestra nombre, UUID, versión, autor y si ya hay una versión instalada. **Nada se
  instala solo**: hay que pulsar **Instalar plugin** (y confirmar si se reemplaza). Después,
  arrastra sus acciones a una tecla desde la vista **Teclas**.
- La ruta del fichero no viaja en la URL: el servidor la guarda tras un token de un solo uso. Si
  instalas y recargas la página, el enlace ya no sirve.
- Si mueves `abac.exe`, vuelve a ejecutar `register`.
- `unregister` borra el ProgID y devuelve a cada extensión el valor por defecto que tenía antes del
  primer `register` (se guarda como `PreviousDefault.<ext>` bajo el ProgID). Si Windows sigue
  abriendo otra app, elige ABAC en «Abrir con…» o restablece la asociación en Configuración:
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
   `%APPDATA%\ABAC\manifests\<uuid>.json`. Si es `partial`/`none`, el plugin se copia pero no se
   lanza hasta aportar un manifest del fabricante con `abac-plugman install --manifest <json>`.
6. **Quitar plugin** solo está disponible para plugins instalados por ABAC (`state.json` →
   `installed`). No borra integrados ni plugins que ya estaban antes. Para quitar uno que `run` está
   usando, para `run` primero.

Los `.js` necesitan `node` en `PATH`; los `.html` no se ejecutan. Detalle del formato y del
instalador de referencia: [`design/plugin-install-handoff.md`](../design/plugin-install-handoff.md) y
[`tools/legacy/plugin-install/README.md`](../../tools/legacy/plugin-install/README.md).

## 9. Autoarranque

[`tools/autostart/abac-task.ps1`](../../tools/autostart/abac-task.ps1) registra una tarea de
usuario normal (sin admin) que arranca `abac run` al iniciar sesión.

```powershell
# Copia el exe y libunwind.dll a %LOCALAPPDATA%\ABAC\bin y registra la tarea DESACTIVADA
.\tools\autostart\abac-task.ps1 install

# Actívala solo cuando la tarea de Stream Dock esté desactivada
# (esto último exige una consola elevada):
schtasks /change /tn StreamDock /disable
.\tools\autostart\abac-task.ps1 enable

.\tools\autostart\abac-task.ps1 status
.\tools\autostart\abac-task.ps1 disable
.\tools\autostart\abac-task.ps1 uninstall
```

- Perfil por defecto: `R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88` (la escena Orca). Se cambia con
  `-Profile <id>`.
- `-Build` apunta a `daemon\target\release` por defecto.
- Copia `libunwind.dll` junto al ejecutable: evita el error `-1073741515` al arrancar sin el `PATH`
  de llvm-mingw.
- Log: `%LOCALAPPDATA%\ABAC\abac.log` (se añade, nunca se rota).
- La tarea `ABAC` se registra desactivada a propósito; activarla con Stream Dock activo rompe
  ambos (mismo HID y puerto 18618).

## 10. Reglas duras

- **No flashear el firmware.** No lanzar `FirmwareUpgradeTool.exe` con 4 argumentos (flashea al
  instante, sin confirmación), ni enviar `CRT\0\0APP NEW` ni comandos de actualización. Solo con
  orden explícita del usuario y plan de recuperación.
- **No ejecutar Stream Dock y ABAC a la vez.**
- No borrar ni mover los ficheros grandes de la raíz del repositorio: son del usuario.

**Calibración por tecla (experimental, requiere modo DRA).** Con ABAC en marcha (`ABAC_DRA=1`), abre el editor → pestaña *Calibración* → «Calibración activa». Pulsa una tecla del dispositivo (o elígela en la pestaña): se dibuja un patrón de prueba (borde blanco, diagonales, número, marca roja arriba-izquierda). Mueve con las flechas (Mayús = 5 px) o los botones, ajusta w/h y desactiva la calibración: se guarda en `calibration.json` y se aplica al arrancar. Con «Usar DRA calibrado al arrancar» no hace falta `ABAC_DRA=1`. Detalles y supuestos sin probar: [`dra-render-notes.md`](../research/hid/dra-render-notes.md).
