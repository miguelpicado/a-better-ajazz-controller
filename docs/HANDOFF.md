# HANDOFF (2026-11-10) — leer primero, luego `docs/STATUS.md`

Repo `C:\Users\migue\Github\orca-stream-dock`, rama `main`. **Nada commiteado** (daemon/, docs/, tools/, .opencode/ sin seguimiento;
README.md modificado). Commit solo con rutas explícitas y push solo si el usuario lo pide. Idioma: español, estilo terso.

## Reglas duras (no negociables)
1. **NO flashear nada y NO lanzar `FirmwareUpgradeTool.exe` con 4 argumentos** (`768 4112 <bin> <icono>` flashea de inmediato, sin
   confirmación). Tampoco enviar `CRT…APP NEW` ni ningún comando CRT de actualización. Solo con orden explícita del usuario.
2. Nunca borrar nada que no se haya nombrado por ruta exacta. Los ficheros grandes de la raíz son del usuario: no moverlos/borrarlos.
3. Preguntas al usuario con `AskUserQuestion` (regla global).

## Entorno de compilación (Windows, sin admin)
```
cd C:\Users\migue\Github\orca-stream-dock\daemon
$env:PATH="C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:RUSTUP_TOOLCHAIN="gnullvm-local"      # toolchain reconstruido en C:\Tools\rust-gnullvm (1.99.0); NO tocar rust-toolchain.toml
cargo build --release ; cargo test --release   # 7 tests verdes
```
Binario: `daemon\target\release\orcadeck.exe`. Subcomandos: `render`, `probe`, `scene <perfil>`, `bright`, `watch [modo]`,
`poll input`, `hidwatch` (solo enumera HID; marca `EEEF:2929` = bootloader). Stream Dock se para con `schtasks /end /tn StreamDock`
y se relanza con `schtasks /run /tn StreamDock` (**ahora está en marcha**). Perfil Orca:
`%APPDATA%\HotSpot\StreamDock\profiles\R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile`.

## Hardware real (AKP153E v1, VID 0x0300 PID 0x1010, fw V2.293S.00.003)
- Teclas 1–15, filas en pasos de 3 (1,4,7,10,13 / 2,5,8,11,14 / 3,6,9,12,15). **Sin dial y la pantalla lateral no es táctil.**
- **Un solo evento por pulsación y llega AL SOLTAR; `state` siempre 0.** No hay pulsar/soltar ni pulsación larga con este firmware.
  Ninguna orden de host lo cambia (probados MOD, SET, QUCMD, init de ajazz-pi…). El firmware dibuja una línea amarilla local al pulsar.
- Dibujo OK con el patrón de la app original (Frida): `DIS`, `LIG`, `CLE ff`, 15 imágenes seguidas, **un** `STP`; sin ACK, sin pausas.
  `watch` necesita `init` (el primer STP activa el reporte de teclas).

## FIRMWARE (la vía abierta) — corregido en esta sesión
- **`FirmwareUpgradeTool.exe` (ya instalada en `C:\Program Files (x86)\Stream Dock AJAZZ Global\`) ES el actualizador del AKP153E.**
  Con 4 args actualiza por HID; con otro nº de args entra en modo Actions ("Updating… 0%"). Protocolo: `EEEF:2929` (bootloader) o
  `CRT\0\0APP NEW` → reinicio; borrar (D2), escribir 60 B (D1), verificar CRC (D3), reiniciar (AA 02). **Solo CRC-16/ARC, sin firma**
  (original = `0xC065`, verificado). Informe: `docs/research/updatetool-origin.md`. Los textos previos sobre UpDateTool/SoC Actions
  están corregidos en STATUS (y `updatetool-analysis.md`/`firmware-feasibility.md` pueden conservar frases erróneas sobre ello).
- Firmware original: `docs/research/firmware/backup/AKP153E.bin` (96 496 B, SHA-256 `15e03f5b5a593d66…`, base `0x08004000`; idéntico a
  `%APPDATA%\HotSpot\StreamDock\update\AKP153E.bin`). Bootloader (0x08000000-0x08003FFF) NO volcado.
- Parche propuesto (NO PROBADO): `docs/research/firmware/work/AKP153E_pressrelease_NO_PROBADO.bin` (SHA a7ea1549…, CRC 0x9991): 50 bytes
  en 7 bloques (6 llamadas redirigidas 0x08006f92…0x080070bc + stub de 36 B en 0x0801ab9c). Emite pulsar(1)/soltar(0).
  Riesgo: si el código parcheado se cuelga, la app no recibe `APP NEW`; recuperación solo por SWD/pin BOOT (desconocidos).
- **Worker Opus TERMINADO** (2026-11-10): `docs/research/firmware/patch-emulation.md`. Veredicto: **v1 NO flashear** (pierde el informe de
  soltar si el host tarda ≥~5 ms → tecla "pegada"). **v2** = `work/AKP153E_pressrelease_v2_NO_PROBADO.bin` (SHA 4075f5e5…5131a,
  CRC 0x18A7; espera acotada a EP libre; 88 B en 0x0801AB9C; generador `work/make_patch_v2.py`, emulación en `work/emu/`).
  Código v2 ≈80 % correcto; flashear sigue exigiendo SWD/unidad de sacrificio. Host debe ignorar `state=1` de tecla ya pulsada
  (ACK del ISR puede duplicarla). Con la app oficial cada acción se dispara dos veces: usar solo `orcadeck`.
- **Handoff del worker de emulación:** su resumen final llega a la sesión que lo lanzó (la antigua). Se le pidió que guarde progreso en
  `patch-emulation.md` (secciones Estado/Hecho/Hallazgos/Pendiente/Cómo reproducir + bloque VEREDICTO) y deje scripts en
  `docs/research/firmware/work/`. Al empezar: leer ese fichero. Si no existe o no tiene VEREDICTO, relanzar con el prompt guardado en
  `docs/research/firmware/work/emulation-worker-prompt.md` (Agent tool, Opus, segundo plano), pidiéndole que continúe desde "Pendiente".
- Siguiente (en este orden, sin flashear): (1) ~~revisar `patch-emulation.md`~~ hecho; (2) revisar el diff v2 a mano con el usuario;
  (3) si el usuario lo decide: ensayo de "reflasheo idéntico" del original con la herramienta oficial **dejando `orcadeck hidwatch`
  corriendo** para ver `EEEF:2929`; (4) solo después, el parche. Mejor con plan de recuperación (programador SWD / unidad de sacrificio).
  Una unidad AKP153E rev. 2 (PID 0x3010, fw V3) sí emite down/up (ver `ramtic-lab/ajazz-pi`, `4ndv/opendeck-akp153`): alternativa barata.

## PRODUCTO — integración hecha (2026-11-10), sin probar con el dispositivo
`daemon/` ya es workspace (host, plugman, builtin, web; un solo `Cargo.lock`; 53 tests verdes con `cargo test --release --workspace`).
Nuevo subcomando **`orcadeck run <perfil.sdProfile>`** (`daemon/src/app.rs`): driver + host de plugins (puerto 18618) + acciones
`builtin` en un proceso. Hilo principal = dueño del HID (entrada, pintado, heartbeat, reconexión cada 2 s con repintado);
hilo `builtin` = `Registry` (tick 400 ms, pulsación por canal); imágenes de plugins (`setImage` data URI, SVG/PNG/JPEG) y de
builtin llegan por un canal; máx. 1 imagen/400 ms/tecla (gana la última), un `STP` por lote. Pulsación (llega al soltar) =
click: builtin `key_down`, plugin `keyDown`+`keyUp`. Solo se lanzan los plugins que usa la escena. Parar Stream Dock antes
(`schtasks /end /tn StreamDock`): mismo HID y mismo puerto.
**Probado en vivo (2026-11-10, perfil Orca): funciona** (pintado, gif/spinner, pulsación → enfocar terminal, cuota ok). Hallazgo clave:
**el firmware pierde pulsaciones si se le pinta casi sin pausa** (envíos por tecla cada ~120 ms: cero pulsaciones en 80 s; sin línea
amarilla). Cada imagen cuesta ~25-30 ms de escritura HID fijos (independiente del tamaño). Arreglo: lotes (todas las teclas
pendientes juntas, `ORCADECK_GAP_MS`, 400 por defecto) + JPEG q75 (3× menos bytes) → pulsaciones fiables según el usuario.
Traza: `ORCADECK_TRACE=1` (entradas HID y duración de cada lote). Ejecutar fuera de la shell con `llvm-mingw\bin` en `PATH`
(`-1073741515` = falta libunwind.dll). Pendiente de la prueba: `Scene Shift` (`com.hotspot.streamdock.profile.rotate`) sin manejador;
retardo pulsación→acción pequeño pero visible (Orca RPC).
Stream Dock quedó relanzado (`schtasks /run /tn StreamDock`).
Después: servidor `web` bajo demanda (`orcadeck edit`), overlays/UI de plugman, feedback ok/alert (parpadeo), títulos/estados de
plugins (`Output::Title/State` hoy ignorados), el host debe ignorar `state=1` duplicado si algún día se flashea el parche,
sustitución (tarea de autoarranque). Ideas de terceros: `docs/research/community-ideas.md` (licencias: MIT reutilizable;
MPL/GPL/sin licencia solo hechos).

## Higiene
- `.gitignore` actualizado (target/, .playwright-mcp/, ficheros grandes de la raíz, `docs/research/firmware/work/extract-*|current-*|venv`).
- Ficheros del usuario en la raíz sin tocar: zip AK820, instalador 561 MB, `.rar` y su carpeta descomprimida.
- Clones temporales de repos de terceros en `/tmp/rr` y `/tmp/ajz` (Git-Bash), borrables.

## app feedback (worker app-feedback)
`daemon/src/app.rs` ya dibuja lo que antes solo se ignoraba o se registraba en log:
- **Title**: compone una barra inferior translúcida con el texto (Segoe UI, resvg) sobre la última
  imagen de la tecla; la barra rasterizada se cachea por tecla hasta que cambie el título.
- **State**: recuerda el estado por tecla (un `setState` repetido no repinta) y, si el manifiesto
  tiene `States[].Image` para ese índice, pinta esa imagen con la misma resolución que los
  placeholders (`Scene::image_of`). Si no resuelve, se ignora.
- **showOk / showAlert** de plugins y **`Feedback::{Ok,Alert}`** de builtin: superponen check verde
  o aspa roja y, 600 ms después, reenvían la imagen actual de la tecla. Dos pintados extra por
  feedback (overlay + vuelta), sin hilo que pinte al dispositivo.
- Invariante intacto: todo pasa por el mismo canal `(key, DynamicImage)` y el mismo lote/`batch_gap`;
  ningún hilo nuevo escribe al HID. Tests unitarios de composición en `app.rs`, sin hardware.
- Sigue ignorado en `app.rs` (no afecta al dibujo): `Output::{Settings, GlobalSettings, OpenUrl}`,
  títulos de más de 14 caracteres (se recortan con `…`) y estados sin imagen resoluble.
- Sin dependencias nuevas; `target/release/orcadeck.exe` = 3.219.456 B (3,07 MiB).
